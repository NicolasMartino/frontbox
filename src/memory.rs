//! A complete in-memory backend, with failure injection.
//!
//! This is a real implementation of all three storage traits, not a stub. It exists so the
//! conformance suite has something to run against before durable backends exist, and so an
//! application can be built and tested without one.
//!
//! # Why one backend holds every scope
//!
//! [`InMemoryBackend`] holds the rows for *all* scopes, and [`InMemoryBackend::open`] hands out a
//! scoped handle. Durable backends will usually give each scope its own database file, so it would
//! have been easier to model one backend as one scope.
//!
//! Sharing is the point. Scope enforcement has to hold when two scopes land in the *same* physical
//! store, because that is exactly the D5 hazard: a backend that turns a scope key into a storage
//! name by replacing characters is not injective, and `tenant/1` and `tenant_1` both become
//! `tenant_1`. If enforcement only worked because the files were separate, it would not be
//! enforcement.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::str::FromStr;

use chrono::{DateTime, Utc};

use crate::clock::Clock;
use crate::error::Error;
use crate::id::MutationId;
use crate::record::{
    DeadLetterRecord, MutationIntent, OperationMeta, OutboxRecord, QuarantinedRecord,
};
use crate::scope::ScopeKey;
use crate::store::{DeadLetterStore, Disposition, OutboxStore, Outcome, QuarantineStore};

pub use crate::clock::ManualClock;

/// An operation that [`InMemoryBackend::fail_next`] can be told to fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoreOp {
    /// [`OutboxStore::enqueue`]
    Enqueue,
    /// [`OutboxStore::pending_batch`]
    PendingBatch,
    /// [`OutboxStore::pending_count`]
    PendingCount,
    /// [`OutboxStore::sweep_corrupt`]
    SweepCorrupt,
    /// [`OutboxStore::apply_outcomes`]
    ApplyOutcomes,
    /// [`DeadLetterStore::list`]
    DeadLetterList,
    /// [`DeadLetterStore::count`]
    DeadLetterCount,
    /// [`DeadLetterStore::purge_older_than`]
    DeadLetterPurge,
    /// [`QuarantineStore::list`]
    QuarantineList,
    /// [`QuarantineStore::count`]
    QuarantineCount,
}

/// One stored outbox row, in the shape a durable backend would keep.
///
/// The identifier and body are held as raw text rather than parsed values, so a row that cannot be
/// decoded is representable. Without that there would be nothing for
/// [`OutboxStore::sweep_corrupt`] to find, and the corrupt-record path could not be tested at all.
#[derive(Debug, Clone)]
struct Row {
    raw_mutation_id: String,
    method: String,
    path: String,
    raw_body: String,
    created_at: i64,
    op: Option<OperationMeta>,
    scope: ScopeKey,
}

#[derive(Default)]
struct State {
    outbox: Vec<Row>,
    dead_letters: Vec<DeadLetterRecord>,
    quarantine: Vec<QuarantinedRecord>,
    pending_failures: HashMap<StoreOp, Error>,
}

/// Shared storage for every scope.
///
/// Cloning is cheap and shares state, so two clones are two handles on the same rows.
#[derive(Clone)]
pub struct InMemoryBackend {
    state: Rc<RefCell<State>>,
    clock: Rc<dyn Clock>,
}

impl std::fmt::Debug for InMemoryBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state.borrow();
        f.debug_struct("InMemoryBackend")
            .field("outbox", &state.outbox.len())
            .field("dead_letters", &state.dead_letters.len())
            .field("quarantine", &state.quarantine.len())
            .finish()
    }
}

impl InMemoryBackend {
    /// Build a backend over an injected clock.
    ///
    /// The clock supplies `rejected_at` and `quarantined_at`. It is held as a trait object, which
    /// is fine here and not a contradiction of the no-`dyn` rule: that rule is about the `async`
    /// traits, which are not dyn-compatible. [`Clock`] is synchronous.
    pub fn new(clock: impl Clock + 'static) -> Self {
        Self {
            state: Rc::new(RefCell::new(State::default())),
            clock: Rc::new(clock),
        }
    }

    /// Open a handle scoped to `scope`.
    pub fn open(&self, scope: ScopeKey) -> InMemoryStore {
        InMemoryStore {
            backend: self.clone(),
            scope,
        }
    }

    /// Make the next call to `op` fail with a storage error.
    ///
    /// Consumed by that call. Used to prove that a failing
    /// [`apply_outcomes`](OutboxStore::apply_outcomes) leaves no partial state.
    ///
    /// This backend cannot produce a genuinely torn write — it commits by replacing state in one
    /// assignment — so what the injection demonstrates is that the *contract* holds under a failing
    /// apply. Proving real transactional rollback is a durable-backend concern.
    pub fn fail_next(&self, op: StoreOp) {
        self.fail_next_with(op, Error::storage_opaque());
    }

    /// Make the next call to `op` fail with a specific error.
    pub fn fail_next_with(&self, op: StoreOp, error: Error) {
        self.state.borrow_mut().pending_failures.insert(op, error);
    }

    /// Write a row directly, bypassing validation.
    ///
    /// A test affordance: durable corruption cannot be produced through
    /// [`enqueue`](OutboxStore::enqueue), because a [`MutationIntent`] holds a parsed body and a
    /// parsed identifier. Corrupt rows arise from storage — a partial write, a schema change, a
    /// hand-edited database — so a test has to write one directly.
    pub fn insert_raw_row(
        &self,
        scope: &ScopeKey,
        raw_mutation_id: impl Into<String>,
        method: impl Into<String>,
        path: impl Into<String>,
        raw_body: impl Into<String>,
        created_at: i64,
    ) {
        self.state.borrow_mut().outbox.push(Row {
            raw_mutation_id: raw_mutation_id.into(),
            method: method.into(),
            path: path.into(),
            raw_body: raw_body.into(),
            created_at,
            op: None,
            scope: scope.clone(),
        });
    }

    /// How many outbox rows exist across every scope, decodable or not.
    ///
    /// Diagnostics only. No scoped API exposes this, by design: work retained under a scope nobody
    /// has opened is deliberately invisible, which is the accepted cost of not destroying offline
    /// writes when a user switches.
    pub fn total_rows(&self) -> usize {
        self.state.borrow().outbox.len()
    }

    fn take_failure(&self, op: StoreOp) -> Option<Error> {
        self.state.borrow_mut().pending_failures.remove(&op)
    }

    fn check(&self, op: StoreOp) -> Result<(), Error> {
        match self.take_failure(op) {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

/// A handle on one scope's records.
///
/// Implements [`OutboxStore`], [`DeadLetterStore`], and [`QuarantineStore`]. Because the latter two
/// both have `list` and `count`, call them through the trait when the receiver is concrete:
/// `DeadLetterStore::count(&store).await`.
#[derive(Clone, Debug)]
pub struct InMemoryStore {
    backend: InMemoryBackend,
    scope: ScopeKey,
}

impl InMemoryStore {
    /// The backend behind this handle.
    pub fn backend(&self) -> &InMemoryBackend {
        &self.backend
    }
}

/// Decode a stored row, or say why it cannot be decoded.
fn decode(row: &Row) -> Result<OutboxRecord, String> {
    let mutation_id = MutationId::from_str(&row.raw_mutation_id)
        .map_err(|_| format!("unparseable mutation_id: {:?}", row.raw_mutation_id))?;

    let body: serde_json::Value =
        serde_json::from_str(&row.raw_body).map_err(|e| format!("body is not valid JSON: {e}"))?;

    // The wire format renders this as an RFC 3339 instant, so a value chrono cannot represent is a
    // record that could never be sent. Catching it here makes it a quarantine case rather than a
    // serialization failure discovered mid-batch.
    if DateTime::<Utc>::from_timestamp_millis(row.created_at).is_none() {
        return Err(format!(
            "created_at outside representable range: {}",
            row.created_at
        ));
    }

    let mut intent = MutationIntent::new(
        mutation_id,
        row.method.clone(),
        row.path.clone(),
        body,
        row.created_at,
    );
    if let Some(op) = row.op.clone() {
        intent = intent.with_op(op);
    }
    Ok(OutboxRecord::stamp(intent, row.scope.clone()))
}

fn quarantine_from(row: &Row, reason: String, quarantined_at: i64) -> QuarantinedRecord {
    QuarantinedRecord {
        mutation_id: MutationId::from_str(&row.raw_mutation_id).ok(),
        raw_mutation_id: row.raw_mutation_id.clone(),
        method: row.method.clone(),
        path: row.path.clone(),
        raw_body: row.raw_body.clone(),
        created_at: row.created_at,
        op: row.op.clone(),
        scope: row.scope.clone(),
        reason,
        quarantined_at,
    }
}

impl OutboxStore for InMemoryStore {
    fn scope(&self) -> &ScopeKey {
        &self.scope
    }

    async fn enqueue(&self, intent: MutationIntent) -> Result<(), Error> {
        self.backend.check(StoreOp::Enqueue)?;

        let raw_body = serde_json::to_string(&intent.body).map_err(Error::serialization)?;
        let row = Row {
            raw_mutation_id: intent.mutation_id.to_string(),
            method: intent.method,
            path: intent.path,
            raw_body,
            created_at: intent.created_at,
            op: intent.op,
            // The store stamps its own scope. Nothing the caller passed can influence this.
            scope: self.scope.clone(),
        };
        self.backend.state.borrow_mut().outbox.push(row);
        Ok(())
    }

    async fn pending_batch(&self, limit: usize) -> Result<Vec<OutboxRecord>, Error> {
        self.backend.check(StoreOp::PendingBatch)?;

        let state = self.backend.state.borrow();
        let mut records: Vec<OutboxRecord> = state
            .outbox
            .iter()
            .filter(|row| row.scope == self.scope)
            .filter_map(|row| decode(row).ok())
            .collect();
        records.sort_by_key(OutboxRecord::order_key);
        records.truncate(limit);
        Ok(records)
    }

    async fn pending_count(&self) -> Result<usize, Error> {
        self.backend.check(StoreOp::PendingCount)?;

        let state = self.backend.state.borrow();
        Ok(state
            .outbox
            .iter()
            .filter(|row| row.scope == self.scope)
            .filter(|row| decode(row).is_ok())
            .count())
    }

    async fn sweep_corrupt(&self) -> Result<usize, Error> {
        self.backend.check(StoreOp::SweepCorrupt)?;

        let quarantined_at = self.backend.clock.now_ms();
        let mut state = self.backend.state.borrow_mut();

        let mut kept = Vec::with_capacity(state.outbox.len());
        let mut moved = Vec::new();
        for row in state.outbox.drain(..) {
            match (row.scope == self.scope, decode(&row)) {
                (true, Err(reason)) => moved.push(quarantine_from(&row, reason, quarantined_at)),
                _ => kept.push(row),
            }
        }

        state.outbox = kept;
        let count = moved.len();
        state.quarantine.extend(moved);
        Ok(count)
    }

    async fn apply_outcomes(&self, outcomes: &[Outcome]) -> Result<(), Error> {
        self.backend.check(StoreOp::ApplyOutcomes)?;

        let now = self.backend.clock.now_ms();
        let mut state = self.backend.state.borrow_mut();

        // Resolve every outcome against a record this store actually holds, before touching
        // anything. An outcome naming an unknown or out-of-scope id fails the whole call.
        let mut targets = Vec::with_capacity(outcomes.len());
        for outcome in outcomes {
            let index = state
                .outbox
                .iter()
                .position(|row| {
                    row.scope == self.scope && row.raw_mutation_id == outcome.id.to_string()
                })
                .ok_or_else(|| {
                    Error::protocol(format!(
                        "outcome for {} names no pending record in scope {}",
                        outcome.id, self.scope
                    ))
                })?;
            targets.push((index, outcome));
        }

        // Build the whole next state, then commit it in one step. Nothing is observable in
        // between, which is the property `apply_outcomes` promises implementors must provide.
        let mut removed = vec![false; state.outbox.len()];
        let mut new_dead_letters = Vec::new();
        let mut new_quarantine = Vec::new();

        for (index, outcome) in targets {
            let row = &state.outbox[index];
            match &outcome.disposition {
                Disposition::Retain => {}
                Disposition::Delete => removed[index] = true,
                Disposition::DeadLetter { error } => {
                    let record =
                        decode(row).map_err(|reason| Error::corrupt(outcome.id, reason))?;
                    new_dead_letters.push(DeadLetterRecord::from_record(
                        record,
                        now,
                        error.clone(),
                    ));
                    removed[index] = true;
                }
                Disposition::Quarantine { reason } => {
                    new_quarantine.push(quarantine_from(row, reason.clone(), now));
                    removed[index] = true;
                }
            }
        }

        let mut index = 0;
        state.outbox.retain(|_| {
            let keep = !removed[index];
            index += 1;
            keep
        });
        state.dead_letters.extend(new_dead_letters);
        state.quarantine.extend(new_quarantine);
        Ok(())
    }
}

impl DeadLetterStore for InMemoryStore {
    async fn list(&self, limit: usize) -> Result<Vec<DeadLetterRecord>, Error> {
        self.backend.check(StoreOp::DeadLetterList)?;

        let state = self.backend.state.borrow();
        let mut records: Vec<DeadLetterRecord> = state
            .dead_letters
            .iter()
            .filter(|record| record.scope == self.scope)
            .cloned()
            .collect();
        records.sort_by_key(|record| (record.rejected_at, record.mutation_id));
        records.truncate(limit);
        Ok(records)
    }

    async fn count(&self) -> Result<usize, Error> {
        self.backend.check(StoreOp::DeadLetterCount)?;

        let state = self.backend.state.borrow();
        Ok(state
            .dead_letters
            .iter()
            .filter(|record| record.scope == self.scope)
            .count())
    }

    async fn purge_older_than(&self, cutoff_ms: i64) -> Result<usize, Error> {
        self.backend.check(StoreOp::DeadLetterPurge)?;

        let mut state = self.backend.state.borrow_mut();
        let before = state.dead_letters.len();
        let scope = self.scope.clone();
        state
            .dead_letters
            .retain(|record| record.scope != scope || record.rejected_at >= cutoff_ms);
        Ok(before - state.dead_letters.len())
    }
}

impl QuarantineStore for InMemoryStore {
    async fn list(&self, limit: usize) -> Result<Vec<QuarantinedRecord>, Error> {
        self.backend.check(StoreOp::QuarantineList)?;

        let state = self.backend.state.borrow();
        let mut records: Vec<QuarantinedRecord> = state
            .quarantine
            .iter()
            .filter(|record| record.scope == self.scope)
            .cloned()
            .collect();
        records.sort_by_key(|record| (record.quarantined_at, record.raw_mutation_id.clone()));
        records.truncate(limit);
        Ok(records)
    }

    async fn count(&self) -> Result<usize, Error> {
        self.backend.check(StoreOp::QuarantineCount)?;

        let state = self.backend.state.borrow();
        Ok(state
            .quarantine
            .iter()
            .filter(|record| record.scope == self.scope)
            .count())
    }
}

/// A [`StoreFactory`](crate::testing::StoreFactory) over [`InMemoryBackend`].
///
/// One factory is one backend, so every scope it opens shares durable state — which is exactly the
/// sharing contract the conformance suite needs.
#[cfg(feature = "testing")]
#[derive(Clone, Debug)]
pub struct InMemoryFactory {
    backend: InMemoryBackend,
    clock: ManualClock,
}

#[cfg(feature = "testing")]
impl Default for InMemoryFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "testing")]
impl InMemoryFactory {
    /// Build a factory over a fresh backend and a clock at the epoch.
    pub fn new() -> Self {
        let clock = ManualClock::new(0);
        Self {
            backend: InMemoryBackend::new(clock.clone()),
            clock,
        }
    }

    /// The backend every store from this factory shares.
    pub fn backend(&self) -> &InMemoryBackend {
        &self.backend
    }
}

#[cfg(feature = "testing")]
impl crate::testing::StoreFactory for InMemoryFactory {
    type Store = InMemoryStore;

    fn clock(&self) -> ManualClock {
        self.clock.clone()
    }

    async fn open(&self, scope: ScopeKey) -> Result<Self::Store, Error> {
        Ok(self.backend.open(scope))
    }

    async fn insert_corrupt_row(
        &self,
        scope: &ScopeKey,
        kind: crate::testing::CorruptKind,
    ) -> Result<(), Error> {
        use crate::testing::CorruptKind;
        match kind {
            CorruptKind::UnparseableId => self.backend.insert_raw_row(
                scope,
                "not-a-uuid",
                "POST",
                "/api/v1/things",
                r#"{"ok":true}"#,
                1,
            ),
            CorruptKind::InvalidBody { id } => self.backend.insert_raw_row(
                scope,
                id.to_string(),
                "POST",
                "/api/v1/things",
                "{ not json",
                1,
            ),
            CorruptKind::UnrepresentableCreatedAt { id } => self.backend.insert_raw_row(
                scope,
                id.to_string(),
                "POST",
                "/api/v1/things",
                r#"{"ok":true}"#,
                i64::MAX,
            ),
        }
        Ok(())
    }
}

#[cfg(feature = "testing")]
impl crate::testing::FaultInjection for InMemoryFactory {
    async fn fail_next_apply_outcomes(&self) {
        self.backend.fail_next(StoreOp::ApplyOutcomes);
    }
}

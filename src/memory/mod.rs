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

use crate::cache::EntityState;
use crate::clock::Clock;
use crate::error::Error;
use crate::id::MutationId;
use crate::record::{
    DeadLetterRecord, MutationIntent, OperationMeta, OutboxRecord, QuarantinedRecord,
};
use crate::scope::ScopeKey;

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
    /// [`CacheVersionStore::state`](crate::cache::CacheVersionStore::state)
    CacheState,
    /// [`CacheVersionStore::all_states`](crate::cache::CacheVersionStore::all_states)
    CacheAllStates,
    /// [`CacheVersionStore::put`](crate::cache::CacheVersionStore::put)
    CachePut,
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
    /// Cache version state, keyed by scope and the entity's canonical string form.
    ///
    /// Keyed by `EntityKey::as_str` rather than by the key type, so that one backend can serve
    /// stores with different key types — and so that the storage layer sees exactly what a durable
    /// backend would see, which is the string. That is what makes the one-textual-form rule on
    /// `EntityKey` testable here rather than only at D5.
    versions: HashMap<(ScopeKey, String), EntityState>,
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

    /// Open a cache version store scoped to `scope`.
    ///
    /// Backed by the same state as [`open`](InMemoryBackend::open), so an outbox and a version
    /// store opened on one scope agree about that scope, and two scopes share physical storage —
    /// which is what makes the isolation cases prove something.
    pub fn open_versions(&self, scope: ScopeKey) -> InMemoryVersionStore {
        InMemoryVersionStore::new(self.clone(), scope)
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

    // The wire format renders this as an RFC 3339 instant, so a value that format cannot express is
    // a record that could never be sent. Catching it here makes it a quarantine case rather than a
    // serialization failure discovered mid-batch.
    if !crate::rfc3339::is_representable(row.created_at) {
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

#[cfg(feature = "testing")]
mod factory;
mod outbox;
mod reads;
mod versions;

pub use versions::InMemoryVersionStore;

#[cfg(feature = "testing")]
pub use factory::InMemoryFactory;

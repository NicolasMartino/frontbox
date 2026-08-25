//! A backend-agnostic conformance suite.
//!
//! Every storage backend must behave identically, so the behavioural tests live here rather than
//! next to any one backend. The in-memory backend runs them today; the durable SQLite and IndexedDB
//! backends will run the same functions, which is what makes "shared conformance tests pass on all
//! backends" a checkable claim rather than an aspiration.
//!
//! Enable the `testing` feature to use this module.
//!
//! ```ignore
//! frontbox::frontbox_conformance_tests! {
//!     #[test]
//!     factory: MyFactory::new(),
//!     block_on: pollster::block_on,
//! }
//! ```

use std::cell::RefCell;
use std::collections::VecDeque;

use crate::clock::ManualClock;
use crate::error::Error;
use crate::id::MutationId;
use crate::protocol::{
    MutationBatchRequest, MutationBatchResponse, MutationResult, MutationStatus, RemoteRejection,
};
use crate::record::MutationIntent;
use crate::scope::ScopeKey;
use crate::store::{DeadLetterStore, OutboxStore, QuarantineStore};
use crate::transport::SyncTransport;

pub mod cases;

/// A kind of durable corruption a backend must be able to reproduce.
///
/// Corruption cannot be created through [`OutboxStore::enqueue`] — a [`MutationIntent`] holds a
/// parsed body and a parsed identifier, so a malformed one is not representable. Corrupt rows come
/// from storage itself: a partial write, a schema change, an older version of the library. A
/// backend has to be able to manufacture one for the corrupt-record path to be testable at all.
#[derive(Debug, Clone, PartialEq)]
pub enum CorruptKind {
    /// A row whose identifier does not parse.
    ///
    /// This is the id-less path: nothing outside the backend can name this row, so only
    /// [`OutboxStore::sweep_corrupt`] can find it.
    UnparseableId,
    /// A row whose stored body is not valid JSON.
    InvalidBody {
        /// The row's identifier, which does parse.
        id: MutationId,
    },
    /// A row whose stored timestamp is outside the range the wire format can express.
    UnrepresentableCreatedAt {
        /// The row's identifier, which does parse.
        id: MutationId,
    },
}

/// Opens stores for the conformance suite.
///
/// # The sharing contract
///
/// Two stores opened from one factory under *different* scopes must share durable state. That is
/// what makes the cross-scope cases meaningful: they have to prove that scope enforcement holds
/// when both scopes live in the same physical store, not merely that separate files cannot see each
/// other. A durable factory honours this by pointing both stores at the same directory or origin.
#[allow(async_fn_in_trait)] // See the note on `SyncTransport`; decision 001.
pub trait StoreFactory {
    /// The backend under test.
    type Store: OutboxStore + DeadLetterStore + QuarantineStore;

    /// The clock the backend stamps `rejected_at` and `quarantined_at` from.
    ///
    /// Returned so a case can move time and assert the result, instead of asserting against
    /// whenever the test happened to run.
    fn clock(&self) -> ManualClock;

    /// Open a store on `scope`.
    async fn open(&self, scope: ScopeKey) -> Result<Self::Store, Error>;

    /// Write a corrupt row directly into `scope`'s storage.
    async fn insert_corrupt_row(&self, scope: &ScopeKey, kind: CorruptKind) -> Result<(), Error>;
}

/// A factory that can also make a specific store operation fail.
///
/// Split from [`StoreFactory`] on purpose. A backend that cannot inject faults simply does not
/// implement this and does not invoke [`frontbox_fault_injection_tests`], which leaves the gap
/// visible in its test file. Folding these cases into the main suite with a runtime "unsupported,
/// skipping" branch would let a backend report a full pass while never testing the atomicity
/// contract.
///
/// [`frontbox_fault_injection_tests`]: crate::frontbox_fault_injection_tests
#[allow(async_fn_in_trait)] // See the note on `SyncTransport`; decision 001.
pub trait FaultInjection: StoreFactory {
    /// Make the next `apply_outcomes` call fail.
    async fn fail_next_apply_outcomes(&self);
}

/// Build a scope key, panicking on an invalid one.
///
/// # Panics
///
/// If `key` is empty.
pub fn scope(key: &str) -> ScopeKey {
    ScopeKey::new(key).expect("conformance scope key must be valid")
}

/// A deterministic mutation id.
///
/// The suite never uses random identifiers: the ordering cases assert a tie-break *by*
/// `mutation_id`, so the identifiers have to be known and reproducible for the assertion to mean
/// anything.
pub fn id(n: u128) -> MutationId {
    MutationId::from_uuid(uuid::Uuid::from_u128(n))
}

/// A mutation intent with a deterministic id and a trivial body.
pub fn intent(n: u128, created_at: i64) -> MutationIntent {
    MutationIntent::new(
        id(n),
        "POST",
        format!("/api/v1/things/{n}"),
        serde_json::json!({ "seq": n }),
        created_at,
    )
}

/// What a [`ScriptedTransport`] answers with.
#[derive(Debug, Clone)]
pub enum Reply {
    /// Give every mutation in the batch the same status.
    All(MutationStatus),
    /// Give the mutation at each position the matching status.
    ///
    /// Positions beyond the end of the list get no result at all, which is a server declining to
    /// rule on them.
    ByPosition(Vec<MutationStatus>),
    /// Answer with exactly these results, whatever the batch contained.
    ///
    /// The only way to produce a verdict for a mutation that was never sent.
    Exact(Vec<MutationResult>),
    /// No request could be attempted.
    Offline,
    /// A request was attempted and failed.
    TransportFailure,
}

/// A transport that answers from a script and records what it was asked.
pub struct ScriptedTransport {
    default: Reply,
    scripted: RefCell<VecDeque<Reply>>,
    sent: RefCell<Vec<MutationBatchRequest>>,
    yielding: std::cell::Cell<bool>,
}

impl ScriptedTransport {
    /// Build a transport that answers every batch with `default`.
    pub fn new(default: Reply) -> Self {
        Self {
            default,
            scripted: RefCell::new(VecDeque::new()),
            sent: RefCell::new(Vec::new()),
            yielding: std::cell::Cell::new(false),
        }
    }

    /// Suspend once before answering.
    ///
    /// A sync pass that never yields cannot be observed mid-flight, so the re-entrancy case needs a
    /// transport that actually gives the executor a chance to poll something else.
    #[must_use]
    pub fn yielding(self) -> Self {
        self.yielding.set(true);
        self
    }

    /// Queue a reply for the next batch, ahead of the default.
    #[must_use]
    pub fn then(self, reply: Reply) -> Self {
        self.scripted.borrow_mut().push_back(reply);
        self
    }

    /// Every batch this transport was asked to send, in order.
    pub fn sent(&self) -> Vec<MutationBatchRequest> {
        self.sent.borrow().clone()
    }

    /// How many batches this transport was asked to send.
    pub fn send_count(&self) -> usize {
        self.sent.borrow().len()
    }
}

impl SyncTransport for ScriptedTransport {
    async fn send_batch(
        &self,
        request: MutationBatchRequest,
    ) -> Result<MutationBatchResponse, Error> {
        self.sent.borrow_mut().push(request.clone());

        if self.yielding.get() {
            YieldOnce(false).await;
        }

        let reply = self
            .scripted
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| self.default.clone());

        let results = match reply {
            Reply::Offline => return Err(Error::Offline),
            Reply::TransportFailure => return Err(Error::transport_opaque()),
            Reply::Exact(results) => results,
            Reply::All(status) => request
                .mutations
                .iter()
                .map(|m| MutationResult::new(m.mutation_id, status))
                .collect(),
            Reply::ByPosition(statuses) => request
                .mutations
                .iter()
                .zip(statuses)
                .map(|(m, status)| MutationResult::new(m.mutation_id, status))
                .collect(),
        };
        Ok(MutationBatchResponse { results })
    }
}

/// A future that is pending exactly once.
struct YieldOnce(bool);

impl std::future::Future for YieldOnce {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if self.0 {
            std::task::Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    }
}

/// A rejection payload with every field populated, for round-trip assertions.
pub fn full_rejection() -> RemoteRejection {
    RemoteRejection::new("title must not be empty")
        .with_code("invalid_input")
        .with_details(serde_json::json!({ "field": "title", "limits": { "min": 1 } }))
}

/// Emit one test per conformance case.
///
/// `factory` is evaluated once per case. `block_on` runs the returned future — this crate has no
/// runtime of its own, so the caller supplies one.
///
/// ```ignore
/// frontbox::frontbox_conformance_tests! {
///     #[test]
///     factory: MyFactory::new(),
///     block_on: pollster::block_on,
/// }
/// ```
#[macro_export]
macro_rules! frontbox_conformance_tests {
    ($(#[$attr:meta])* factory: $factory:expr, block_on: $block_on:path $(,)?) => {
        $crate::__frontbox_emit_cases! {
            attr: [$(#[$attr])*],
            factory: $factory,
            block_on: $block_on,
            cases: [
                case_01_applied_is_deleted,
                case_02_duplicate_is_deleted,
                case_03_rejected_becomes_a_dead_letter,
                case_04_blocked_stays_queued,
                case_05_pending_stays_queued,
                case_06_mixed_batch_applies_every_status,
                case_07_offline_leaves_work_untouched,
                case_08_transport_failure_is_an_attempted_send,
                case_10_pending_batch_respects_limit,
                case_11_ordering_is_oldest_first,
                case_12_same_timestamp_orders_stably_by_id,
                case_13_rejected_at_comes_from_the_clock,
                case_14_purge_uses_the_supplied_cutoff,
                case_15_corrupt_record_leaves_the_pending_total,
                case_16_unknown_result_is_an_anomaly,
                case_17_rejection_payload_round_trips,
                case_18_sweep_finds_rows_outcomes_cannot_name,
                case_19_blocked_clears_on_the_next_sync,
                case_20_fully_retained_batch_is_no_progress,
                case_21_pending_prefix_starves_the_tail_observably,
                case_22_scopes_cannot_observe_each_other,
                case_23_empty_scope_key_is_rejected,
                case_24_operation_meta_survives_every_transition,
                case_25_invalid_json_cannot_enter_the_outbox,
                case_26_unrepresentable_timestamp_is_corruption_not_panic,
                case_27_wire_payload_matches_the_source_protocol,
                case_28_reentrant_sync_does_not_double_send,
                case_29_scope_keys_are_not_normalized,
            ]
        }
    };
}

/// Emit the conformance cases that need fault injection.
///
/// Separate from [`frontbox_conformance_tests`] so that a backend which cannot inject faults leaves
/// a visible gap rather than a silent pass.
#[macro_export]
macro_rules! frontbox_fault_injection_tests {
    ($(#[$attr:meta])* factory: $factory:expr, block_on: $block_on:path $(,)?) => {
        $crate::__frontbox_emit_cases! {
            attr: [$(#[$attr])*],
            factory: $factory,
            block_on: $block_on,
            cases: [
                case_09_failed_apply_leaves_no_partial_state,
            ]
        }
    };
}

// Emitted one case at a time rather than with a single `$(...)*` over the case list, because the
// attributes and the case names repeat at different depths and macro_rules cannot nest those.
#[doc(hidden)]
#[macro_export]
macro_rules! __frontbox_emit_cases {
    (
        attr: [$($attr:tt)*],
        factory: $factory:expr,
        block_on: $block_on:path,
        cases: []
    ) => {};

    (
        attr: [$($attr:tt)*],
        factory: $factory:expr,
        block_on: $block_on:path,
        cases: [$case:ident $(, $rest:ident)* $(,)?]
    ) => {
        $($attr)*
        fn $case() {
            $block_on($crate::testing::cases::$case(&$factory))
                .expect(concat!("conformance case failed: ", stringify!($case)));
        }

        $crate::__frontbox_emit_cases! {
            attr: [$($attr)*],
            factory: $factory,
            block_on: $block_on,
            cases: [$($rest),*]
        }
    };
}

//! Durable storage traits.
//!
//! Every read on every trait here is scoped. A record written under one [`ScopeKey`] is never
//! returned, never batched, and never counted by a store opened under another
//! (`wiki/decisions/009-local-scope-identity.decision.md`). Enforcement lives here, on the read
//! path, because that is where it has to be: fresh per-send authentication is correct and
//! insufficient — a valid token for one principal applied to another principal's queued record
//! produces an *authorized* wrong write.

use crate::error::Error;
use crate::record::{MutationIntent, OutboxRecord};
use crate::scope::ScopeKey;

mod coalescing;
mod outcome;
mod rows;
mod terminal;

pub use coalescing::{CoalescingEnqueue, CoalescingPolicy, CoalescingRefusal};
pub use outcome::{Disposition, Outcome};
pub use rows::RowStore;
pub use terminal::{DeadLetterStore, QuarantineStore};

/// Exclusive drain rights for one scope, held for the length of one pass.
///
/// Released on drop, whatever ends the pass — returning, erroring, or being cancelled. Core never
/// looks inside: a backend puts whatever it needs to release in the closure, and a backend with
/// nothing to release supplies none.
pub struct DrainLease(Option<Box<dyn FnOnce()>>);

impl DrainLease {
    /// A lease with nothing to release.
    ///
    /// What the default [`claim_drain`](OutboxStore::claim_drain) hands back, and what a
    /// single-realm backend should return.
    #[must_use]
    pub fn granted() -> Self {
        Self(None)
    }

    /// A lease that runs `release` when it is dropped.
    #[must_use]
    pub fn held(release: impl FnOnce() + 'static) -> Self {
        Self(Some(Box::new(release)))
    }
}

impl Drop for DrainLease {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            release();
        }
    }
}

impl std::fmt::Debug for DrainLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0 {
            Some(_) => "DrainLease(held)",
            None => "DrainLease(granted)",
        })
    }
}

/// The pending mutation queue.
///
/// # One drain per scope
///
/// **At most one drain may be in flight per scope, across every handle on it.** Core holds that
/// within a realm: [`SyncRunner::sync_once`](crate::runner::SyncRunner::sync_once) claims the
/// scope rather than the runner, so a second handle opened under the same key reports
/// [`SyncPass::AlreadyRunning`](crate::runner::SyncPass) instead of sending. Conformance case 61
/// asserts it.
///
/// **A backend whose storage can be opened from more than one realm must extend that exclusion
/// across realms.** Core cannot: a second browser tab is a second wasm instance with its own
/// memory, and nothing in this process is visible to it. Two tabs on one origin are two realms over
/// one IndexedDB database, each with its own runner, drawing from one queue — which needs nobody to
/// make a mistake. The mechanism is the backend's, because only the backend knows its concurrency
/// model: Web Locks (`navigator.locks.request`, available in window *and* worker scopes) for
/// IndexedDB, usually nothing at all for SQLite in a one-process native application.
///
/// A drain pass is *read the batch* → *POST* → *apply outcomes*: two transactions with a network
/// round trip between them. **Transactional atomicity cannot supply this**, because the two
/// transactions never overlap — whatever excludes the second drainer has to outlive a transaction.
///
/// What a violation costs is specific, and worth knowing before treating this as optional.
/// Idempotency survives, because the server dedupes on `mutation_id`. Ordering does not: at
/// `batch_limit = 1` the second drainer ships record 2 while record 1 is still in flight, which is
/// the guarantee [`read_for_send`](OutboxStore::read_for_send)'s order key exists to provide. The
/// retention bound does not either, since two drainers spend one record's `attempts` budget in
/// parallel and a wedged head reaches its bound in half the wall clock the bound was chosen to
/// represent (`wiki/decisions/031-cross-realm-single-flight.decision.md`).
///
/// Case 61 cannot enforce this half: a conformance suite drives a backend through one process, and
/// a second realm is not something a case can open. It is stated here for the same reason decision
/// 024's injectivity obligation is stated on [`ScopeKey`] — the test pins the half it can reach,
/// and the prose has to carry the rest.
#[allow(async_fn_in_trait)] // See the note on `SyncTransport`; decision 001.
pub trait OutboxStore {
    /// The scope this store was opened under.
    fn scope(&self) -> &ScopeKey;

    /// Claim exclusive drain rights for this scope, for as long as the returned lease lives.
    ///
    /// `Ok(None)` means someone else holds them and this pass must stand down — the runner turns
    /// that into [`SyncPass::AlreadyRunning`](crate::runner::SyncPass::AlreadyRunning), exactly as
    /// it does for the in-process claim.
    ///
    /// # Why this exists, and why it defaults to granted
    ///
    /// Decision 031's in-realm half is core's and is already built: a scope registry means two
    /// handles in one process cannot drain together. **The cross-realm half is not reachable from
    /// core at all** — two browser tabs are two wasm instances with two registries, and neither can
    /// see the other. Only something with a view of the storage layer can arbitrate, which is the
    /// backend.
    ///
    /// So core states the obligation here and names no mechanism, exactly as decisions 024 and 025
    /// do for storage naming and quarantine. The default grants immediately, which is the honest
    /// answer for a backend that has one realm by construction: an in-memory store dies with its
    /// process, and a SQLite file is normally opened by one.
    ///
    /// # What a lease must guarantee
    ///
    /// It is released when dropped, including on cancellation — a pass abandoned mid-flight must
    /// not leave a scope claimed forever, which is the failure
    /// [`case_30`](crate::testing::cases::case_30_a_cancelled_pass_does_not_wedge_the_runner)
    /// pins down for the in-process claim.
    async fn claim_drain(&self) -> Result<Option<DrainLease>, Error> {
        Ok(Some(DrainLease::granted()))
    }

    /// Queue a mutation, stamping it with this store's scope.
    async fn enqueue(&self, intent: MutationIntent) -> Result<(), Error>;

    /// Read up to `limit` pending records in enqueue order, by
    /// [`order_key`](crate::record::OutboxRecord::order_key).
    ///
    /// **Inspection, not the drain handoff.** [`SyncRunner`](crate::runner::SyncRunner) sends what
    /// [`read_for_send`](OutboxStore::read_for_send) returns, which is this read plus a durable
    /// mark. Use this one to look at the queue without changing it — a pending-work view, a
    /// diagnostic, a test. Everything below about ordering, scope, and undecodable rows applies to
    /// both.
    ///
    /// **The order is the sequence the store assigned at enqueue**, not the client clock. D1 and D2
    /// ordered by `(created_at, mutation_id)`, which was total and reproducible but not faithful:
    /// `created_at` is a clock reading, and same-millisecond ties broke on a random
    /// [`MutationId`](crate::id::MutationId), so three rapid writes could be drained in an order the
    /// application never wrote in — a set before the session it belongs to. Under batching the
    /// server received the whole batch and evaluated it in that same wrong order; at
    /// `batch_limit = 1` a wrong order is a wrong write
    /// (`wiki/decisions/016-monotonic-enqueue-sequence.decision.md`).
    ///
    /// This buys faithfulness, not causality. It replays the order the caller enqueued in; it
    /// cannot make that order correct if the caller enqueued a child before its parent.
    ///
    /// Records stamped with a different [`ScopeKey`] are never returned, and neither are rows that
    /// will not decode — see [`sweep_corrupt`](OutboxStore::sweep_corrupt).
    ///
    /// `limit` bounds the blast radius of any one server verdict. The source loads the entire
    /// outbox in one batch, so a single terminal rejection can block every later correlated
    /// mutation at once.
    async fn pending_batch(&self, limit: usize) -> Result<Vec<OutboxRecord>, Error>;

    /// Count decodable pending records in this store's scope.
    ///
    /// Quarantined rows are excluded and reported by [`QuarantineStore::count`], so a wedged record
    /// can never hide inside a pending total. Records under another scope are never counted.
    ///
    /// Undecodable rows that have not yet been swept are not counted either. See
    /// [`sweep_corrupt`](OutboxStore::sweep_corrupt) for why reads behave this way and what a
    /// caller has to do about it.
    async fn pending_count(&self) -> Result<usize, Error>;

    /// Move rows that cannot be decoded into quarantine, returning how many moved.
    ///
    /// This is the id-less path. A row whose `mutation_id` is missing or unparseable cannot be
    /// named by an [`Outcome`], so nothing outside the backend can find it — and a row that is
    /// silently skipped on every read stays in storage, keeps occupying space, is never sent, is
    /// never deleted, and never reaches a user-visible view. The source has five such
    /// silent-drop paths.
    ///
    /// # Reads skip corruption; only this surfaces it
    ///
    /// [`pending_batch`](OutboxStore::pending_batch) returns `OutboxRecord` values and
    /// [`pending_count`](OutboxStore::pending_count) counts them, so neither can represent a row
    /// that will not decode. Between a row becoming corrupt and a sweep finding it, that row is
    /// invisible to every read — the same silence this crate criticises the source for, narrowed to
    /// a window rather than made permanent.
    ///
    /// Two alternatives were considered and are worse. Sweeping inside the read makes a read
    /// mutate storage, which is surprising and rules out read-only transactions. Returning
    /// [`Error::CorruptRecord`] from a read lets one bad row block every read, wedging a queue that
    /// is otherwise healthy — the outcome decision 006 exists to prevent.
    ///
    /// So the contract is: **a caller that reads without ever syncing must call this itself.**
    /// [`SyncRunner`](crate::runner::SyncRunner) sweeps at the start of every pass *that reads* and
    /// reports the count in [`SyncReport::quarantined`](crate::runner::SyncReport::quarantined), so
    /// an application that syncs while connected closes the window on its own.
    ///
    /// **A pass that ends at
    /// [`offline_now`](crate::transport::SyncTransport::offline_now) does not sweep**, because it
    /// touches storage not at all — that is the property it exists to have. An application offline
    /// for a long stretch therefore keeps any corrupt rows invisible until it next reaches the
    /// network, or until it calls this itself. The window is bounded by connectivity rather than by
    /// the poll interval, which is a weaker bound than the one this paragraph used to state.
    async fn sweep_corrupt(&self) -> Result<usize, Error>;

    /// Apply every outcome atomically.
    ///
    /// Deletions, dead-letter inserts, and quarantine transitions either all commit or none do.
    /// The source performs this as an insert followed by a separate delete whose result it
    /// discards; when the delete fails, the record is both pending and dead-lettered, gets re-sent
    /// on every subsequent sync, and its `rejected_at` keeps moving forward so retention never
    /// purges it. The trait makes that intermediate state unrepresentable for implementors.
    ///
    /// # What the outcome set must satisfy
    ///
    /// Every outcome must name a record this store holds in its own scope, and **no id may appear
    /// twice**. A store handed an unknown id, an out-of-scope id, or a repeated id returns
    /// [`Error::Protocol`] and commits nothing.
    ///
    /// The duplicate rule is not pedantry. Two outcomes for one id resolve to the same record, so a
    /// pair of `DeadLetter` dispositions writes two dead letters for one row and the dead-letter
    /// count stops matching reality; a `Delete` paired with a `DeadLetter` has no defensible
    /// winner. Rejecting the set is the only answer that cannot silently corrupt a count.
    ///
    /// [`SyncRunner`](crate::runner::SyncRunner) satisfies both rules already: it drops verdicts
    /// for ids it did not send, and drops *every* verdict for an id the same response named more
    /// than once, reporting both kinds as anomalies. A store therefore never has to arbitrate — but
    /// it still has to reject, because `apply_outcomes` is public and a backend cannot assume the
    /// runner is its only caller.
    ///
    /// This rule is uniform across backends. Conformance case 31 asserts it, so the in-memory
    /// store, SQLite, and IndexedDB all fail the suite if any of them silently applies one of a
    /// repeated pair instead.
    async fn apply_outcomes(&self, outcomes: &[Outcome]) -> Result<(), Error>;

    /// Queue a mutation, replacing one safe same-row queued write instead of appending behind it.
    ///
    /// The opt-in half of `enqueue`. Where a caller has a newer body for a write that is still
    /// queued and still unsent, this replaces that record's body in place rather than adding a
    /// second one — so two offline edits to one row leave one eventual send, carrying the later body
    /// under the earlier record's identifier and precondition.
    ///
    /// # When a replacement happens
    ///
    /// All of these, or the call falls back to what [`policy`](CoalescingPolicy) says:
    ///
    /// - the intent carries a [`RowRef`](crate::record::RowRef);
    /// - exactly one decodable pending record in this scope has the same `RowRef`, method, and path;
    /// - that record is **not transport-started**.
    ///
    /// # Transport-started is one durable fact with two writers
    ///
    /// The last condition is what makes this safe. It is a durable per-record boolean, never
    /// cleared, and **both** of the ways a record can reach the server set it:
    ///
    /// - [`read_for_send`](OutboxStore::read_for_send) sets it on every record it returns, in the
    ///   same transaction, before the request is built;
    /// - [`apply_outcomes`](OutboxStore::apply_outcomes) sets it when it applies a
    ///   [`Disposition::Retain`], because a verdict cannot exist without a request.
    ///
    /// The second writer is not redundant. This trait is public and a backend cannot assume the
    /// runner is its only caller: a caller may read with [`pending_batch`](OutboxStore::pending_batch),
    /// send that batch through its own transport, and apply the verdicts itself, never once touching
    /// `read_for_send`. Phrasing eligibility as "not read for sending" would call such a record
    /// coalescible after the server has certainly seen it — the hole review found in the first
    /// implementation, and what conformance case 80 now pins shut.
    ///
    /// Eligibility is also deliberately *not* `attempts == 0`.
    /// [`attempts`](crate::record::OutboxRecord::attempts) counts verdicts **received**, so an
    /// offline pass and an attempted transport failure both leave it at zero after the record may
    /// already have reached the server (`wiki/decisions/017-bounded-retention.decision.md`).
    ///
    /// # What survives, and what the new intent supplies
    ///
    /// The queued slot and its guard are the queue's; the content is the caller's latest.
    ///
    /// | Kept from the queued record | Taken from the new intent |
    /// | --- | --- |
    /// | `seq`, `mutation_id`, `precondition` | `body`, `op`, `traceparent`, `created_at` |
    ///
    /// `seq` keeps the record where decision 016 put it, `mutation_id` keeps idempotency stable, and
    /// the precondition is the whole point — it names the last state the server confirmed, which is
    /// the only state a conflict can honestly be detected against
    /// (`wiki/decisions/026-replayable-preconditions.decision.md`).
    ///
    /// Everything in the right-hand column *describes the body*, and the body is being replaced.
    /// Keeping the old [`op`](crate::record::OperationMeta) would put a stale operation name and a
    /// stale `version` — documented as "what wrote the body" — over a body that did not write it,
    /// on the surface a human reads after a refusal. Keeping the old `traceparent` would point the
    /// trace at the user action whose body was discarded, which is the link decision 022 exists to
    /// preserve. Keeping the old `created_at` would send the newer body stamped with the older
    /// write's `client_datetime`. None of the four is ordered on, because `seq` is the only sort key.
    ///
    /// `attempts` and `last_error` need no rule: a record that is not transport-started has received
    /// no verdict, so they are already `0` and `None`.
    ///
    /// # Replacement discards the queued body
    ///
    /// **Opt in only where your bodies carry full row state.** For a full-document `PUT` this is
    /// last-write-wins and correct. For a partial update it is not: coalescing two `PATCH` bodies
    /// drops every field the first changed that the second does not mention.
    ///
    /// The obligation is stated here rather than left to the caller because the caller cannot always
    /// discharge it. Under [`CoalescingPolicy::RequireExisting`] the application does not know what
    /// body is queued — not knowing is the reason it cannot compute a precondition — so it is in no
    /// position to work out what a replacement would throw away.
    ///
    /// # Atomicity
    ///
    /// One transaction: either exactly one record is replaced, exactly one is appended, or nothing
    /// is written. It must serialize against [`read_for_send`](OutboxStore::read_for_send), or a
    /// replacement could land against a record whose old body is already on its way to the server.
    /// A backend must not make the eligibility check a read-only transaction for this reason —
    /// IndexedDB runs those concurrently with read-write ones.
    ///
    /// # Errors
    ///
    /// A storage failure, or a body that will not serialize. A refusal is **not** an error: it is
    /// [`CoalescingEnqueue::NotQueued`], and only [`CoalescingPolicy::RequireExisting`] produces one.
    async fn enqueue_coalescing(
        &self,
        intent: MutationIntent,
        policy: CoalescingPolicy,
    ) -> Result<CoalescingEnqueue, Error>;

    /// Read up to `limit` pending records for sending, **marking each as transport-started in the
    /// same transaction**.
    ///
    /// The drain handoff. [`pending_batch`](OutboxStore::pending_batch) is the same read without the
    /// mark, and is inspection only — [`SyncRunner`](crate::runner::SyncRunner) calls this instead,
    /// so the ordering guarantee `pending_batch` documents is delivered here.
    ///
    /// The mark is what [`enqueue_coalescing`](OutboxStore::enqueue_coalescing) tests against, and it
    /// is never cleared. That asymmetry is the design:
    ///
    /// - **Set before the request, not after it.** A pass that ends in [`Error::Offline`] cannot
    ///   prove no request left the device — `src/transport.rs` tells implementors to return it when
    ///   a browser `fetch` fails for lack of connectivity, and such a `fetch` rejects identically
    ///   whether the request never went out or reached the server and lost its response. A body
    ///   rewritten under that uncertainty is a body the server dedupes away and never applies.
    /// - **Never cleared, so nothing has to be recovered.** A crash, a cancelled future, or a lost
    ///   response all leave the mark already set, which is the conservative reading each of them
    ///   needs. There is no lease to release and no abandoned state to sweep.
    ///
    /// The cost is that a pass which reads a batch spends that batch's coalescibility whether or not
    /// anything was sent. [`SyncTransport::offline_now`](crate::transport::SyncTransport::offline_now)
    /// is what keeps an offline application from paying it on every poll: the runner asks before it
    /// reads, so a client that knows it is offline never reaches this method.
    ///
    /// Ordering, scope filtering, and the treatment of undecodable rows are exactly
    /// [`pending_batch`](OutboxStore::pending_batch)'s — see there.
    async fn read_for_send(&self, limit: usize) -> Result<Vec<OutboxRecord>, Error>;
}

//! Which scopes have a drain in flight, in this realm.
//!
//! Single-flight used to be a `Cell<bool>` on [`SyncRunner`](super::SyncRunner), which excluded a
//! second pass *by that runner* and nothing else. Two handles opened on one scope are two views of
//! one queue, so two runners over them each observed their own flag false and both sent the same
//! batch. Idempotency absorbed the double send — the server dedupes on `mutation_id` — but ordering
//! and the retention bound did not: at `batch_limit = 1` the second drainer ships record 2 while
//! record 1 is still in flight, and both spend the same `attempts` budget in parallel
//! (`wiki/decisions/031-cross-realm-single-flight.decision.md`).
//!
//! The claim is keyed by [`ScopeKey`] rather than by store identity, because a scope is a
//! principal's queue rather than a handle on one (`wiki/decisions/009-local-scope-identity.decision.md`).
//! Two runners over two *different* backends that were opened under one key exclude each other, and
//! that is the intended reading: if they are not the same queue, the key was not injective.
//!
//! # What one realm can and cannot see
//!
//! This closes the in-process hole completely. It cannot close the cross-realm one: two browser tabs
//! on one origin are two realms over one IndexedDB database, and nothing in this process is visible
//! to the other. That half is an obligation on durable backends, stated on
//! [`OutboxStore`](crate::store::OutboxStore) and discharged with a mechanism that outlives a
//! realm — Web Locks for IndexedDB.

use std::cell::RefCell;
use std::collections::HashSet;

use crate::scope::ScopeKey;

thread_local! {
    /// The scopes currently draining on this thread.
    ///
    /// A `thread_local` rather than a global: every future in this crate is `!Send` by design
    /// (decision 001), so a scope draining on another thread is a queue this one cannot reach
    /// anyway, and a shared map would need a lock that the single-threaded targets would pay for
    /// and never use.
    static DRAINING: RefCell<HashSet<ScopeKey>> = RefCell::new(HashSet::new());
}

/// Holds one scope's drain slot for as long as it lives.
///
/// Released on drop, which is the only release path: a pass that returns early, returns an error,
/// or is *cancelled* frees the scope on the way out. Cancellation is routine on the runtimes this
/// crate targets — a Dioxus `use_future` is dropped whenever its component re-renders — and a slot
/// released only on the success path would leave every later pass reporting
/// [`SyncPass::AlreadyRunning`](super::SyncPass) forever.
pub(super) struct DrainClaim(ScopeKey);

impl DrainClaim {
    /// Claim `scope`, or `None` if a drain already holds it.
    pub(super) fn try_acquire(scope: &ScopeKey) -> Option<Self> {
        DRAINING
            .try_with(|slots| slots.borrow_mut().insert(scope.clone()))
            // Thread-local storage is already torn down, so nothing on this thread can still be
            // draining. Refusing the claim here would report `AlreadyRunning` against a drain that
            // cannot exist.
            .unwrap_or(true)
            .then(|| Self(scope.clone()))
    }
}

impl Drop for DrainClaim {
    fn drop(&mut self) {
        // `try_with` rather than `with`: this runs in a `Drop`, and `with` panics once the
        // thread-local has been destroyed.
        let _ = DRAINING.try_with(|slots| {
            slots.borrow_mut().remove(&self.0);
        });
    }
}

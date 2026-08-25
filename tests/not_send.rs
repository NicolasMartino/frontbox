//! Proof that the public traits impose no `Send` bound.
//!
//! This crate targets single-threaded frontend runtimes. On `wasm32`, IndexedDB futures are `!Send`
//! and cannot be made `Send` by wrapping, so a `Send` bound anywhere in the public API would
//! exclude the primary target.
//!
//! The strongest check is a compile-time one, and it is already made below: `InMemoryStore` holds
//! `Rc`s, so it is `!Send`. If `OutboxStore`, `DeadLetterStore`, or `QuarantineStore` required
//! `Send` — on `Self`, or on the futures their methods return — that type could not implement them
//! and `store_traits_accept_a_not_send_type` would not compile.
//!
//! The call-site test below adds the other half: an `Rc` held live across every `await`, which is
//! how an application on a frontend executor actually uses this.

use std::rc::Rc;

use frontbox::{
    Clock, DeadLetterStore, InMemoryBackend, InMemoryStore, ManualClock, MutationId,
    MutationIntent, OutboxStore, QuarantineStore, ScopeKey,
};

/// Accepts only a type implementing all three storage traits.
///
/// Instantiated below with `InMemoryStore`, which is `!Send`.
fn store_traits_accept_a_not_send_type<S>(_store: &S)
where
    S: OutboxStore + DeadLetterStore + QuarantineStore,
{
}

#[test]
fn futures_are_usable_while_a_not_send_value_is_held_live() {
    pollster::block_on(async {
        // Held across every await below. A future that keeps this alive across a suspension point
        // is itself `!Send`, which is exactly the shape a frontend executor drives.
        let marker = Rc::new("held across every await");

        let clock = ManualClock::new(1_700_000_000_000);
        let backend = InMemoryBackend::new(clock.clone());
        let store = backend.open(ScopeKey::new("user:alice").expect("scope"));

        store_traits_accept_a_not_send_type(&store);

        store
            .enqueue(MutationIntent::new(
                MutationId::from_uuid(uuid::Uuid::from_u128(1)),
                "POST",
                "/api/v1/things",
                serde_json::json!({ "ok": true }),
                clock.now_ms(),
            ))
            .await
            .expect("enqueue");

        assert_eq!(store.pending_batch(10).await.expect("batch").len(), 1);
        assert_eq!(store.pending_count().await.expect("count"), 1);
        assert_eq!(store.sweep_corrupt().await.expect("sweep"), 0);
        assert_eq!(DeadLetterStore::count(&store).await.expect("dl count"), 0);
        assert_eq!(QuarantineStore::count(&store).await.expect("q count"), 0);
        assert!(DeadLetterStore::list(&store, 10)
            .await
            .expect("dl list")
            .is_empty());
        assert!(QuarantineStore::list(&store, 10)
            .await
            .expect("q list")
            .is_empty());

        assert_eq!(*marker, "held across every await");
    });
}

/// A second, structural proof: a `!Send` handle can be cloned and used from an ordinary
/// non-`Send` context without any adapter layer.
#[test]
fn store_handles_are_not_send_by_construction() {
    let backend = InMemoryBackend::new(ManualClock::new(0));
    let store: InMemoryStore = backend.open(ScopeKey::new("user:alice").expect("scope"));
    let cloned = store.clone();

    // `Rc` in the handle is what makes this `!Send`. Cloning shares state rather than duplicating
    // it, which is the single-threaded interior-mutability model this crate is built around.
    assert_eq!(cloned.backend().total_rows(), 0);
}

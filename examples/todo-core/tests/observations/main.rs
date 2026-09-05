//! The D4a trial's observations, as tests rather than as a thing to look at.
//!
//! Each one is a falsifiable claim from `wiki/proposals/offline-todo-trial.proposal.md` §5, run
//! against a real axum/sqlx server over real HTTP. The server does not depend on `frontbox` and
//! writes its own wire shapes from the spec, so **these tests are the wire-format oracle**: if the
//! two ends disagree by one field name, every one of them fails.
//!
//! Observation 3 — reload mid-queue and the work survives — was the one D4a could not make, and it
//! had a test anyway, asserting the opposite so the gap was proven rather than claimed. **D4b
//! inverted it.** The test below now opens a real SQLite file, drops the application holding it,
//! reopens, and finds the write still queued. The inversion is the deliverable; the fact that it
//! is the same test, renamed in neither direction, is what makes it evidence.

// `#[path]` because the facade moved one directory down; the helpers stay where every suite finds
// them.
#[path = "../common/mod.rs"]
mod common;

use frontbox::SyncRunner;

// Split where the claims split: what a queue does unreached, and what happens once answered.
mod coalescing;
mod queue;
mod verdicts;

/// The runner is `!Send`, and the trial's application holds one without ever needing otherwise.
///
/// A compile-time assertion rather than a runtime one. It exists because the whole `!Send` posture
/// (decision 001) is justified by browser storage, and D4a is the first place an application has
/// actually had to live with it.
///
/// # Why two impls prove an absence
///
/// A bound cannot be asserted away directly — `T: !Send` is not a thing one can write. So the
/// property is turned into an inference question. Two blanket impls of one trait are offered: one
/// for every `T`, one only for `T: Send`. If the type is `Send`, **both** apply, `A` is ambiguous,
/// and the call below fails to compile. If it is not `Send`, exactly one applies and `A` resolves.
/// Compiling is therefore the proof, and the day someone adds a `Send` bound this file goes red.
/// This is the mechanism behind `static_assertions::assert_not_impl_all!`, written out because the
/// trial takes one assertion rather than a dependency.
#[test]
fn the_application_runtime_is_not_send() {
    trait AmbiguousIfSend<A> {
        fn assert_not_send() {}
    }
    impl<T: ?Sized> AmbiguousIfSend<()> for T {}
    impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}

    type Runtime = SyncRunner<frontbox::InMemoryStore, todo_core::HttpTransport>;
    <Runtime as AmbiguousIfSend<_>>::assert_not_send();
}

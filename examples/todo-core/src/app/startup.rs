//! What starting up produced, and why starting up is a method rather than a caller's checklist.
//!
//! Split out of the `app` module beside `direct.rs` and `index.rs`, back when it was a single
//! file, for the reason `mod.rs`'s own doc gives: these are the findings. This one is Finding 6.

use super::PendingIndexGap;

/// The outcome of [`TodoApp::start`](super::TodoApp::start).
///
/// # Why this type exists at all
///
/// Because the sequence it reports used to be the *caller's* to perform, and the only caller that
/// mattered did not perform it. `refresh_from_server` had six callers and every one was a test;
/// `examples/todo-app` had none, so the browser UI was write-only against the server and a reload
/// showed an empty list while the server held the rows. Both halves were individually green — the
/// method worked and had tests, the UI compiled and had gates — and nothing looked at the seam.
///
/// A test can assert on a `Startup`. It cannot assert on a sequence a UI was supposed to remember.
/// See `wiki/plans/d4a-offline-todo-trial.plan.md`, `## Finding 6`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Startup {
    /// Whether the server's list was actually read into the projection.
    ///
    /// **False is a normal outcome, not a failure.** A client that opens with no network starts on
    /// whatever the queue and the projection already hold, which is the entire proposition of an
    /// offline-first cache. What it must not do is refuse to start.
    pub hydrated: bool,
    /// What the pending-index rebuild could not account for.
    ///
    /// Always [`PendingIndexGap::default`] today, because the in-memory outbox is empty at mount.
    /// It is reported anyway because at D4b it will not be — a durable queue survives the reload
    /// that clears the projection, and that is exactly the case a startup sequence has to rebuild
    /// the index for.
    pub gap: PendingIndexGap,
    /// How many rows came back from durable storage before the network was tried.
    ///
    /// **This is what an offline start is worth.** Non-zero means the user saw their data without
    /// a server; zero on a fresh install is normal, and zero on a returning client means storage
    /// lost something.
    pub restored: usize,
    /// Rows the hydration refused to overwrite because they had unsent work.
    ///
    /// Empty on almost every start. Non-empty means the screen deliberately disagrees with the
    /// server, and the application can say so rather than leaving the user to wonder
    /// (`wiki/decisions/032-opaque-row-store.decision.md`).
    pub protected: Vec<String>,
}

impl Startup {
    /// Whether startup both read the server and accounted for everything queued.
    pub fn is_complete(&self) -> bool {
        self.hydrated && self.gap.is_empty()
    }

    /// Whether the projection came back from storage rather than starting empty.
    pub fn restored_anything(&self) -> bool {
        self.restored > 0
    }
}

# Events Are Toasts; Conditions Are The Status Bar

Document Class: Decision
Status: Accepted 2026-09-01; built the same day
Date: 2026-09-01
Category: Trial Application Policy
Scope: Where a message goes in the trial UI, and the rule that decides — replacing three surfaces with no rule between them.
Sources: `examples/todo-app/src/toast.rs`, `examples/todo-app/src/sync.rs`, `examples/todo-app/src/session.rs`, `examples/todo-app/src/main.rs`, `examples/todo-app/src/rows.rs`, `examples/todo-app/src/users.rs`
Related: `wiki/decisions/041-the-poll-stops-when-nobody-is-looking.decision.md`, `wiki/decisions/020-observability-surface.decision.md`, `wiki/decisions/027-dead-letter-reason.decision.md`

## Decision

**One rule, by duration.**

- A thing that **happened** and is over — a refresh started, a sign-up queued, a write refused — is a
  **toast**. Transient, stacked, auto-retiring except for failures, which are dismissed by hand.
- A thing that is **still true** — how many records are pending, how the last drain ended, which
  service has not answered, whether this window is polling — is on the **status bar**. Persistent,
  and it stops saying so when it stops being true.

The three message surfaces that existed are collapsed into that: `SyncView::notice` and
`SessionBar`'s local `status` signal are gone, replaced by `crate::toast`. `SyncView::error` stays,
and stays on the status bar, because it is a condition.

## The Defect This Closes

Three surfaces was not the problem. **The problem was that nothing stopped a call site picking the
wrong one, and one had.**

`examples/todo-app/src/rows.rs` routed the todo composer's failures to `SyncView::error`, and
`crates/frontbox-dioxus/src/sync/mod.rs` sets `error` to `None` on every drain that succeeds. So a
rejected "add todo" disappeared within five seconds of being shown.

What makes it worth a decision rather than a one-line fix is that `examples/todo-app/src/sync.rs`
already carried a paragraph explaining why `error` and `notice` were separate — *"sharing one would
let a drain seconds later clear the only thing the user was told about the click they just made"* —
and the call site next door used the wrong one anyway. A distinction that has to be re-derived at
every call site from a comment in another file is not a distinction the code has.

## Why Duration Rather Than Author

The rejected split is by *who wrote the message*: background loops here, user actions there. It is
the one the old shape used, and it is what produced two signals that a reader had to keep straight.

Duration is checkable at the call site without knowing anything about the rest of the tree. "Is this
still true in thirty seconds?" has an answer the author already knows. "Which of two signals does a
drain clear?" does not.

It also puts the right thing in the right place for the case that kept recurring: **a service that
has been unreachable for an hour.** `tick.dropped` was populated on every round and rendered nowhere,
so a permanently-down user service looked identical to a healthy one. A toast for it would expire and
leave the same silence. It is a condition, so it goes on the status bar and stays there —
`cache: user-service not answering (offline)` — for as long as it is true.

The converse holds too. A refresh that is *happening* is the moment the screen is about to change
under the user without their having asked, and it is over a second later; a status-bar entry for it
would either flicker or lie. It is a toast, and it is the direct answer to "make it clearer when the
fetch and refresh of the cache occurs".

## Progress Toasts Resolve In Place

A `Kind::Progress` toast has no expiry and is replaced by its outcome rather than followed by it:
`refreshing todo, user` becomes `refreshed todo, user` at the same position in the stack. Pushing a
second toast would read as two separate things happening, which is the opposite of what a
started-then-finished operation is.

This is why `Toasts::push` returns an id. Nothing else uses the return value.

## Errors Are Not On A Timer

`Kind::Error` toasts have no TTL and are dismissed by hand. A failure the user did not happen to be
looking at is a failure they never saw, and the whole reason this decision exists is a message that
vanished before it was read.

`Kind::Success` retires after four seconds. `Kind::Progress` has no timer because its resolution is
its timer.

## Consequences

- **`dispatch` takes the queue rather than a signal.** Every write in the application already went
  through it, so this is the one place that had to change for every call site to be correct by
  construction.
- **`Ok(None)` says nothing.** A write that queued exactly as intended is not news, and announcing
  every one would train the eye past the stack that also carries refusals.
- **The status bar gained the cache line it never had**: per-source freshness, in-flight state, and
  the paused state decision 041 introduces. That line is the reason "the window stopped polling" is
  now a sentence on screen rather than an inference from an absence.
- **The `set_done` refusal is pushed by hand**, not returned through `dispatch`, because `dispatch`
  reads an `Ok` message as a success and a server declining on the merits is not one. It will never
  reach the dead-letter panel either, because direct dispatch means it was never queued
  (`wiki/decisions/027-dead-letter-reason.decision.md`).
- **The e2e asserts on both surfaces.** A toast that does not appear and a toast that does not retire
  are both failures, and the paused state is asserted as text rather than as an absence of requests.

## What This Does Not Decide

Anything about `frontbox` or `frontbox-dioxus`. This is a trial-application decision about a trial
application's UI; the adapter publishes signals and says nothing about how they are rendered, which
is the boundary `wiki/decisions/020-observability-surface.decision.md` draws and this does not move.

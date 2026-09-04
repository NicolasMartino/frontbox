//! The drain loop, and the two panels that read it.
//!
//! # This is `frontbox_dioxus::use_sync_loop` now
//!
//! It was not, and the reason it was not is the whole of register entry 14. The loop here used to
//! be a near-transcription of the adapter's — same cadence type, same `Sleeper`, same three
//! signals, same "refresh the counts even when the drain failed" argument — because two things
//! stopped it composing, and neither was a detail this file could work around.
//!
//! **The old hook wanted a `SyncRunner` that `TodoApp` will not give up.** It took the runner *by
//! value*, so a shared handle could be the one `Rc` everybody read through. `TodoApp` owns its
//! runner privately and exposes `store()`, `transport()`, `outbox()` and `sync()`; nothing hands
//! the runner out, and adding an accessor would have been an API change to the application
//! demonstrating the problem.
//!
//! **It also drained the runner, and draining the runner is not what this application means by
//! syncing.** `TodoApp::sync` is `drain()` *plus* the bookkeeping that keeps the row-to-pending
//! index — the thing [`TodoStore::is_saving`](todo_core::TodoStore::is_saving) answers from — in
//! step with the queue. Route the loop through the old hook and every row stayed on "saving…"
//! forever.
//!
//! Both are gone rather than merely unused: the handle survived entry 14 as a convenience for
//! applications that let Dioxus own the runner, that class never acquired a member, and its one
//! obvious use was a trap — two loops over one queue, each believing itself alone.
//!
//! At the time that second half was `refresh_pending()`, a full scan of the queue after every
//! drain, because a `DrainReport` named a mutation only when something had gone wrong and there was
//! no success event to decrement on. That was D4a's Finding 1, and
//! `wiki/decisions/035-reports-name-what-drained.decision.md` closed it: `sync` now decrements by
//! exactly what `DrainReport::drained` names, and `refresh_pending` reconciles at startup instead.
//! **The composition problem was unaffected by the fix**, which is the point worth keeping — the
//! hook could not express "drain, then do something with the result" whichever form that something
//! took.
//!
//! **The hook now asks for the two capabilities instead of the object.** `SyncStep` is "run one
//! sync and tell me how it ended" and `CountsStep` is "read the counts", so `TodoApp::sync` goes in
//! whole — index bookkeeping and all — and this file keeps only what is genuinely its own: the
//! dead-letter poll and the revision bump, which ride along inside the same two closures.
//!
//! What composed unchanged all along, and still does: [`SyncCadence`] as the entire backoff policy,
//! [`Sleeper`] as the timer seam, and [`OutboxCounts::read`] over
//! `app.outbox()`. Those three are plain values and functions rather than hooks, which is exactly
//! why they survived contact with an application that owns its own runtime.

use dioxus::prelude::*;
use frontbox::{DeadLetterReason, DeadLetterRecord, DrainReport};
use frontbox_dioxus::{
    use_sync_loop, CountsStep, OutboxCounts, Sleeper, SyncCadence, SyncState, SyncStep,
};

use crate::{log, Ui};

const DEAD_LETTERS_LIMIT: usize = 100;

/// What the drain loop publishes to the tree.
///
/// `Copy`, because every field is a `Signal` and a signal is a handle rather than a value.
#[derive(Clone, Copy)]
pub struct SyncView {
    /// The most recent drain, or `None` before the first one finishes.
    pub last: Signal<Option<DrainReport>>,
    /// The three store counts, refreshed after every drain.
    pub counts: Signal<OutboxCounts>,
    /// What will not be sent again.
    ///
    /// Polled with the counts rather than derived from them. `OutboxCounts` reports *how many*
    /// dead letters there are and the adapter offers nothing that lists them, so a panel showing
    /// what a server actually refused has to call `DeadLetterStore::list` on its own schedule.
    pub dead: Signal<Vec<DeadLetterRecord>>,
    /// The last background failure: a drain that errored, or a store read that did.
    ///
    /// **Kept as a signal rather than pushed to `crate::toast` with everything else, because it is
    /// a condition and not an event.** The adapter's loop clears it on the next drain that
    /// succeeds, so what it holds is "the drain is failing *now*" — which stays true until it stops
    /// being true, and a toast for it would expire while the queue was still stuck. The rule this
    /// file follows: what happened goes to the toast stack, what is still the case goes on the
    /// status bar.
    ///
    /// It used to carry user-action failures too, and that was the defect: the todo composer routed
    /// its errors here and the next successful drain wiped them within five seconds.
    pub error: Signal<Option<String>>,
}

/// Drain the queue forever, on a cadence, and provide what happened to the subtree.
///
/// The loop is the adapter's. What is passed in is what only this application knows: that syncing
/// means `TodoApp::sync` rather than `drain`, and that a round is not over until the dead letters
/// have been re-read and the rows told to re-render.
pub fn use_todo_sync(ui: Ui, cadence: SyncCadence, sleeper: Sleeper) -> SyncView {
    let dead = use_signal(Vec::new);

    let sync = {
        let ui = ui.clone();
        SyncStep::new(move || {
            let ui = ui.clone();
            async move {
                log("ui sync step");
                let report = ui.app.sync().await?;
                // `sync` decremented the pending index for everything that drained, so any row's
                // "saving…" marker may have moved.
                crate::bump(ui.revision);
                Ok(report)
            }
        })
    };

    let counts = {
        let (ui, dead) = (ui.clone(), dead);
        CountsStep::new(move || {
            let (ui, mut dead) = (ui.clone(), dead);
            async move {
                log("ui counts step");
                // Polled with the counts because the adapter offers nothing that lists dead
                // letters, only how many there are. A failure here is reported the same way a
                // counts failure is: through the step's own error, on the loop's `error` signal.
                dead.set(ui.app.dead_letters(DEAD_LETTERS_LIMIT).await?);
                OutboxCounts::read(ui.app.outbox()).await
            }
        })
    };

    // `..` because `SyncState` is `#[non_exhaustive]`: a field the adapter adds later must not
    // break this destructuring, which is the point of the attribute.
    let SyncState {
        last,
        counts: counts_signal,
        error,
        ..
    } = use_sync_loop(sync, counts, cadence, sleeper);

    let view = SyncView {
        last,
        counts: counts_signal,
        dead,
        error,
    };
    use_context_provider(|| view);
    view
}

/// Toggle the deterministic offline switch the trial transport exposes.
///
/// Renders the label alone rather than its own row, so [`SessionBar`](crate::session::SessionBar)
/// can place it beside the identity and the refresh button — where the wireframe puts it and where
/// somebody reaching for it will look. It had its own `div.controls` until the nested UI landed,
/// which was fine when it sat between two lists and is not now.
#[component]
pub fn OfflineControl() -> Element {
    let ui = use_context::<Ui>();
    let mut offline = use_signal(|| false);
    let checked = *offline.read();

    rsx! {
        label { class: "offline",
            input {
                r#type: "checkbox",
                checked,
                onchange: move |event| {
                    let next = event.checked();
                    log(&format!("ui offline control={next}"));
                    ui.app.transport().set_offline(next);
                    offline.set(next);
                    if !next {
                        // **Coming back online settles the bet every pending wait is making.**
                        // Without this the loop serves out `offline_ms` — fifteen seconds of a
                        // queue visibly not draining after the user has just told it the network
                        // is back. Found by running the app; see `platform::app_wake`.
                        ui.wake.wake();
                    }
                },
            }
            span { "offline" }
        }
    }
}

/// Everything that is still *true*: the queue's three counts, how the last drain ended, what the
/// index could not account for, and — the part this trial kept needing and not having — when each
/// cache source last answered and whether this window is polling at all.
///
/// Events do not belong here. They go to `crate::toast`; see [`SyncView::error`] for the line.
#[component]
pub fn StatusBar() -> Element {
    let ui = use_context::<Ui>();
    let view = use_context::<SyncView>();
    let activity = use_context::<Signal<crate::session::CacheActivity>>();
    // The pending index is plain `RefCell` state like the rows are, so the same counter that tells
    // the list to re-render is what tells this bar the gap moved. See `Ui::revision`.
    ui.revision.read();
    let gap = ui.app.pending_index_gap();
    let counts = *view.counts.read();
    // `DrainEnd` is `Copy`, so this reads the discriminant out and drops the guard rather than
    // holding a borrow of the signal across the render below.
    let ended = view.last.read().as_ref().map(|report| report.ended);
    let alert = view.error.read().clone();
    let cache = activity.read().clone();
    // Read, not called: `Ui::now` ticking is what re-renders "4s ago" into "5s ago". Reading the
    // clock here instead would produce a fresh tree every render with nothing to schedule the next.
    let now = *ui.now.read();

    rsx! {
        section { class: "status",
            span { "pending {counts.pending}" }
            span { "dead {counts.dead_letters}" }
            span { "quarantined {counts.quarantined}" }
            match ended {
                Some(end) => rsx! { span { "last drain: {end:?}" } },
                None => rsx! { span { "no drain yet" } },
            }
            if !gap.is_empty() {
                // Said out loud rather than swallowed: every "saving…" marker is read out of an
                // index that this many queued records are missing from, so the markers below are
                // incomplete and the user is entitled to know which way.
                span { class: "gap",
                    "index incomplete: {gap.beyond_scan_cap} beyond the scan cap, "
                    "{gap.unrecoverable} unplaceable"
                }
            }
            if let Some(message) = alert {
                span { class: "alert", "{message}" }
            }
            span { class: "cache",
                "cache: "
                if cache.sources.is_empty() {
                    "not polled yet"
                } else {
                    {cache.sources.iter().map(|source| {
                        let detail = match (&source.failing, source.last_ok_ms) {
                            (Some(reason), _) => format!("not answering ({reason})"),
                            (None, Some(at)) => ago(now, at),
                            (None, None) => "no answer yet".to_owned(),
                        };
                        rsx! { span { key: "{source.name}", class: "source",
                            "{source.name} {detail}"
                        } }
                    })}
                }
            }
            if cache.paused {
                // The whole of the reported "the window stops polling": it does, on purpose, and
                // saying so is the difference between a policy and a defect.
                span { class: "paused", "polling paused — this window is not in front" }
            } else if cache.working {
                span { class: "working", "checking…" }
            }
        }
    }
}

/// How long ago, in the coarsest unit that is still honest.
fn ago(now_ms: i64, then_ms: i64) -> String {
    let seconds = (now_ms - then_ms).max(0) / 1_000;
    match seconds {
        0 => "just now".to_owned(),
        1..=59 => format!("{seconds}s ago"),
        _ => format!("{}m ago", seconds / 60),
    }
}

/// What the server refused, and why.
#[component]
pub fn DeadLetters() -> Element {
    let records = use_context::<SyncView>().dead.read().clone();
    if records.is_empty() {
        return rsx! {};
    }

    rsx! {
        section { class: "dead",
            h2 { "dead letters" }
            ul {
                for record in records {
                    li { key: "{record.mutation_id}",
                        span { class: "op", "{record.method} {record.path}" }
                        span { {reason_line(&record.reason)} }
                    }
                }
            }
        }
    }
}

/// Say why a record was parked, in the words of whoever parked it.
///
/// The wildcard is mandatory rather than defensive: `DeadLetterReason` is `#[non_exhaustive]`, so
/// from outside `frontbox` this match cannot be proved complete even though it names every variant
/// that exists today. The arm is written to be readable if it is ever reached — a client that does
/// not understand why a write is terminal should say so, not guess.
fn reason_line(reason: &DeadLetterReason) -> String {
    match reason {
        DeadLetterReason::Rejected {
            error: Some(refusal),
        } => match &refusal.code {
            Some(code) => format!("the server refused it ({code}): {}", refusal.message),
            None => format!("the server refused it: {}", refusal.message),
        },
        DeadLetterReason::Rejected { error: None } => {
            "the server refused it and said nothing further".to_owned()
        }
        // Deliberately not phrased as a refusal. No server ever declined this one; the client gave
        // up on it, and telling a user otherwise puts words in the server's mouth.
        DeadLetterReason::RetentionBound => {
            "this client stopped retrying it; no server ever refused it".to_owned()
        }
        DeadLetterReason::Caller(why) if why.is_empty() => "this client parked it".to_owned(),
        DeadLetterReason::Caller(why) => format!("this client parked it: {why}"),
        _ => "parked for a reason this build of the client does not recognise".to_owned(),
    }
}

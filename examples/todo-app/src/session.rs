//! Identity, the refresh button, and the invalidation poll — D4d's whole UI surface.
//!
//! # Two things called "user", and the UI has to show both
//!
//! The **scope** is this client's local queue identity, valid the instant it is picked and never
//! issued by a server (`wiki/decisions/009-local-scope-identity.decision.md`). The **user record**
//! is a row on the user service, created by an ordinary queued write like any other. Sign up
//! offline and the first exists immediately while the second does not exist anywhere yet — which
//! is exactly the state this panel has to render without lying about either.

use dioxus::prelude::*;

use crate::toast::{Kind, Toasts};
use crate::{dispatch, log, platform, Ui};

/// How long a client will tolerate not having asked anything, when the tab is in front.
///
/// Not a staleness budget — the budgets live per source, in `todo-core`, because they are about
/// entities rather than about tabs. This is only how often the loop *checks* whether any source is
/// past its budget, so it needs to be **meaningfully smaller** than the tightest one.
///
/// # Why "smaller" and not "no larger"
///
/// It read "no larger" and was set to exactly `BROWSER_OBSERVABLE_STALENESS_MS`, which is the
/// boundary case rather than a safe one. A source that comes due just after a tick is not noticed
/// until the next, so equal values put the worst case at *two* budgets — thirty seconds before a
/// second window even asks whether anything moved, for a fifteen-second budget. Measured at 11.3 s
/// for one write, which is inside that band and looks like a bug from the outside.
///
/// A third of the budget costs nothing to run: a tick that finds no source due issues no request,
/// so this number sets the *granularity* of the check and not the request rate.
const TICK_MS: u32 = 5_000;

/// Whether anybody is looking at this client.
///
/// **No `cfg` here any more, and that is the point.** This used to be two functions — a browser one
/// reading `document.hidden` and `document.hasFocus()`, and a native one hard-coded `true` under a
/// comment saying mobile lifecycle gating was named and not built. It is built now, and everything
/// a platform decides about it lives in [`platform::Foreground`] where the rest of this crate's
/// platform differences already live.
fn focused(ui: &Ui) -> bool {
    ui.foreground.is_foreground()
}

/// What the invalidation loop is doing, for the status bar to say out loud.
///
/// # Why this is *state* and not a toast
///
/// The events — "checking…", "refreshed todos" — are toasts, because they happen and are over. This
/// is the other half: how long ago each service last answered, whether one of them has stopped
/// answering, and whether the loop is polling at all. A toast for "the user service is not
/// answering" expires and leaves a screen that no longer mentions a service which has now been down
/// for an hour, which is precisely the failure this trial kept finding in itself: `tick.dropped` was
/// populated on every round and rendered nowhere.
#[derive(Clone, PartialEq, Default)]
pub struct CacheActivity {
    /// One entry per source, in the order the loop asked them.
    pub sources: Vec<SourceState>,
    /// True while a poll or a hydrate is in flight.
    pub working: bool,
    /// True while the loop is parked because nobody is looking at this window.
    pub paused: bool,
}

/// One invalidation source, as the status bar renders it.
#[derive(Clone, PartialEq)]
pub struct SourceState {
    pub name: String,
    /// When it last answered, or `None` if it never has.
    pub last_ok_ms: Option<i64>,
    /// Why it could not answer, if the last round is the one that failed.
    pub failing: Option<String>,
}

/// Poll the invalidation sources forever, gated on whether anybody is looking.
///
/// # Why this loop is the application's and not the adapter's
///
/// Decision 038: core and the adapter gain nothing until the seam has been held by a real
/// application. `use_sync_loop` exists for the *drain* because a drain is one call with one report;
/// an invalidation round is several sources with per-source failure, and whether that generalises
/// to the adapter's shape is precisely the question D4d is supposed to answer rather than assume.
pub fn use_invalidation_poll(ui: Ui) -> Signal<CacheActivity> {
    let toasts = use_context::<Toasts>();
    let activity = use_signal(CacheActivity::default);
    // Built once per component rather than per render. `wakeable` mints this sleeper's standing in
    // the shared wake, and a fresh one every render would be a new waiter the running loop is not
    // the one holding.
    let sleeper = use_hook({
        let wake = ui.wake.clone();
        move || platform::sleeper(wake)
    });

    use_future(move || {
        let (ui, sleeper, mut activity) = (ui.clone(), sleeper.clone(), activity);
        async move {
            loop {
                // **Paused, not slowed.** This used to serve out a ten-minute `HIDDEN_TICK_MS`,
                // which is a schedule pretending to be a policy: there is no interval that is
                // correct for "nobody is looking", only a decision to stop and a decision to
                // resume. `until_woken` runs no timer at all, and `platform::app_wake` fires on the
                // window regaining focus and on the tab becoming visible.
                //
                // `continue` rather than falling through, because the same wake is also fired by
                // the offline switch and by a bfcache restore — arriving here does not by itself
                // mean anybody is looking.
                if !focused(&ui) {
                    log("ui invalidation paused (nobody is looking)");
                    activity.with_mut(|state| state.paused = true);
                    sleeper.until_woken().await;
                    continue;
                }
                activity.with_mut(|state| {
                    state.paused = false;
                    state.working = true;
                });
                log("ui invalidation tick");
                match ui.app.poll_invalidation().await {
                    Ok(tick) => {
                        log(&format!(
                            "ui invalidation result polled={:?} changed={:?} dropped={}",
                            tick.polled,
                            tick.changed,
                            tick.dropped.len()
                        ));
                        let at = platform::now_ms();
                        activity.with_mut(|state| state.observe(&tick, at));
                        if !tick.changed.is_empty() {
                            // Something the server holds moved. Say so while it is happening: this
                            // is the moment the user's screen is about to change under them without
                            // their having asked, and it is the one the report "I can't tell when
                            // it refreshes" was about.
                            let progress = toasts.push(
                                Kind::Progress,
                                format!("refreshing {}", tick.changed.join(", ")),
                            );
                            match hydrate(&ui).await {
                                Ok(()) => {
                                    toasts.resolve(
                                        progress,
                                        Kind::Success,
                                        format!("refreshed {}", tick.changed.join(", ")),
                                    );
                                    // Only on success. Bumping regardless announced a re-render of
                                    // rows that had not been re-read, which made a failed refetch
                                    // indistinguishable from a successful one that changed nothing.
                                    crate::bump(ui.revision);
                                }
                                Err(failure) => {
                                    toasts.resolve(
                                        progress,
                                        Kind::Error,
                                        format!("could not refresh: {failure}"),
                                    );
                                }
                            }
                        }
                    }
                    // Only a *storage* failure reaches here — a source that could not answer is
                    // reported per-source in `tick.dropped` above. It used to be discarded whole,
                    // message and all, which left the one arm that means "this client's own
                    // database is broken" logging a constant string.
                    Err(failure) => {
                        log(&format!("ui invalidation storage error {failure}"));
                        toasts.push(Kind::Error, format!("local storage: {failure}"));
                    }
                }
                activity.with_mut(|state| state.working = false);
                sleeper.sleep(TICK_MS).await;
            }
        }
    });

    use_context_provider(|| activity)
}

/// Re-read both services, reporting the first failure rather than swallowing both.
///
/// These were two `let _ =` bindings. A refetch that failed was invisible, and the revision was
/// bumped anyway — so "the second window never showed the first window's write" had no on-screen
/// and no console explanation, which is most of why it took a browser session to find.
async fn hydrate(ui: &Ui) -> Result<(), frontbox::Error> {
    ui.app.refresh_from_server().await?;
    ui.app.refresh_users_from_server().await?;
    Ok(())
}

impl CacheActivity {
    /// Fold one round's outcome into what the status bar shows.
    fn observe(&mut self, tick: &todo_core::invalidation::InvalidationTick, at_ms: i64) {
        for name in &tick.polled {
            let failure = tick
                .dropped
                .iter()
                .find(|dropped| &dropped.source == name)
                .map(|dropped| dropped.reason.clone());
            let entry = match self.sources.iter_mut().find(|source| &source.name == name) {
                Some(entry) => entry,
                None => {
                    self.sources.push(SourceState {
                        name: name.clone(),
                        last_ok_ms: None,
                        failing: None,
                    });
                    self.sources.last_mut().expect("just pushed")
                }
            };
            // `last_ok_ms` is left alone on a failure rather than reset: "answered 4s ago" and
            // "has not answered since 4s ago" are the same fact, and clearing it would throw away
            // the only number that says how long the outage has been running.
            if failure.is_none() {
                entry.last_ok_ms = Some(at_ms);
            }
            entry.failing = failure;
        }
    }
}

/// Who this client is, and the button that asks both services for news.
#[component]
pub fn SessionBar() -> Element {
    let ui = use_context::<Ui>();
    let mut name = use_signal(String::new);
    let toasts = use_context::<Toasts>();

    // Subscribes this component to the projection, so a drain or a poll re-renders the name.
    let _ = ui.revision.read();
    let known = ui.app.user_name(ui.app.user_id());

    // Cloned into each handler rather than fetched inside it. `use_context` is a hook, and a hook
    // called from an event handler runs outside the render it belongs to — Dioxus identifies hooks
    // by call order, so there is no scope to attach to and it panics. Every other component in this
    // crate captures `ui` the same way; this one is not a special case.
    let sign_up = {
        let ui = ui.clone();
        move |_| {
            let chosen = name.read().trim().to_owned();
            if chosen.is_empty() {
                return;
            }
            name.set(String::new());
            // Queued like any other write, with no network involved. Offline signup is the whole
            // story D4d tells: the scope this application is already running under was never
            // waiting for this call to succeed.
            dispatch(&ui, toasts, move |app| async move {
                log("ui action sign_up");
                app.sign_up(&chosen).await?;
                Ok(Some("signed up — queued, not sent".to_owned()))
            });
        }
    };

    let refresh = {
        let ui = ui.clone();
        move |_| {
            dispatch(&ui, toasts, move |app| async move {
                log("ui action refresh");
                let tick = app.refresh_now().await?;
                if let Some(dropped) = tick.dropped.first() {
                    // Named rather than swallowed. A source that could not answer has already
                    // marked its own entities stale, and a button that said "nothing changed"
                    // after that would be reporting the opposite of what happened.
                    return Ok(Some(format!("{} is not answering", dropped.source)));
                }
                if tick.changed.is_empty() {
                    return Ok(Some("up to date".to_owned()));
                }
                // The same path the cadence takes, which is the point of the button being a
                // trigger rather than a source of its own: there is no second code route that
                // could drift out of step with the automatic one.
                app.refresh_from_server().await?;
                app.refresh_users_from_server().await?;
                Ok(Some(format!("refreshed {}", tick.changed.join(", "))))
            });
        }
    };

    rsx! {
        div { class: "controls session",
            match &known {
                // The record exists locally. Whether the *server* has it is a separate question,
                // and the status bar's pending count is the honest answer to that — so this claims
                // no more than "this client believes it is you".
                Some(display) => rsx! { span { class: "identity", "signed in as {display}" } },
                None => rsx! {
                    input {
                        r#type: "text",
                        placeholder: "your name",
                        value: "{name}",
                        oninput: move |event| name.set(event.value()),
                    }
                    button { onclick: sign_up, "sign up" }
                },
            }
            button { onclick: refresh, "refresh" }
            // The offline switch lives here rather than in a row of its own: it is the control this
            // demo exists to be operated with, and it belongs beside the identity it applies to.
            crate::sync::OfflineControl {}
        }
    }
}

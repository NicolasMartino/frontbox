//! A UI over [`todo_core::TodoApp`] on three platforms, and the D4a trial's other half.
//!
//! `todo-core` carries the application logic and no framework; this crate carries the framework and
//! no application logic. Nothing here decides what a write means, when to fall back to the queue,
//! or what a row looks like once the server has spoken — every one of those already has an answer
//! next door, and reimplementing any of it here would make the trial's claim of a framework-neutral
//! core unfalsifiable.
//!
//! What is left is the part a UI genuinely owns: turning a tap into a spawned `async` call, turning
//! a `RefCell` read model into something Dioxus re-renders, and running the drain loop on a
//! cadence. The third of those comes from `frontbox-dioxus` — it did not, until the hook learned to
//! take closures. See [`sync`] for what changed.
//!
//! # Three platforms, one component tree
//!
//! This crate builds for a browser, a desktop window, an iPhone and an Android device, and **not
//! one component below this file is conditionally compiled**. Everything a platform decides is in
//! [`platform`]: the clock, the timer, where storage lives, and where the server is. That the list
//! is four items long rather than forty is the finding, and it belongs to the seams underneath —
//! `todo-core`'s backend module and `frontbox_dioxus::Sleeper` — not to this file.
#![allow(non_snake_case)]

mod platform;
mod rows;
mod session;
mod style;
mod sync;
mod toast;
mod users;

use std::future::Future;
use std::rc::Rc;

use dioxus::prelude::*;
use frontbox_dioxus::SyncCadence;
use todo_core::TodoApp;

use crate::toast::Toasts;

fn main() {
    dioxus::launch(App);
}

/// A built application, usable as a Dioxus prop.
///
/// Components require their props to be [`PartialEq`] so the renderer can skip subtrees that did
/// not change. [`TodoApp`] is not — it owns a `RefCell` read model and a `SyncRunner`, and
/// comparing two of those by value would be meaningless anyway. Pointer identity is the honest
/// comparison: two handles are the same application exactly when they are the same allocation.
#[derive(Clone)]
pub struct AppHandle(pub Rc<TodoApp>);

impl PartialEq for AppHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// What every component needs, shared by reference count.
///
/// `Rc` rather than `Arc` because [`TodoApp`] is `!Send` and honestly so: it owns a `SyncRunner`
/// whose in-flight flag is a `Cell` and a read model built on `RefCell`. An `Arc` here would claim
/// a guarantee neither this crate nor `frontbox` makes.
#[derive(Clone)]
pub struct Ui {
    /// The application. One instance, built once, shared by the whole tree.
    pub app: Rc<TodoApp>,
    /// The one wake every waiting loop in this tree races.
    ///
    /// Fired by whatever means "the user is back" on this platform — a bfcache restore, a tab
    /// becoming visible, a window regaining focus, an OS resuming a suspended application — and by
    /// the offline switch coming back on. See `platform::app_wake`. A component that changes
    /// something a pending wait was betting against should fire it.
    pub wake: frontbox_dioxus::Wake,
    /// Whether this client is in front, asked rather than assumed.
    ///
    /// Carried on `Ui` because the loop that reads it is an `async` block with no Dioxus context to
    /// reach through, and because that is what lets `session::focused` hold no `cfg` of its own.
    pub foreground: platform::Foreground,
    /// Wall time, re-read on a short tick, for anything whose *rendering* changes as time passes.
    ///
    /// Two readers: the toast queue retires finished messages against it, and the status bar's
    /// "4s ago" would otherwise be written once and stay wrong. Neither is a reason to consult a
    /// clock during a render — a render that reads `Date.now()` produces a different tree every
    /// time it runs and nothing tells it when to run again — so the tick is the subscription, the
    /// same trick `revision` is.
    pub now: Signal<i64>,
    /// Bumped whenever the projection or the pending index changed.
    ///
    /// # Why a counter and not a signal per row
    ///
    /// `TodoStore` is `RefCell`-backed and knows nothing about Dioxus, which is the point of
    /// decision 023 — frontbox stores the queue and the rows, the application owns the projection
    /// it renders. Nothing in either crate then tells a renderer that the rows moved, so a
    /// component reading `store().rows()` would render once and never again.
    ///
    /// A counter is the smallest bridge that works: reading it subscribes a component, and one
    /// write after each mutation invalidates every reader. It over-renders — a rename to one row
    /// re-renders all of them — and that is the honest cost of a read model the framework cannot
    /// see into, not something this file should hide behind finer-grained plumbing.
    pub revision: Signal<u64>,
}

/// Announce that the projection or the pending index changed.
pub fn bump(mut revision: Signal<u64>) {
    *revision.write() += 1;
    log("ui revision bumped");
}

/// One diagnostic line, on whichever surface this platform has.
///
/// **One copy, at the crate root.** This was five identical private `log` functions — here and in
/// `session`, `rows`, `sync` and `users` — each with the same two-arm `cfg` and the same `[frontbox]`
/// prefix. Nothing enforced that they stayed identical, and the prefix is what `e2e/ui.mjs` filters
/// the console on, so one of them drifting would have made a module silently invisible to the
/// harness rather than obviously wrong.
///
/// Deliberately not `todo_core::trace`: that one is private to the application crate and describes
/// what the *application* did. This describes what the UI did, and the two travelling under one
/// prefix is what makes a browser console read as a single story.
pub fn log(message: &str) {
    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&format!("[frontbox] {message}").into());
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("[frontbox] {message}");
}

/// Spawn one application call, publish anything worth reading, and re-render the list.
///
/// Every write goes through here. `TodoApp`'s writes are `async` because *enqueueing* is — the
/// mutation reaches the durable queue before it reaches the network — so an event handler cannot
/// call one directly, and [`spawn`] is what Dioxus offers for a future that is not `Send`.
///
/// `work` takes the `Rc` by value rather than receiving a borrow, so the future it builds owns what
/// it awaits. A closure handed `&TodoApp` would produce a future borrowing a temporary, which is
/// the shape that does not compile.
pub fn dispatch<F>(ui: &Ui, toasts: Toasts, work: impl FnOnce(Rc<TodoApp>) -> F + 'static)
where
    F: Future<Output = Result<Option<String>, frontbox::Error>> + 'static,
{
    let (app, revision) = (Rc::clone(&ui.app), ui.revision);
    spawn(async move {
        log("ui dispatch start");
        match work(app).await {
            // `None` is the ordinary case and says nothing. A write that queued exactly as it was
            // meant to is not news, and announcing every one of them would train the eye to ignore
            // the stack that also carries refusals.
            Ok(None) => log("ui dispatch ok"),
            Ok(Some(message)) => {
                log("ui dispatch ok");
                toasts.push(toast::Kind::Success, message);
            }
            Err(failure) => {
                log(&format!("ui dispatch error {failure}"));
                toasts.push(toast::Kind::Error, failure.to_string());
            }
        }
        bump(revision);
    });
}

/// How often the UI clock re-reads wall time.
///
/// Fast enough that a four-second toast does not visibly overstay and "3s ago" is not "5s ago", and
/// slow enough to be two renders a second of a tree this size. It is not a *staleness* budget and
/// nothing fetches on it — see `session::TICK_MS` for the one that does.
const UI_TICK_MS: u32 = 500;

/// Opening storage, which is the one thing that has to happen before anything can render.
///
/// # Why this is a component of its own
///
/// **Opening durable storage is asynchronous**, and D4b is what made it so: `TodoApp::new` used to
/// be a plain call because an in-memory queue needs no opening; a browser database does, and no
/// amount of seam-drawing makes that synchronous. So the tree has a state it did not have before —
/// *opening* — and that state is rendered rather than hidden behind a blank page. A first paint
/// showing nothing while storage opens is indistinguishable from one showing nothing because the
/// queue is empty, and the two need different reactions from whoever is looking.
///
/// The split into two components is not stylistic. **Dioxus identifies hooks by call order**, so a
/// component that returns early before its later hooks runs a different number of them on the
/// first render than on the second. Doing the opening here and everything else in [`Ready`] means
/// every hook below is unconditional, which is the only arrangement that stays correct as hooks
/// are added.
#[component]
fn App() -> Element {
    let opened = use_resource(|| async {
        let storage = platform::storage()?;
        TodoApp::new(
            todo_core::Config {
                todo_url: platform::BASE_URL.to_owned(),
                user_url: Some(platform::USER_BASE_URL.to_owned()),
                scope: platform::SCOPE.to_owned(),
                user_id: platform::USER_ID.to_owned(),
                storage,
            },
            platform::clock(),
        )
        .await
        .map(|app| AppHandle(Rc::new(app)))
        .map_err(|failure| failure.to_string())
    });

    match &*opened.read_unchecked() {
        Some(Ok(app)) => rsx! { Ready { app: app.clone() } },
        // A scope key this crate hard-codes cannot fail, but the *storage* genuinely can — private
        // browsing refuses IndexedDB outright, and a phone can refuse a write — and a blank page is
        // the worst way to be told.
        Some(Err(failure)) => shell(rsx! { p { class: "alert", "storage: {failure}" } }),
        None => shell(rsx! { p { class: "notice", "opening local storage…" } }),
    }
}

/// The whole page, once there is an application behind it.
#[component]
fn Ready(app: AppHandle) -> Element {
    let revision = use_signal(|| 0);
    let now = use_signal(platform::now_ms);
    let (wake, foreground) = platform::app_wake();
    let ui = use_context_provider(|| Ui {
        app: app.0,
        revision,
        now,
        wake: wake.clone(),
        foreground,
    });
    let toasts = toast::use_toasts();

    // The UI clock. Deliberately on `platform::timer()` and not the wakeable sleeper: this is the
    // one loop in the tree that must *not* be cut short by a focus or a bfcache restore, because
    // it is measuring rendering time rather than waiting on anything.
    use_future(move || {
        let (timer, mut now) = (platform::timer(), now);
        async move {
            loop {
                timer.sleep(UI_TICK_MS).await;
                let at = platform::now_ms();
                now.set(at);
                toasts.prune(at);
            }
        }
    });

    // The timer is the only thing in this tree that knows which platform it is on, and it arrives
    // already built — `Sleeper` is the seam the adapter draws for exactly that reason. On web it
    // also carries the wake; see `platform::sleeper` and `platform::app_wake`.
    //
    // `use_hook` so it is built once per component and not once per render: `Sleeper::wakeable`
    // mints this loop's standing in the shared wake, and a fresh sleeper every render would be a
    // waiter that the already-running loop is not the one holding.
    let sleeper = use_hook(move || platform::sleeper(wake));
    sync::use_todo_sync(ui.clone(), SyncCadence::default(), sleeper);

    // Read the server's list once, before the user does anything.
    //
    // This is the whole of the fix for Finding 6, and its smallness is the finding. `TodoApp` had
    // `refresh_from_server` all along and this file never called it, so the UI was write-only
    // against the server: writes drained and showed up in the API, and a reload rendered an empty
    // list. The sequence now lives in [`TodoApp::start`] where a test can assert on it, and what
    // is left here is the one line that says when to run it.
    use_future({
        let ui = ui.clone();
        move || {
            let ui = ui.clone();
            async move {
                log("ui startup task");
                match ui.app.start().await {
                    // An offline start is a start. Said out loud, because a list that is empty
                    // because nobody has written one and a list that is empty because the server
                    // could not be reached look identical, and only one of them is worth waiting on.
                    Ok(startup) => {
                        if !startup.hydrated {
                            toasts.push(
                                toast::Kind::Error,
                                "started offline — this is local state, not the server's",
                            );
                        }
                        bump(ui.revision);
                    }
                    // A wrong `TODO_SERVER_URL` lands here rather than in the notice above, which is
                    // the distinction `map_send_error` exists to preserve.
                    Err(failure) => {
                        toasts.push(toast::Kind::Error, format!("could not start: {failure}"));
                    }
                }
            }
        }
    });

    session::use_invalidation_poll(ui.clone());

    // The gate. Before there is a user record for this client there is nothing todo-shaped on
    // screen at all — but the offline switch and the status bar stay, because signing up *while
    // offline* is the headline behaviour and it is only legible if the queue is visible.
    //
    // **Reading `revision` is the subscription, and leaving it out is why this shipped broken
    // once.** `user_name` reads a `RefCell` Dioxus cannot see into, so without this line the gate
    // is computed on the first render and never again: signing up updated `SessionBar`, which does
    // read the counter, while this component went on rendering "sign up to begin" underneath it.
    // The symptom was a page that acknowledged the signup and refused to move past it.
    ui.revision.read();
    let signed_up = ui.app.user_name(ui.app.user_id()).is_some();

    shell(rsx! {
        session::SessionBar {}
        if signed_up {
            users::UserList {}
        } else {
            p { class: "notice",
                "Sign up to begin. Your name is queued like every other write, so this works with "
                "the network off."
            }
        }
        sync::StatusBar {}
        sync::DeadLetters {}
        toast::ToastStack {}
    })
}

/// The page frame every render path shares, so a failure to start still looks like the application.
fn shell(inner: Element) -> Element {
    rsx! {
        style { {style::CSS} }
        main { class: "app",
            h1 { "todo" }
            {inner}
        }
    }
}

//! Whether anybody is looking, and the wake that says they are back.
//!
//! Split out of [`super`] when it passed the four-hundred-line cap `AGENTS.md` sets.
//! The line is a real seam and not just a length: everything next door answers "what does this
//! platform provide" — a clock, a timer, a place to put a database — and this answers a question
//! that changes while the application runs.

/// The application's one wake: bfcache restore, the tab becoming visible, the window regaining
/// focus, and coming back online.
///
/// # One wake, four events, and why they belong together
///
/// [`Sleeper::wakeable`](frontbox_dioxus::Sleeper::wakeable) takes a single [`Wake`], so anything
/// that should cut a wait short has to fire the *same* one. That is not a limitation to work
/// around — it is the right model. A wait is a bet about what happens next, and all four of these
/// settle it: the page was frozen and is back, nobody was looking and now is, the window was in the
/// background and is in front, the network was gone and is not.
///
/// The last is fired by the offline switch in `crate::sync::OfflineControl`, and it is the one
/// running the app found: unticking "offline" left the drain loop serving out `offline_ms` —
/// fifteen seconds of a queue visibly not draining, with nothing on screen explaining the wait.
///
/// # Why `focus` as well as `visibilitychange`
///
/// **They are not the same event and the difference is the whole two-window case.** Two browser
/// windows side by side are *both* `document.visible`; clicking from one to the other fires `focus`
/// on the one gaining it and no `visibilitychange` at all. A client listening only for visibility
/// therefore never learns that the user has come back to it — which is exactly the report this
/// listener was added for. `visibilitychange` still earns its place: it is the one that fires when
/// a tab is backgrounded behind another tab in the same window, where `focus` does not.
///
/// Note the cost, because it is real and it is the same trade `refetchOnWindowFocus` makes: opening
/// devtools blurs the page, so the poll `crate::session` gates on this pauses while they are open.
///
/// # Why the application owns this and the adapter does not
///
/// `frontbox_dioxus::use_bfcache_wake` exists for `pageshow`, and the adapter stops there on
/// purpose: a back/forward-cache restore is a *correctness* event — the page's timers did not fire
/// while it was frozen, so any pending wait is measuring an interval that no longer means anything.
/// A tab merely being hidden is different. Its timers keep running and nothing is wrong; what is
/// true is that nobody is looking, which is a **policy** judgement about whether to spend requests.
///
/// `Wake` is public so an application can supply its own event, and this is that
/// (`crates/frontbox-dioxus/src/sync/wait.rs`).
#[cfg(target_arch = "wasm32")]
pub fn app_wake() -> (frontbox_dioxus::Wake, Foreground) {
    use wasm_bindgen::prelude::*;

    // The adapter's `pageshow` wake is the base, and both listeners below fire the same one rather
    // than creating their own. One `Wake` is now genuinely shared rather than merely intended to
    // be: it releases every sleeper waiting on it, which is what lets the drain loop and the
    // invalidation poll both hear a single event (`crates/frontbox-dioxus/src/sync/wait.rs`).
    let base = frontbox_dioxus::use_bfcache_wake();
    let wake = dioxus::prelude::use_hook(|| {
        let wake = base;
        let Some(window) = web_sys::window() else {
            // No window is not a failure worth refusing to start over: the loop still runs on its
            // budget, it simply never gets woken early.
            return VisibilityWake {
                wake,
                listener: None,
            };
        };
        let Some(document) = window.document() else {
            return VisibilityWake {
                wake,
                listener: None,
            };
        };

        let on_visible = wake.clone();
        let visibility = Closure::<dyn FnMut()>::new(move || {
            // `visible` only. `hidden` needs no wake — the loop is already waiting, and telling it
            // to go around again the moment nobody is watching is the opposite of the point.
            // `hidden()` rather than `visibility_state()`: the boolean is on the `Document`
            // feature this crate already takes, and the enum would add another for a distinction
            // — `prerender` — that nothing here acts on differently.
            if web_sys::window()
                .and_then(|window| window.document())
                .is_some_and(|document| !document.hidden())
            {
                on_visible.wake();
            }
        });
        let _ = document.add_event_listener_with_callback(
            "visibilitychange",
            visibility.as_ref().unchecked_ref(),
        );

        // Unconditional: a `focus` event on the window *is* the window gaining focus, so there is
        // no state left to check the way there is for `visibilitychange`.
        let on_focus = wake.clone();
        let focus = Closure::<dyn FnMut()>::new(move || on_focus.wake());
        let _ = window.add_event_listener_with_callback("focus", focus.as_ref().unchecked_ref());

        VisibilityWake {
            wake,
            // `Rc` because `use_hook` hands back a *clone* of its state on every render and those
            // clones are dropped at the end of it — a bare listener would unregister itself on the
            // first re-render. The same trap `use_bfcache_wake` documents.
            listener: Some(std::rc::Rc::new(Listener {
                document,
                visibility,
                window,
                focus,
            })),
        }
    })
    .wake;
    (wake, Foreground)
}

/// A desktop window's focus and a phone's suspend, through the adapter's `native` feature.
///
/// # The same two questions, from a different event stream
///
/// The browser arm above answers "is anybody looking" from `document.hidden` and
/// `document.hasFocus()`, and gets told about changes by `visibilitychange` and `focus`. A native
/// target has both halves too, they just arrive on tao's event loop instead: a desktop window
/// reports `WindowEvent::Focused`, and iOS and Android report `Suspended`/`Resumed` for the OS
/// putting the whole application away. `frontbox_dioxus::use_lifecycle_wake` is that, and the
/// reason it is the adapter's rather than this file's is written on the module itself — an OS
/// suspending a process is the same *correctness* event as a frozen page, not the policy judgement
/// a hidden tab is.
#[cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "desktop", feature = "mobile")
))]
pub fn app_wake() -> (frontbox_dioxus::Wake, Foreground) {
    let lifecycle = frontbox_dioxus::use_lifecycle_wake();
    (lifecycle.wake(), Foreground(lifecycle))
}

/// A native build with no renderer feature: nothing to observe, so nothing is claimed.
///
/// **This arm is load-bearing, not defensive.** `cargo test --workspace` and a plain
/// `cargo build -p todo-app` compile this crate with the default `web` feature *on a native host*,
/// so `not(target_arch = "wasm32")` holds while neither renderer feature does. Without this the
/// workspace stops compiling — and the failure would look like a mistake in the arm above rather
/// than a missing case.
#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(feature = "desktop", feature = "mobile"))
))]
pub fn app_wake() -> (frontbox_dioxus::Wake, Foreground) {
    (
        dioxus::prelude::use_hook(frontbox_dioxus::Wake::new),
        Foreground,
    )
}

/// Whether anybody is looking at this client.
///
/// # Why this is a handle and not a function
///
/// On web the answer is two synchronous DOM reads and a free function would do. On native it is
/// state maintained by an event handler, and the loop that asks is an `async` block with no Dioxus
/// context to reach through. Making it a value the tree already carries — `Ui::foreground` — is
/// what lets `session::focused` be one line with **no `cfg` at all**, which is what this module's
/// own header claims the file is for.
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy)]
pub struct Foreground;

/// Whether anybody is looking at this client. See the web arm for why this is a handle.
#[cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "desktop", feature = "mobile")
))]
#[derive(Clone)]
pub struct Foreground(frontbox_dioxus::Lifecycle);

/// Whether anybody is looking at this client. See the web arm for why this is a handle.
#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(feature = "desktop", feature = "mobile"))
))]
#[derive(Clone, Copy)]
pub struct Foreground;

impl Foreground {
    /// Whether this client is in front *now*.
    #[cfg(target_arch = "wasm32")]
    pub fn is_foreground(&self) -> bool {
        web_sys::window()
            .and_then(|window| window.document())
            // **Both defaults are `true`, for one reason.** A browser that will not answer should
            // leave the client polling, because the failure mode of guessing "nobody is looking" is
            // a window that silently stops updating and never says why.
            //
            // The outer half used to be `is_some_and`, which defaults the other way: no window or
            // no document — a worker realm, a page being torn down — read as "nobody is looking"
            // and paused the poll. That contradicted this very comment, the `app_wake` fallback
            // above ("the loop still runs on its budget"), and the no-renderer arm below ("yes, and
            // keep polling"). Three statements of the rule and one implementation of its opposite.
            .is_none_or(|document| !document.hidden() && document.has_focus().unwrap_or(true))
    }

    /// Whether this client is in front *now*.
    #[cfg(all(
        not(target_arch = "wasm32"),
        any(feature = "desktop", feature = "mobile")
    ))]
    pub fn is_foreground(&self) -> bool {
        self.0.is_foreground()
    }

    /// A build with nothing to ask answers the same way the browser arm does when the browser will
    /// not answer: yes, and keep polling.
    #[cfg(all(
        not(target_arch = "wasm32"),
        not(any(feature = "desktop", feature = "mobile"))
    ))]
    pub fn is_foreground(&self) -> bool {
        true
    }
}

/// The hook's state: the wake to hand out, and the listener's lifetime.
#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
struct VisibilityWake {
    wake: frontbox_dioxus::Wake,
    #[allow(dead_code)] // Held for its `Drop`, which is what unregisters the listener.
    listener: Option<std::rc::Rc<Listener>>,
}

/// Removes both listeners when the component that registered them goes away.
///
/// One type for two registrations rather than two types: they share a lifetime and an owner, and
/// splitting them would make it possible to drop one and keep the other.
#[cfg(target_arch = "wasm32")]
struct Listener {
    document: web_sys::Document,
    visibility: wasm_bindgen::closure::Closure<dyn FnMut()>,
    window: web_sys::Window,
    focus: wasm_bindgen::closure::Closure<dyn FnMut()>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for Listener {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast;
        let _ = self.document.remove_event_listener_with_callback(
            "visibilitychange",
            self.visibility.as_ref().unchecked_ref(),
        );
        let _ = self
            .window
            .remove_event_listener_with_callback("focus", self.focus.as_ref().unchecked_ref());
    }
}

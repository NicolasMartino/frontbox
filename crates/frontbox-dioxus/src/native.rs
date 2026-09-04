//! The one seam a desktop window and a phone application share, behind the `native` feature.
//!
//! # Why this exists at all, having been argued against
//!
//! `platform::app_wake` in this repository's trial used to carry a comment saying the adapter
//! "needs no `mobile` feature of its own", and the reasoning was that a tab merely being hidden is
//! a *policy* judgement — nobody is looking — while a back/forward-cache restore is a *correctness*
//! event, because the page's timers did not fire and any pending wait is now measuring an interval
//! that no longer means anything.
//!
//! **An operating system suspending an application is the correctness event, by that argument's own
//! terms.** The process stops being scheduled; its timers do not fire; the wall clock moves on
//! without it. That is the frozen page exactly, and it happens on two of this crate's four targets.
//! So this module is the native half of [`use_bfcache_wake`](crate::use_bfcache_wake), and the
//! earlier comment was drawing the line in the wrong place rather than drawing a line that was not
//! there.
//!
//! # One handler, three platforms, two different signals
//!
//! `dioxus::desktop` and `dioxus::mobile` are the same crate — `dioxus` aliases both to
//! `dioxus_desktop` — so one `use_wry_event_handler` covers a desktop window, an iPhone and an
//! Android device. What differs is which event actually means "the user is here":
//!
//! | Target | The signal that means in front | Why |
//! | --- | --- | --- |
//! | macOS, Windows, Linux | `WindowEvent::Focused(bool)` | `Suspended`/`Resumed` are **never** emitted on a desktop backend |
//! | iOS | `Event::Resumed` / `Event::Suspended` | `applicationDidBecomeActive` / `applicationWillResignActive` |
//! | Android | either | `onResume`/`onPause` *and* `onWindowFocusChanged` |
//!
//! So this observes **both** and lets the most recent one win, which is the only arrangement that
//! is correct on all three without asking the caller which platform it is on.
//!
//! **iOS's `Focused` is not app foreground and must not be read as it.** It comes from
//! `becomeKeyWindow`/`resignKeyWindow` on a `UIWindow`, so in a single-window application it fires
//! about once, at launch. Observing it anyway is harmless — if a system alert steals key window,
//! `applicationDidBecomeActive` restores the flag when the alert goes away — and special-casing it
//! would mean a `cfg` that buys nothing.
//!
//! # Desktop is wired and does not work, and the reason is upstream
//!
//! `WindowEvent::Focused` is the right signal on macOS, Windows and Linux, tao emits it, and the
//! arm below matches it. **No window event reaches a handler in a `dx serve` desktop build.**
//! `dioxus-desktop` drops every `Event::WindowEvent` whose `window_id` differs from the one the
//! handler registered against, and the two do not match here. Measured by dumping the raw stream:
//! 350 events across two deliberate window resizes and three focus changes, of which
//! `MainEventsCleared`, `RedrawEventsCleared`, `NewEvents` and `UserEvent` were all of them and
//! `WindowEvent` was none — a resize proves it is the filter and not the absence of focus changes.
//!
//! Asking the window instead does not rescue it: `use_window().is_focused()` reports `false` while
//! the window is frontmost and focused, which is the same mismatch seen from the other side. A
//! gate built on it pauses a healthy loop within seconds, so this deliberately does **not** poll
//! it — a wrong answer is worse than no answer, because the failure is a client that silently
//! stops updating.
//!
//! So a desktop build observes the same events as everything else and simply never sees one that
//! changes the flag, which leaves it permanently in the foreground: the behaviour it had before
//! this module existed. The route that would work is
//! `Config::with_custom_event_handler`, which `dioxus-desktop` calls with the unfiltered event —
//! it is a launch-time configuration rather than a hook, so it belongs to an application, and it
//! is not worth threading through while register entry 20 stops the desktop executor after a few
//! seconds regardless of what wakes it.
//!
//! # What the platform does not offer
//!
//! **True background/foreground is not available on iOS through this stack.** tao registers
//! `applicationWillEnterForeground:` and `applicationDidEnterBackground:` and leaves both bodies
//! empty, so active/inactive is the whole of what reaches us. That also fires for transient
//! interruptions — the control centre, an incoming call, the app switcher — which for a client
//! deciding whether to spend requests is arguably the better line anyway. It is recorded because it
//! is a limit inherited rather than a behaviour chosen.

use std::cell::Cell;
use std::rc::Rc;

use dioxus_desktop::tao::event::{Event, WindowEvent};
use dioxus_desktop::use_wry_event_handler;

use crate::sync::Wake;

/// Whether this application is in front, and a [`Wake`] that fires when it comes back.
///
/// Cheap to clone: a [`Wake`] and one shared flag, both reference-counted.
#[derive(Clone, Debug)]
pub struct Lifecycle {
    wake: Wake,
    foreground: Rc<Cell<bool>>,
}

impl Lifecycle {
    /// Fold one observation into the flag, and wake if it means the user came back.
    ///
    /// Separate from the handler so the rule is testable without an event loop: the wake fires on a
    /// *transition* to the front and on nothing else. Re-firing on every `Resumed` would release a
    /// wait that no event had invalidated, and firing on the way out would tell a loop already
    /// waiting to go around again at the exact moment nobody is watching.
    fn observe(&self, in_front: bool) {
        if self.foreground.get() == in_front {
            return;
        }
        self.foreground.set(in_front);
        if in_front {
            self.wake.wake();
        }
    }

    /// The wake to hand to [`Sleeper::wakeable`](crate::Sleeper::wakeable).
    ///
    /// Fired when the application returns to the front. Not fired when it leaves: a loop that is
    /// already waiting needs no telling to go around again at the moment nobody is watching.
    #[must_use]
    pub fn wake(&self) -> Wake {
        self.wake.clone()
    }

    /// Whether the application is in front *now*.
    ///
    /// Read rather than awaited, because the caller is a loop deciding whether to do its next round
    /// rather than a component deciding what to render.
    #[must_use]
    pub fn is_foreground(&self) -> bool {
        self.foreground.get()
    }
}

/// Observe this application's focus and lifecycle, on whichever native platform it is running.
///
/// The returned [`Lifecycle`] starts in the foreground. **Assuming the front is the safe default**
/// for the same reason the browser half reads `document.hasFocus().unwrap_or(true)`: the failure
/// mode of wrongly believing nobody is looking is a client that silently stops updating and never
/// says why, and the failure mode of wrongly believing somebody is, is one extra request.
///
/// The handler unregisters itself when the calling component is dropped —
/// `use_wry_event_handler` is built on `use_hook_with_cleanup`, so unlike the `web` module's
/// listener this needs no `Rc` kept alive by hand.
pub fn use_lifecycle_wake() -> Lifecycle {
    let lifecycle = dioxus_core::use_hook(|| Lifecycle {
        wake: Wake::new(),
        foreground: Rc::new(Cell::new(true)),
    });

    let observed = lifecycle.clone();
    // The closure's argument types are deliberately left to inference: the event carries
    // `dioxus_desktop::ipc::UserWindowEvent`, whose module is private and never re-exported, so the
    // type cannot be named from outside that crate even though the value is handed to us.
    use_wry_event_handler(move |event, _| {
        if let Some(in_front) = in_front(event) {
            observed.observe(in_front);
        }
    });

    lifecycle
}

/// What one event says about being in front, or [`None`] if it says nothing.
///
/// # Why this is a free function and generic
///
/// The handler above cannot name the type of its own argument — the user event is
/// `dioxus_desktop::ipc::UserWindowEvent`, in a private module — so the matching had to live inside
/// a closure whose parameters were inferred, and a closure like that cannot be called from a test.
/// Lifting it out with the user-event type as a parameter costs nothing at the call site (it is
/// still inferred) and makes the table in this module's header executable rather than merely
/// asserted, which matters here more than usual: **no test in this crate can run an event loop**,
/// so without this the whole module would be reasoning with no evidence under it.
fn in_front<T>(event: &Event<'_, T>) -> Option<bool> {
    match event {
        // iOS and Android. Never emitted on a desktop backend, and the only signal that survives on
        // iOS — see the module header on why `Focused` will not do there.
        Event::Resumed => Some(true),
        Event::Suspended => Some(false),
        // Android's second signal, and the one a desktop window would use.
        Event::WindowEvent {
            event: WindowEvent::Focused(focused),
            ..
        } => Some(*focused),
        _ => None,
    }
}

#[cfg(test)]
mod tests;

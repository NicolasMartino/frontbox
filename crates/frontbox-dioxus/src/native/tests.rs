//! The tests for [`super`]: which events mean "in front", and when the wake fires.
//!
//! # What these can and cannot reach
//!
//! **No event loop runs here, and none can.** `use_lifecycle_wake` is a Dioxus hook wrapping a wry
//! handler registration, so the *wiring* — that tao delivers these events to that handler — is only
//! provable on a device, and `wiki/decisions/043-in-front-on-every-platform.decision.md` records the
//! iOS and Android runs that did it.
//!
//! **The `WindowEvent::Focused` arm is not reachable from here either**, and for a reason worth
//! writing down rather than working around: `Event::WindowEvent` is `#[non_exhaustive]`, so no crate
//! but tao can construct one. The arm can be matched and not built. That leaves the desktop half of
//! the table in [`super`]'s header resting on tao's documentation and on the raw-stream dump
//! recorded there — which is the same evidence it always had, now stated instead of implied. It
//! compounds with the finding next door: `dioxus-desktop` filters those events out before a handler
//! sees them, so the arm is unexercised at runtime as well.
//!
//! What is provable here is the part that is pure decision and was previously buried in a closure
//! whose argument type cannot be named: the mobile mapping, the refusal to answer for anything else,
//! and the rule that the wake fires on a transition back to the front and on nothing else. That
//! rule is what a caller depends on — a wake per `Resumed` would release waits no event had
//! invalidated — and it had no evidence at all.

use std::cell::Cell;
use std::rc::Rc;

use dioxus_desktop::tao::event::{Event, StartCause};

use super::{in_front, Lifecycle};
use crate::sync::Wake;

/// The user-event parameter, which these tests never construct. The real one is
/// `dioxus_desktop::ipc::UserWindowEvent`, in a private module; the mapping does not read it.
type NoUserEvent = ();

fn lifecycle() -> Lifecycle {
    Lifecycle {
        wake: Wake::new(),
        foreground: Rc::new(Cell::new(true)),
    }
}

/// Whether the lifecycle's wake has something latched.
///
/// A sleeper is the only way to observe a [`Wake`] from outside, and `until_woken` on one whose
/// timer never fires is the smallest: it is ready exactly when the latch has something in it. Each
/// call mints a fresh cursor, so this asks "has it ever fired", not "has this waiter spent it".
fn latched(lifecycle: &Lifecycle) -> bool {
    poll_ready(crate::Sleeper::new(|_| std::future::pending()).wakeable(lifecycle.wake()))
}

fn poll_ready(sleeper: crate::Sleeper) -> bool {
    let mut waiting = Box::pin(sleeper.until_woken());
    std::future::Future::poll(
        waiting.as_mut(),
        &mut std::task::Context::from_waker(std::task::Waker::noop()),
    )
    .is_ready()
}

#[test]
fn resume_and_suspend_are_the_mobile_answer() {
    // The only pair that survives on iOS: `applicationDidBecomeActive` and
    // `applicationWillResignActive`, which is the whole of what tao forwards there.
    assert_eq!(in_front::<NoUserEvent>(&Event::Resumed), Some(true));
    assert_eq!(in_front::<NoUserEvent>(&Event::Suspended), Some(false));
}

#[test]
fn every_other_event_says_nothing() {
    // The stream is overwhelmingly these — 350 events across two resizes and three focus changes,
    // measured, of which `MainEventsCleared`, `RedrawEventsCleared`, `NewEvents` and `UserEvent`
    // were all of them. Answering `false` for an event that is not about focus would park a healthy
    // loop, which is the failure the module header refuses to risk.
    assert_eq!(
        in_front::<NoUserEvent>(&Event::NewEvents(StartCause::Poll)),
        None
    );
    assert_eq!(in_front::<NoUserEvent>(&Event::MainEventsCleared), None);
    assert_eq!(in_front::<NoUserEvent>(&Event::RedrawEventsCleared), None);
    assert_eq!(in_front::<NoUserEvent>(&Event::LoopDestroyed), None);
}

#[test]
fn a_new_lifecycle_assumes_it_is_in_front() {
    // Stated as a test because the alternative fails silently: a client that starts by believing
    // nobody is looking stops polling and never says why. See `use_lifecycle_wake`.
    assert!(lifecycle().is_foreground());
}

#[test]
fn coming_back_to_the_front_fires_the_wake() {
    let lifecycle = lifecycle();

    lifecycle.observe(false);
    assert!(!lifecycle.is_foreground(), "suspending is observed");
    assert!(
        !latched(&lifecycle),
        "and leaving does not wake: a loop already waiting needs no telling to go around again at \
         the moment nobody is watching"
    );

    lifecycle.observe(true);
    assert!(lifecycle.is_foreground());
    assert!(latched(&lifecycle), "coming back is what ends the wait");
}

#[test]
fn a_repeated_signal_is_not_a_second_wake() {
    // Android reports `onResume` *and* `onWindowFocusChanged`, so one transition arrives twice.
    // Waking per event rather than per transition would release a wait the second event had not
    // invalidated — and on a platform that emits both, that is every single resume.
    let lifecycle = lifecycle();
    lifecycle.observe(false);
    lifecycle.observe(true);
    lifecycle.observe(true);

    // One sleeper polled twice, rather than `latched` twice: this is the caller's shape, where a
    // single long-lived waiter spends the latch once and then waits for real.
    let sleeper = crate::Sleeper::new(|_| std::future::pending()).wakeable(lifecycle.wake());
    assert!(poll_ready(sleeper.clone()), "the transition woke it");
    assert!(
        !poll_ready(sleeper),
        "and the duplicate did not: one return to the front is one wake"
    );
}

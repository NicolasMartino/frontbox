//! The tests for [`super`], which are the specification of the latch.
//!
//! In their own file because `wait.rs` passed the four-hundred-line cap `AGENTS.md`
//! sets when the multi-waiter cases were added — the same split `src/rfc3339.rs` uses.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::*;

/// A sleep that never finishes, so only the wake can end the wait.
fn never() -> Sleeper {
    Sleeper::new(|_| std::future::pending())
}

/// A waker that counts how many times it was asked to reschedule.
///
/// `Waker::noop()` would prove the latch and nothing else. What makes the wake work in a real
/// executor is that it *notifies*, and a future that returns `Pending` without arranging to be
/// polled again is a hang no assertion on a noop waker can see.
#[derive(Default)]
struct Counter(AtomicUsize);

impl std::task::Wake for Counter {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

fn poll_once<F: Future>(future: &mut Pin<Box<F>>, waker: &Waker) -> Poll<F::Output> {
    future.as_mut().poll(&mut Context::from_waker(waker))
}

#[test]
fn a_wake_ends_a_wait_in_progress() {
    let counter = Arc::new(Counter::default());
    let waker = Waker::from(Arc::clone(&counter));
    let wake = Wake::new();
    let sleeper = never().wakeable(wake.clone());

    let mut waiting = Box::pin(sleeper.sleep(5_000));
    assert!(poll_once(&mut waiting, &waker).is_pending());

    wake.wake();
    assert_eq!(
        counter.0.load(Ordering::Relaxed),
        1,
        "the wait must be rescheduled, not merely marked"
    );
    assert!(poll_once(&mut waiting, &waker).is_ready());
}

#[test]
fn a_wake_with_nothing_waiting_is_latched() {
    let waker = Waker::from(Arc::new(Counter::default()));
    let wake = Wake::new();
    wake.wake();

    // The restore arrived mid-drain. Dropping it would mean the next wait — chosen before the
    // page was frozen — runs to completion anyway.
    let sleeper = never().wakeable(wake);
    let mut waiting = Box::pin(sleeper.sleep(5_000));
    assert!(poll_once(&mut waiting, &waker).is_ready());
}

#[test]
fn a_latch_is_spent_once() {
    let waker = Waker::from(Arc::new(Counter::default()));
    let wake = Wake::new();
    wake.wake();
    let sleeper = never().wakeable(wake);

    assert!(poll_once(&mut Box::pin(sleeper.sleep(5_000)), &waker).is_ready());
    assert!(
        poll_once(&mut Box::pin(sleeper.sleep(5_000)), &waker).is_pending(),
        "one restore is one wake; the wait after it is a real wait"
    );
}

#[test]
fn a_sleeper_without_a_wake_still_sleeps() {
    let waker = Waker::from(Arc::new(Counter::default()));
    let sleeper = never();
    let mut waiting = Box::pin(sleeper.sleep(5_000));
    assert!(poll_once(&mut waiting, &waker).is_pending());

    // And a wake nobody wired in changes nothing, which is what `wakeable` being explicit buys.
    Wake::new().wake();
    assert!(poll_once(&mut waiting, &waker).is_pending());
}

#[test]
fn the_timer_still_ends_the_wait() {
    let waker = Waker::from(Arc::new(Counter::default()));
    let sleeper = Sleeper::new(|_| std::future::ready(())).wakeable(Wake::new());
    assert!(poll_once(&mut Box::pin(sleeper.sleep(5_000)), &waker).is_ready());
}

#[test]
fn one_wake_releases_every_sleeper_waiting_on_it() {
    // Two loops, two sleepers, one wake — which is what the trial actually does: a drain loop
    // on a five-second cadence and an invalidation poll on its own, both handed
    // `platform::app_wake()`. A single-slot latch releases whichever registered last and the
    // other serves out an interval chosen before the event that made it meaningless.
    let (first_count, second_count) = (Arc::new(Counter::default()), Arc::new(Counter::default()));
    let first_waker = Waker::from(Arc::clone(&first_count));
    let second_waker = Waker::from(Arc::clone(&second_count));
    let wake = Wake::new();
    let (first, second) = (
        never().wakeable(wake.clone()),
        never().wakeable(wake.clone()),
    );

    let mut waiting_first = Box::pin(first.sleep(5_000));
    let mut waiting_second = Box::pin(second.sleep(600_000));
    assert!(poll_once(&mut waiting_first, &first_waker).is_pending());
    assert!(poll_once(&mut waiting_second, &second_waker).is_pending());

    wake.wake();

    assert_eq!(
        first_count.0.load(Ordering::Relaxed),
        1,
        "the first waiter must be rescheduled"
    );
    assert_eq!(
        second_count.0.load(Ordering::Relaxed),
        1,
        "and so must the second"
    );
    assert!(poll_once(&mut waiting_first, &first_waker).is_ready());
    assert!(
        poll_once(&mut waiting_second, &second_waker).is_ready(),
        "one event, every wait: the second sleeper is the ten-minute one, and leaving it \
         asleep is a tab that never polls again"
    );
}

#[test]
fn each_waiter_spends_the_latch_once_on_its_own() {
    // The per-waiter cursor, stated as a test: a wake released both, and neither gets a second
    // free pass out of the same fire.
    let waker = Waker::from(Arc::new(Counter::default()));
    let wake = Wake::new();
    let (first, second) = (
        never().wakeable(wake.clone()),
        never().wakeable(wake.clone()),
    );
    wake.wake();

    assert!(poll_once(&mut Box::pin(first.sleep(5_000)), &waker).is_ready());
    assert!(poll_once(&mut Box::pin(second.sleep(5_000)), &waker).is_ready());
    assert!(poll_once(&mut Box::pin(first.sleep(5_000)), &waker).is_pending());
    assert!(poll_once(&mut Box::pin(second.sleep(5_000)), &waker).is_pending());
}

#[test]
fn a_pending_wait_registers_one_waker_however_often_it_is_polled() {
    let waker = Waker::from(Arc::new(Counter::default()));
    let wake = Wake::new();
    let sleeper = never().wakeable(wake.clone());
    let mut waiting = Box::pin(sleeper.sleep(5_000));
    for _ in 0..10 {
        assert!(poll_once(&mut waiting, &waker).is_pending());
    }
    assert_eq!(
        wake.0.wakers.borrow().len(),
        1,
        "a re-poll re-registers the same waker; appending would grow this for the life of the loop"
    );
}

#[test]
fn a_second_waker_replaces_the_one_the_waiter_parked() {
    // An executor may poll the same future with a *different* waker, and the future is obliged to
    // wake the most recent one. Deduplicating by `will_wake` alone satisfies that obligation by
    // accident — it keeps both — and leaves a retired waker parked until the next fire. Keyed by
    // waiter, the second poll replaces the first.
    let (first, second) = (Arc::new(Counter::default()), Arc::new(Counter::default()));
    let first_waker = Waker::from(Arc::clone(&first));
    let second_waker = Waker::from(Arc::clone(&second));
    let wake = Wake::new();
    let sleeper = never().wakeable(wake.clone());
    let mut waiting = Box::pin(sleeper.sleep(5_000));

    assert!(poll_once(&mut waiting, &first_waker).is_pending());
    assert!(poll_once(&mut waiting, &second_waker).is_pending());
    assert_eq!(
        wake.0.wakers.borrow().len(),
        1,
        "one waiter parks one waker, however many the executor hands it"
    );

    wake.wake();
    assert_eq!(
        second.0.load(Ordering::Relaxed),
        1,
        "the waker from the most recent poll is the one woken"
    );
    assert_eq!(
        first.0.load(Ordering::Relaxed),
        0,
        "and the one it replaced is not still held against a task that has moved on"
    );
    assert!(poll_once(&mut waiting, &second_waker).is_ready());
}

#[test]
fn two_waiters_each_park_their_own_entry() {
    // The other half of keying by waiter: replacement must not collapse *different* waiters into
    // one slot. Both share a waker here, which is the case `will_wake` would have deduplicated.
    let waker = Waker::from(Arc::new(Counter::default()));
    let wake = Wake::new();
    let (first, second) = (
        never().wakeable(wake.clone()),
        never().wakeable(wake.clone()),
    );

    let mut waiting_first = Box::pin(first.sleep(5_000));
    let mut waiting_second = Box::pin(second.sleep(5_000));
    assert!(poll_once(&mut waiting_first, &waker).is_pending());
    assert!(poll_once(&mut waiting_second, &waker).is_pending());
    assert_eq!(
        wake.0.wakers.borrow().len(),
        2,
        "two waiters are two entries, even sharing one waker"
    );
}

#[test]
fn until_woken_waits_for_the_wake_and_runs_no_timer() {
    let waker = Waker::from(Arc::new(Counter::default()));
    let wake = Wake::new();
    // A timer that would resolve instantly, to prove `until_woken` never consults it.
    let sleeper = Sleeper::new(|_| std::future::ready(())).wakeable(wake.clone());

    let mut paused = Box::pin(sleeper.until_woken());
    assert!(
        poll_once(&mut paused, &waker).is_pending(),
        "a pause is not a sleep"
    );

    wake.wake();
    assert!(poll_once(&mut paused, &waker).is_ready());
}

#[test]
fn until_woken_without_a_wake_returns_rather_than_hanging() {
    // A wait nothing can end is a hang, not a policy. The caller's loop re-checks its own
    // condition instead, which is the only thing that could have changed.
    let waker = Waker::from(Arc::new(Counter::default()));
    assert!(poll_once(&mut Box::pin(never().until_woken()), &waker).is_ready());
}

thread_local! {
    /// The wait `PollsInline` re-enters. A `thread_local` because `std::task::Wake` demands a
    /// `Send + Sync` waker and every future in this crate is neither.
    static REENTERED: RefCell<Option<Pin<Box<dyn Future<Output = ()>>>>> =
        const { RefCell::new(None) };
}

/// A waker that polls its task *inline*, from inside `wake()`.
///
/// Dioxus's waker only sends a channel message, so this shape is not reachable today — which is
/// exactly why it is worth pinning. `Wake::wake` used to hold a `RefMut` on its waker slot
/// across the call, and under edition 2021 an `if let` scrutinee's temporary lives for the whole
/// body, so an executor that polled inline would have panicked with "already mutably borrowed".
struct PollsInline;

impl std::task::Wake for PollsInline {
    fn wake(self: Arc<Self>) {
        let waker = Waker::from(Arc::new(PollsInline));
        REENTERED.with(|slot| {
            if let Some(waiting) = slot.borrow_mut().as_mut() {
                let _ = waiting.as_mut().poll(&mut Context::from_waker(&waker));
            }
        });
    }
}

#[test]
fn a_waker_that_polls_inline_does_not_re_enter_a_held_borrow() {
    let wake = Wake::new();
    // Leaked so the wait can be `'static` and live in the thread-local the waker reaches.
    let sleeper: &'static Sleeper = Box::leak(Box::new(never().wakeable(wake.clone())));
    REENTERED.with(|slot| *slot.borrow_mut() = Some(Box::pin(sleeper.sleep(5_000))));

    let waker = Waker::from(Arc::new(PollsInline));
    REENTERED.with(|slot| {
        let mut held = slot.borrow_mut();
        let waiting = held.as_mut().expect("just installed");
        assert!(waiting
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending());
    });

    // The assertion is that this returns at all.
    wake.wake();

    REENTERED.with(|slot| *slot.borrow_mut() = None);
}

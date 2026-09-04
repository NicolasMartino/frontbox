//! How the loop waits, and what can cut a wait short.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// What a [`Sleeper`] hands back.
///
/// Not `Send`, like every other future in both crates: an IndexedDB future cannot be made one,
/// and a timer that demanded it would rule out the target this crate exists for.
type SleepFuture = Pin<Box<dyn Future<Output = ()>>>;

/// How the loop waits.
///
/// Boxed behind an `Rc` rather than carried as two more generic parameters, so the hook's signature
/// stays readable and the closure can be cloned per render. The future is not `Send`, matching
/// every other future in both crates.
#[derive(Clone)]
pub struct Sleeper {
    sleep: Rc<dyn Fn(u32) -> SleepFuture>,
    waiter: Option<Waiter>,
}

/// One loop's standing in a shared [`Wake`].
///
/// # Why the cursor lives here and not in `Wake`
///
/// A `Wake` is fired by *events* and awaited by *loops*, and there is no reason for those to be
/// one-to-one — this crate's own trial hands the same wake to a drain loop and an invalidation
/// poll. So "has this waiter seen the latest wake" is a fact about the waiter, and putting it in
/// the shared state would make one loop's wait consume the other's.
///
/// Cloned with the [`Sleeper`], and the `Rc` is what makes those clones one waiter: the hook
/// rebuilds its sleeper on every render, and a per-clone cursor would forget every wake.
#[derive(Clone)]
struct Waiter {
    wake: Wake,
    /// The generation this waiter has already been released for.
    seen: Rc<Cell<u64>>,
}

impl Waiter {
    /// This waiter's identity, for the shared waker table.
    ///
    /// The cursor's address, because the cursor is *already* what makes clones of a [`Sleeper`] one
    /// waiter and separate `wakeable` calls two — so reusing it needs no second field and cannot
    /// disagree with the thing it identifies. An address freed and reissued to a later waiter would
    /// at worst overwrite a dead waiter's parked waker, which is the outcome that was wanted anyway.
    fn id(&self) -> usize {
        Rc::as_ptr(&self.seen) as usize
    }
}

impl Sleeper {
    /// Build one from whatever timer the application already has.
    ///
    /// `gloo_timers::future::TimeoutFuture::new` on web and `tokio::time::sleep` on desktop both
    /// fit directly.
    pub fn new<F, Fut>(sleep: F) -> Self
    where
        F: Fn(u32) -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        Self {
            sleep: Rc::new(move |ms| Box::pin(sleep(ms))),
            waiter: None,
        }
    }

    /// Let `wake` cut any wait short.
    ///
    /// The wait a loop is serving is a bet about what happens next, and some events settle that bet
    /// early. The one this exists for is a back/forward cache restore: a frozen page's timers do not
    /// fire, so a `Sleeper` stalls for however long the page sat in the cache and then resumes as
    /// though its wait had been continuous — while the wall clock has jumped forward by minutes. A
    /// page restored after ten minutes should drain, not serve out the remainder of a five-second
    /// interval chosen for a live page (`wiki/references/open-decisions.reference.md`, entry 16).
    ///
    /// A wake that fires while nothing is waiting is *latched*, not lost: the next wait returns
    /// immediately. Dropping it would mean a wake arriving mid-drain silently did nothing.
    ///
    /// **The latch is per sleeper, so one wake releases every loop waiting on it.** Each call here
    /// mints a fresh cursor, which is what lets two loops share one `Wake` without racing for it —
    /// see entry 16's note on the two consumers this crate's trial actually has.
    #[must_use]
    pub fn wakeable(mut self, wake: Wake) -> Self {
        self.waiter = Some(Waiter {
            wake,
            seen: Rc::new(Cell::new(0)),
        });
        self
    }

    /// Wait for `ms` milliseconds, or until the wake fires.
    pub async fn sleep(&self, ms: u32) {
        let sleeping = (self.sleep)(ms);
        match &self.waiter {
            None => sleeping.await,
            Some(waiter) => {
                Race {
                    sleeping,
                    woken: Woken(waiter),
                }
                .await;
            }
        }
    }

    /// Wait for the wake and nothing else, with no timer running at all.
    ///
    /// This is a *pause*, not a long sleep, and the difference is the point: a loop that has
    /// decided it should not be running — the trial's invalidation poll, when nobody is looking at
    /// the tab — has no interval to serve out, and expressing that as a very large `sleep` would be
    /// a schedule pretending to be a policy.
    ///
    /// **A sleeper with no wake attached returns immediately** rather than waiting forever. A wait
    /// nothing can end is a hang, and hanging is never what a caller meant; returning lets the
    /// caller's loop re-check its own condition, which is the only thing that could have changed.
    pub async fn until_woken(&self) {
        if let Some(waiter) = &self.waiter {
            Woken(waiter).await;
        }
    }
}

impl std::fmt::Debug for Sleeper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Sleeper(..)")
    }
}

/// A signal that a wait should end now.
///
/// Cloneable and cheap: every clone fires the same latch. Hand it to
/// [`Sleeper::wakeable`] and keep one for whatever decides the wait is over —
/// [`use_bfcache_wake`](crate::use_bfcache_wake) behind the `web` feature, or an application's own
/// listener on a platform this crate does not cover.
///
/// **Any number of sleepers may wait on one wake, and a fire releases all of them.** That is worth
/// stating because the obvious implementation — a `bool` and one `Waker` — does not do it, and the
/// failure is silent: whichever loop happened to register last is woken and the rest serve out
/// intervals chosen before the event that made them meaningless.
#[derive(Clone, Default)]
pub struct Wake(Rc<WakeState>);

#[derive(Default)]
struct WakeState {
    /// Bumped once per fire. A waiter is released when this differs from what it last saw, which
    /// is what makes the latch survive arriving while nobody was waiting *and* be spendable once
    /// per waiter rather than once in total.
    generation: Cell<u64>,
    /// One entry per waiter currently parked, keyed by [`Waiter::id`].
    ///
    /// Keyed rather than a bare `Vec<Waker>`: an executor may hand a future a *different* waker on
    /// a later poll, and deduplicating by `Waker::will_wake` alone then keeps the old one and adds
    /// the new. That is not a missed wake — the current waker is in the list too, and waking a
    /// retired one is a no-op by contract — but it holds a task reference that nothing will use
    /// until the next fire, and it made the comment beside the registration say "replaced" about
    /// code that only ever appended.
    wakers: RefCell<Vec<(usize, Waker)>>,
}

impl Wake {
    /// Build one that has not fired.
    pub fn new() -> Self {
        Self::default()
    }

    /// End every current wait, and the next one of any waiter that was not waiting.
    pub fn wake(&self) {
        // `wrapping_add` because the counter is only ever compared for *inequality*. At one fire
        // per user gesture it will not wrap this century, and a saturating counter would silently
        // stop waking anything if it ever did.
        self.0
            .generation
            .set(self.0.generation.get().wrapping_add(1));
        // Taken into a local **before** anything is woken, so the borrow is released first. A waker
        // that polls its task synchronously would otherwise re-enter `Woken::poll` while this
        // `RefMut` is still alive and panic on the second `borrow_mut`. Dioxus's waker only sends a
        // channel message today, which is precisely the kind of thing that is true until it is not.
        //
        // Draining rather than cloning: a waker is good for one wake, and each waiter re-registers
        // on its next poll.
        let woken = std::mem::take(&mut *self.0.wakers.borrow_mut());
        for (_, waker) in woken {
            waker.wake();
        }
    }
}

impl std::fmt::Debug for Wake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wake")
            .field("generation", &self.0.generation.get())
            .field("waiting", &self.0.wakers.borrow().len())
            .finish_non_exhaustive()
    }
}

/// Resolves the first time its [`Wake`] fires after this waiter last saw it.
struct Woken<'a>(&'a Waiter);

impl Future for Woken<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let waiter = self.0;
        let generation = waiter.wake.0.generation.get();
        if waiter.seen.get() != generation {
            waiter.seen.set(generation);
            return Poll::Ready(());
        }
        let mut wakers = waiter.wake.0.wakers.borrow_mut();
        // **Replaced, and now that is literally what happens.** A pending future is polled many
        // times before it resolves, and appending on each one would grow this vec for the life of
        // the loop. Keying by the waiter rather than by the waker is what makes the replacement
        // total: an executor is free to poll the same future with a new waker, and `will_wake`
        // alone would then retain the old one beside it.
        //
        // A waiter dropped while pending leaves its entry here until the next fire, which drains
        // the whole vec. That is bounded by the number of loops that have ever waited rather than
        // by time, and deregistering on drop would cost a `Drop` impl and a back-reference for a
        // handful of pointers.
        let id = waiter.id();
        match wakers.iter_mut().find(|(held, _)| *held == id) {
            Some((_, held)) => held.clone_from(cx.waker()),
            None => wakers.push((id, cx.waker().clone())),
        }
        Poll::Pending
    }
}

/// Whichever of a sleep and a wake finishes first.
///
/// Hand-written rather than `futures::select`: this crate has no futures dependency, and taking one
/// to race two futures — one of which is a `Pin<Box<_>>` already — would be a dependency per line
/// of code saved.
struct Race<'a> {
    sleeping: SleepFuture,
    woken: Woken<'a>,
}

impl Future for Race<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // `Race` is `Unpin` — a `Pin<Box<_>>` and a reference — so this is a move out of the `Pin`
        // rather than an assumption about it.
        let this = self.get_mut();
        // The wake is polled first so a latched one wins a race it arrived before, rather than
        // depending on whether the timer happened to be ready in the same poll.
        if Pin::new(&mut this.woken).poll(cx).is_ready() {
            return Poll::Ready(());
        }
        this.sleeping.as_mut().poll(cx)
    }
}

#[cfg(test)]
mod tests;

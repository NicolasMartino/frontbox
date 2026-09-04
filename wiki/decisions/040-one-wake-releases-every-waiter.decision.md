# One Wake Releases Every Waiter

Document Class: Decision
Status: Accepted 2026-09-01; built the same day, measured in a browser before and after. **Amended 2026-09-02**: the pending-waker table is keyed by waiter rather than deduplicated by `Waker::will_wake` — see `## Amendment, 2026-09-02: Keyed By Waiter`
Date: 2026-09-01
Category: Adapter Runtime
Scope: How many loops may wait on a single `Wake`, and what one fire is guaranteed to do to them.
Sources: `crates/frontbox-dioxus/src/sync/wait.rs`, `crates/frontbox-dioxus/src/sync/wait/tests.rs`, `examples/todo-app/src/platform/mod.rs`, `examples/todo-app/src/session.rs`, `examples/todo-app/src/main.rs`
Related: `wiki/references/open-decisions.reference.md` (entry 16), `wiki/proposals/browser-background-services.proposal.md`, `wiki/decisions/041-the-poll-stops-when-nobody-is-looking.decision.md`, `wiki/decisions/028-drain-loop-boundary.decision.md`

## Decision

**A `Wake` is a broadcast, not a handoff.** Any number of [`Sleeper`]s may wait on one, and a single
`wake()` releases every one of them. The latch that made a wake survive arriving while nothing was
waiting is kept, and it becomes **per waiter**: each sleeper spends it once, and one sleeper
spending it does not consume it for the others.

`Sleeper::until_woken` is added alongside, for a loop that wants the wake and no timer at all.

The public API does not change shape. `Wake::new`, `Wake::wake`, `Sleeper::wakeable` all keep their
signatures; what changes is what the second consumer of a shared wake is promised.

## The Defect This Closes

`Wake` held one `Cell<bool>` and one `Option<Waker>`:

```rust
struct WakeState {
    fired: Cell<bool>,
    waker: RefCell<Option<Waker>>,
}
```

`Woken::poll` consumed the latch with `fired.replace(false)` and *overwrote* the single waker slot.
Two futures waiting on one `Wake` therefore raced: the second to poll evicted the first's waker, and
whichever polled first after a fire consumed the latch. **One event released exactly one loop.**

Nothing said so. The type's own documentation discussed the latch as if there were one waiter ("a
wake that fires while nothing is waiting is latched, not lost"), and all five of its tests used one.

The trial had two, and its comments asserted the behaviour the type did not implement —
`examples/todo-app/src/session.rs`: *"The same wake the drain loop races, so one event cuts both
waits short"*, and `examples/todo-app/src/platform/mod.rs`: *"Two wakes would mean two sleepers, and the
drain loop can only wait on one."* Both describe a broadcast. Neither was getting one.

**Which loop lost was a race, and the losing one served out its full interval.** The drain loop
re-sleeps every five seconds and the invalidation poll every fifteen, so the drain loop usually held
the slot — and the poll, which is the loop that makes a second browser window see the first one's
writes, was the one that did not hear that the user had come back to the tab.

## The Evidence

Measured in the running trial before any code changed, by firing the wake through the offline switch
— which fires the same one — with both loops waiting, six times:

| | before | after |
| --- | --- | --- |
| both loops woke | **1 / 6** | **6 / 6** |
| only the drain loop woke | 3 / 6 | 0 / 6 |
| only the invalidation poll woke | 2 / 6 | 0 / 6 |
| neither | 0 / 6 | 0 / 6 |

After: both loops, every time, at 18–24 ms. The alternation between which loop won is the signature
of the single slot, and it is why the reported symptom was "it works sometimes".

## Why Broadcast Rather Than One Wake Per Loop

The alternative was to leave the type alone and give each loop its own `Wake`, with the application
firing all of them. It was rejected on two grounds.

**It puts the invariant in the wrong place.** The events that fire a wake — a bfcache restore, the
window regaining focus, the network coming back — are properties of the *page*, and there is exactly
one page. Modelling them as N parallel signals means every new listener has to know how many loops
exist, and every new loop has to be registered with every listener. The fan-out belongs at the point
where one event becomes many wake-ups, which is the wake.

**The failure mode of the old design is silent.** Nothing errors, nothing logs, and the loop that
lost is indistinguishable from one whose interval simply had not expired. A shape that is wrong in a
way no test and no console will show is worth removing rather than documenting around.

## How The Latch Survives

The requirement that a wake arriving mid-drain is not lost is the reason the old design had a
`bool`, and it does not survive a shared flag: one waiter clearing it takes it from the others.

So the shared state counts and the waiter remembers:

```rust
struct WakeState {
    generation: Cell<u64>,
    wakers: RefCell<Vec<(usize, Waker)>>,   // keyed by waiter; see the amendment below
}
```

`wake()` bumps `generation`. Each `Sleeper` carries its own cursor, minted by `wakeable` and starting
at zero, and is released whenever `generation` differs from what it last saw. A fire that lands while
a loop is mid-drain is still waiting for it when the loop next sleeps; a fire that releases two loops
is spent once by each; and a sleeper attached after a fire gets the one it missed, which is what
keeps the original latch tests passing unchanged.

The cursor lives in the `Sleeper` rather than beside the wake because a `Sleeper` is what a loop
holds, and its clones — the hook rebuilds one per render — share the cursor by `Rc` so a re-render
does not forget a wake.

## The Re-Entrancy Fixed On The Way Past

`Wake::wake` held a `RefMut` on its waker slot across the call to `waker.wake()`. Under edition
2021 an `if let` scrutinee's temporary lives for the whole body, so an executor that polled its task
*inline* would have re-entered `Woken::poll` and panicked on the second `borrow_mut`.

Dioxus's waker only sends a channel message, so this was not reachable. It is the kind of thing that
is not reachable until it is, and the new `wake()` takes the waker list into a local before waking
anything. `a_waker_that_polls_inline_does_not_re_enter_a_held_borrow` pins it with a waker that
polls inline, which is a test that could not have been written against the old shape without
asserting the panic.

## Consequences

- **Callers may share one wake freely**, which is what the trial was already doing and what the
  application-owned-listener half of entry 16 assumed.
- **`Sleeper::until_woken` makes "paused" expressible.** Before it, a loop that wanted to stop had to
  sleep for a number chosen to look like forever; decision 041 is what that enables.
- **A sleeper with no wake returns immediately from `until_woken`.** A wait nothing can end is a
  hang rather than a policy, and the caller's loop re-checking its own condition is the only useful
  thing left to do.
- **The waker table holds one entry per waiter**, so a pending future being polled many times
  registers once rather than growing the vector for the life of the loop. See the amendment below
  for why the key is the waiter and not the waker.

## Amendment, 2026-09-02: Keyed By Waiter

The table was a `Vec<Waker>` deduplicated with `Waker::will_wake`, and the comment beside the
registration said the entry was "replaced rather than appended". It was not. `will_wake` answers
*"would these two wake the same task"*, and an executor is entitled to hand a future a **different**
waker on a later poll — so the old one stayed and the new one was pushed beside it.

**Nothing was ever missed**, and that is worth stating plainly rather than dressing this up as a
correctness fix. The waker from the latest poll is in the table, so the waiter is released; waking a
retired waker is a no-op by contract. What was wrong was the cost and the claim: an entry per waker
churn, each holding a task reference nothing would use until the next fire, under a comment
asserting the opposite behaviour.

The key is `Rc::as_ptr` of the waiter's own cursor — the `Rc` that already decides which sleepers
are one waiter and which are two, so the identity cannot drift from the thing it identifies and no
new field is needed. Replacement is now total, and the two halves of it are pinned by
`a_second_waker_replaces_the_one_the_waiter_parked` and `two_waiters_each_park_their_own_entry`: one
waiter parks one entry however many wakers the executor hands it, and two waiters park two entries
even when they share a waker.

The alternative the reviews suggested — a waker slot on the waiter itself — was rejected for the
reason the original comment gives: `wake()` drains the table without knowing its waiters, and moving
the slot would require a back-reference and a `Drop` impl to keep it from outliving them. Keying the
shared table costs one `usize`.

## What This Does Not Decide

Nothing about *when* to fire a wake. Which browser events count is the application's, which is what
entry 16 settled and what `examples/todo-app/src/platform/mod.rs` implements; decision 041 records the
policy the trial adopted.

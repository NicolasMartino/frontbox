# The Invalidation Poll Stops When Nobody Is Looking; The Drain Does Not

Document Class: Decision
Status: Accepted 2026-09-01; built the same day
Date: 2026-09-01
Category: Trial Application Policy
Scope: What a browser window does with its two background loops when the user is working somewhere else, and which browser signal decides it.
Sources: `examples/todo-app/src/session.rs`, `examples/todo-app/src/platform/mod.rs`, `examples/todo-app/e2e/ui.mjs`
Related: `wiki/decisions/040-one-wake-releases-every-waiter.decision.md`, `wiki/decisions/038-invalidation-delivery-in-the-trial.decision.md`, `wiki/references/open-decisions.reference.md` (entry 16), `wiki/proposals/browser-background-services.proposal.md`

## Decision

**The invalidation poll pauses outright when this window is not in front, and resumes on the wake.**
Not a longer interval — no timer runs at all. `Sleeper::until_woken` is the wait, and
`platform::app_wake` ends it.

**The drain loop is not paused.** It keeps its cadence whether or not anybody is looking.

"In front" is `!document.hidden && document.hasFocus()`. Both halves are load-bearing and they
answer different questions.

## Why The Two Loops Are Treated Differently

They are not the same kind of work, and reading them both as "background activity" is what would
make this look inconsistent.

**The poll is a read on the user's behalf.** Its entire purpose is that what is on screen matches
what the server holds. Nobody is looking at the screen, so there is nothing for it to be right about;
spending a request every few seconds to keep an invisible view fresh is the cost this exists to
avoid. This is `refetchOnWindowFocus` reasoning and it is not novel.

**The drain is a write the user already made.** It is not on the user's behalf, it is *theirs* —
already enqueued, already durable, and owed to a server. Pausing it would mean a todo typed a second
before switching windows sits in the outbox until the user comes back, which is a client that loses
the offline-first behaviour this whole trial exists to demonstrate at exactly the moment it is most
visible. The saving is also small: an idle drain finds an empty queue and issues no request.

The asymmetry is stated here because it is the obvious next thing a reader would "fix".

## Why `focus` As Well As `visibilitychange`

`visibilitychange` alone was the shape before this, and it does not fire for the case that was
reported.

**Two browser windows side by side are both `document.visible`.** Clicking from one to the other
fires `focus` on the window gaining it and no `visibilitychange` at all. A client listening only for
visibility therefore never learns that the user has come back to it. That is the whole of "when I
gain focus I should refresh the cache, and it doesn't".

`visibilitychange` still earns its place: it fires when a tab is backgrounded behind another tab in
the same window, where `focus` does not. Neither is sufficient, so the gate reads both properties
and both events fire the same wake.

## The Cost, Named

**Opening devtools blurs the page, so the poll pauses while they are open.** This is the same trade
`refetchOnWindowFocus` makes, and it is worth writing down because it will look like the defect this
decision fixed: a developer with devtools open watching for polling will see none.

The mitigation is not to weaken the gate but to say so on screen — `StatusBar` renders "polling
paused — this window is not in front" whenever the loop is parked, which is decision 042's rule
applied to exactly this condition.

## Why A Pause And Not A Long Interval

The previous shape was `HIDDEN_TICK_MS = 600_000` — ten minutes — sampled *before* the sleep. It is
deleted.

**A schedule was standing in for a policy.** There is no interval that is correct for "nobody is
looking"; there is a decision to stop and a decision to resume. Ten minutes is a number chosen to
approximate stopping, and it inherits every property of a real wait: it is committed to at the
moment it starts, and only the wake can shorten it.

That mattered, because the wake was not arriving — decision 040. The two defects composed into the
reported symptom exactly: the window committed to a ten-minute wait the moment it lost focus, and
the event that should have cut it short was being consumed by the drain loop. A window that "stops
polling completely" is what that looks like from outside.

With `until_woken` there is no interval to be wrong about. The loop is either running or parked, and
`continue` after the wake re-checks the condition rather than assuming the wake meant focus — the
same wake is also fired by the offline switch and by a bfcache restore.

## The Tick Below The Budget

Separately, `TICK_MS` moves from 15 000 to 5 000.

It had been set equal to `BROWSER_OBSERVABLE_STALENESS_MS`, under a comment saying it "needs to be no
larger than the tightest" budget. Equal is the boundary case, not a safe one: a source that comes due
just after a tick waits a whole further tick, so equal values put the worst case at *two* budgets —
thirty seconds before a second window even asks whether anything moved. One measured write took
11.3 s to cross, which is inside that band and looks like a defect from the outside.

It costs nothing. A tick only *checks* due-ness; a tick that finds no source due issues no request,
so this sets the granularity of the check and not the request rate.

## How It Is Tested, And What Is Synthesised

**Chromium under an automation protocol will not report a page as unfocused.** `document.hasFocus()`
answers `true` for every page in the context — headed as well as headless — and `bringToFront` fires
no `visibilitychange`. Measured before the test was written, because the alternative is a test that
silently asserts nothing.

So `examples/todo-app/e2e/ui.mjs` overrides `document.hidden`, `document.visibilityState` and
`document.hasFocus` in an init script and dispatches the real events. That is the posture the bfcache
wake was verified under too, and the reason it is honest is that `session::focused()` reads exactly
those two properties: the browser's report is synthesised and every line downstream of it — the wake,
the pause, the resume, the status line — is the shipping path.

The assertions are that no tick occurs across three tick-lengths while unfocused, and that one occurs
within ~1 s of focus returning. The second is only possible if this loop received the wake, so it
covers decision 040 in the application as well.

## What This Does Not Decide

~~**Mobile lifecycle gating is still named and not built.** An iOS or Android application that the OS
backgrounds is a different mechanism from a blurred window, and D4c already recorded that it is
unexplored. `focused()` is `true` on every native target, which is a statement that this trial has
not tested the question rather than an answer to it.~~

**Built 2026-09-01**, the same day, as
`wiki/decisions/043-in-front-on-every-platform.decision.md`. The mechanism turned out to be one
`use_wry_event_handler` for all three native targets, and it is verified against real OS
transitions on iOS and Android. `focused()` no longer has a native arm at all — it has no `cfg`.

**Desktop is the part that stayed open**, and not for the reason this section guessed: the event
exists and `dioxus-desktop` filters it out before a handler sees it. 043 records the measurements.

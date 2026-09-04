# The Application Knows When It Is In Front, On Every Platform It Can

Document Class: Decision
Status: Accepted 2026-09-01; built and measured on iOS and Android the same day. Re-verified 2026-09-02 against rebuilt containers — see `## Re-Verification, 2026-09-02`. **Desktop is wired and does not work** — see `## Desktop, And Why It Is Not Built`
Date: 2026-09-01
Category: Adapter Runtime
Scope: How a native build learns that the user has come back to it, and where that observation lives.
Sources: `crates/frontbox-dioxus/src/native.rs`, `crates/frontbox-dioxus/src/native/tests.rs`, `crates/frontbox-dioxus/src/lib.rs`, `examples/todo-app/src/platform/foreground.rs`, `examples/todo-app/src/session.rs`, `examples/todo-app/src/main.rs`, `justfile`
Related: `wiki/decisions/041-the-poll-stops-when-nobody-is-looking.decision.md`, `wiki/decisions/040-one-wake-releases-every-waiter.decision.md`, `wiki/decisions/038-invalidation-delivery-in-the-trial.decision.md`, `wiki/compatibility/dioxus-adapter.compat.md`, `wiki/plans/d4c-multi-platform-trial.plan.md`

## Decision

**`frontbox-dioxus` gains a `native` feature and `use_lifecycle_wake`**, the native counterpart of
`use_bfcache_wake`. It returns a `Lifecycle`: a `Wake` that fires when the application comes back to
the front, and `is_foreground()` for a loop that wants to ask rather than wait.

Decision 041 said mobile lifecycle gating was "named and not built" and that `focused()` returning
`true` on every native target was a statement that the question was untested. It is tested now, and
this is the answer for two of the three targets.

## Why The Adapter, Having Argued The Other Way

`examples/todo-app/src/platform/mod.rs` carried the argument for keeping this out of the adapter: a
bfcache restore is a **correctness** event — the page's timers did not fire, so a pending wait is
measuring an interval that no longer means anything — while a hidden tab is a **policy** judgement
about spending requests, and policy belongs to an application.

**An operating system suspending an application is the correctness event, by that argument's own
terms.** The process stops being scheduled; its timers do not fire; the wall clock moves on without
it. That is the frozen page, on two of this project's four targets. The line was drawn in the wrong
place rather than drawn where there was nothing.

The cost is real and worth naming: `dioxus-desktop` brings wry, tao and a webview into a crate whose
own manifest said it "needs neither RSX nor a renderer". It is optional and default-off, and
`scripts/verify.sh` runs two gates with **no** features as the proof a browser build never links it —
the same pairing that already protects `web`.

Named `native`, not `desktop`, because `dioxus` aliases `mobile` to `dioxus_desktop`; one feature
serves macOS, iOS and Android, and a feature an iPhone build must switch on should not be called
`desktop`.

## One Handler, Two Signals, Three Platforms

`use_wry_event_handler` hands over the raw `tao::event::Event`, and `App::tick` runs it first for
every event, so nothing is pre-filtered except window events (which matters below).

| Target | The signal that means in front | Emitted by |
| --- | --- | --- |
| macOS, Windows, Linux | `WindowEvent::Focused(bool)` | `tao/macos/window_delegate.rs:379,411` |
| iOS | `Event::Resumed` / `Suspended` | `applicationDidBecomeActive` / `applicationWillResignActive` |
| Android | both | `onResume`/`onPause` and `onWindowFocusChanged` |

Observing both and letting the most recent win is the only arrangement correct on all three without
asking the caller which platform it is on.

**iOS's `Focused` is not app foreground and is not read as it.** It comes from
`becomeKeyWindow`/`resignKeyWindow`, so in a single-window application it fires about once. Matching
it anyway is harmless: if a system alert steals key window, `applicationDidBecomeActive` restores the
flag when the alert goes.

The flag starts `true`. **Assuming the front is the safe default** for the same reason the browser
half reads `has_focus().unwrap_or(true)`: wrongly believing nobody is looking is a client that
silently stops updating, and wrongly believing somebody is costs one request.

## Desktop, And Why It Is Not Built

The desktop arm is written, tao emits the event, and **it never arrives.**

`dioxus-desktop`'s `apply_event` drops every `Event::WindowEvent` whose `window_id` differs from the
one the handler registered against, and in a `dx serve` desktop build the two do not match.
Measured by dumping the raw event stream: **350 events across two deliberate window resizes and
three focus changes, of which `MainEventsCleared`, `RedrawEventsCleared`, `NewEvents` and
`UserEvent` were all of them and `WindowEvent` was none.** The resizes are what make it the filter
rather than an absence of focus changes.

Asking the window instead does not rescue it. `use_window().is_focused()` reports **`false` while
the window is frontmost and focused** — the same mismatch seen from the other side. A gate built on
it paused a healthy loop 13 seconds after launch, before anything had been blurred. That version was
built, measured, and deleted: **a wrong answer is worse than no answer here**, because the failure
mode is a client that stops updating and says nothing.

So a desktop build observes the same events and never sees one that changes the flag, leaving it
permanently in the foreground — the behaviour it had before this module existed. Nothing regresses;
the feature is simply inert there.

The route that would work is `Config::with_custom_event_handler`, which `dioxus-desktop` calls with
the **unfiltered** event. It is launch-time configuration rather than a hook, so it belongs to an
application rather than here, and it is not worth threading through while register entry 20 stops
the desktop executor within seconds regardless of what wakes it.

### A Served Desktop Window Floats, And That Alone Blocks The Test

`dioxus-desktop` builds its window with `always_on_top().unwrap_or(true)` (`config.rs`), so a
`dx serve` desktop window sits above everything by default. That is convenient while editing and
fatal here: **the one thing this arm exists to exercise is the window losing focus**, and a window
that cannot be covered is a window whose blur is awkward to produce on purpose.

`just examples native` therefore passes `--always-on-top false`. It changes no application code —
the flag reaches the app as `DIOXUS_ALWAYS_ON_TOP`, which is where `dioxus-desktop` reads it — and
it is set in the recipe rather than in `main()` so that `launch(App)` stays free of a desktop-only
branch, which is the claim this crate's header makes about itself.

It does not rescue the arm. The window event still never arrives, for the reason above.

## What The Platform Does Not Offer

**True background/foreground is unavailable on iOS through this stack.** tao registers
`applicationWillEnterForeground:` and `applicationDidEnterBackground:` and leaves both bodies empty,
so active/inactive is the whole of what reaches us. That also fires for transient interruptions — the
control centre, an incoming call, the app switcher — which for a client deciding whether to spend
requests is arguably the better line. It is recorded because it is a limit inherited, not chosen.

## The Shape It Forced On The Application

`platform::app_wake()` now returns `(Wake, Foreground)`, and `Foreground` is what
`session::focused` asks. That function **lost its `cfg` split entirely** — it had two bodies, a
browser one reading `document.hidden`/`hasFocus` and a native one hard-coded `true`. Everything a
platform decides now lives in `examples/todo-app/src/platform/mod.rs`, which is what that
module's header always claimed.

A third arm is load-bearing rather than defensive: `cargo test --workspace` and a plain
`cargo build -p todo-app` compile the app with the default `web` feature **on a native host**, so
`not(target_arch = "wasm32")` holds while neither renderer feature does. Without it the workspace
stops compiling, and the failure would read as a mistake in the mobile arm rather than a missing
case.

## The Fallback The File Stated Three Times And Implemented Once, 2026-09-02

Every arm of this design has to answer one question that is not about any platform: **what does a
build say when it cannot find out whether anybody is looking?** The answer, everywhere, is *yes, and
keep polling* — because the failure mode of guessing "nobody is looking" is a window that silently
stops updating and never says why, which is exactly the symptom decision 041 makes possible by
letting a loop stop at all.

`examples/todo-app/src/platform/foreground.rs` said so three times: on the `app_wake` fallback for a
missing window ("the loop still runs on its budget"), on the `has_focus()` call inside the web arm
(`unwrap_or(true)`, with the reasoning spelled out beside it), and on the no-renderer arm ("yes, and
keep polling"). It implemented the opposite once. The web arm's outer half was `is_some_and`, which
returns **false** when there is no window or no document — a worker realm, a page being torn down —
so the one case that comment was about defaulted the wrong way.

It is now `is_none_or`, and both halves of that expression default to `true` for one stated reason.

Found by review rather than by running anything, and running something would not have found it:
`web_sys::window()` is `Some` on the main thread of every build this trial ships, so the branch is
unreachable today. It is recorded rather than fixed silently because the whole value of a rule
stated three times is that the fourth reader can rely on it.

## Re-Verification, 2026-09-02

Re-run after `just examples down && just examples all` rebuilt both services and the web bundle from
scratch, so nothing measured here rests on a container that predates the feature.

- **Web** — `just examples e2e`, 41 assertions passed, 0 failed, no console errors. Section 11
  covers the gate: `paused is said out loud`, `no polling while unfocused`, `polling resumes on
  focus`.
- **Android** — HOME at 07:17:39, `ui invalidation paused (nobody is looking)` at 07:17:40.6, **no
  tick for the 38 s backgrounded**, relaunch at 07:18:19 answered by a tick at 07:18:18.7.
- **iOS** — Safari launched over it, **41 s with no tick**, then the paused line and a tick 0.42 s
  behind it.
- **Desktop** — 2 ticks, then nothing across a deliberate defocus and refocus, and no paused line
  ever. Entry 20 again; the gate is not reachable behind it.

### The Pause Line Arrives On Resume On iOS, Not On Leaving

Android logs `paused` when the user leaves, because the process is still scheduled long enough to
run the check. **iOS logs it on the way back in.** The OS freezes the process at
`applicationWillResignActive`, so the loop's next check does not run until the app is resumed, and
the ordering that reaches the log is `paused` and then a tick 0.42 s later. The observable claim —
no requests spent while away, a tick immediately on return — is identical; the sequence is not, and
reading the iOS log expecting Android's order is confusing without this written down.

### The Server Log Is The Wrong Instrument

The first Android measurement of the day read as **"the gate does nothing"**: requests every 5 s at
both services, straight through a 40-second background. It was wrong, and the way it was wrong is
worth keeping. Three clients were polling those two services at once — a stale iOS build left
running from the previous day, a desktop process from the same, and a browser tab — and
`GET /api/v1/todos -> 200 OK` names none of them. Force-stopping the Android app changed nothing in
the log, which is what exposed it.

**A per-application log is the only sound instrument for this claim**, because it is the only one
that says which client it is: `adb logcat -s RustStdoutStderr:V` on Android, `dx serve`'s captured
stdout on iOS. Both name the process, and both showed the pause the server log could not.

One trap inside that: `dx serve` may exit once it has launched a desktop build, and the app's stdout
dies with the pipe. A silent log then looks exactly like a stalled loop. Running the built binary
directly — `target/dx/todo-app/debug/macos/TodoApp.app/Contents/MacOS/todo-app` — is what separated
the two, and it is what confirms the stall is real rather than a lost pipe.

## Verification

Real OS transitions, not synthesised ones — unlike the browser, where Playwright cannot report a
page as unfocused and `e2e/ui.mjs` has to stub `document.hidden`.

- **Android** — `adb shell input keyevent KEYCODE_HOME`, then relaunch. Observed: ticks, then
  `ui invalidation paused (nobody is looking)`, then a tick **immediately** on return. A prompt
  resume is only possible if the loop received the wake, so it exercises decision 040 on native too.
- **iOS** — `xcrun simctl launch booted com.apple.mobilesafari` to background it, then relaunch
  `com.example.TodoApp`. Same three observations.
- **Desktop** — the negative results above, twice, with the handler registered and with it not
  registered.

## What Is Tested, And The Half That Cannot Be

Added 2026-09-02, in answer to a review finding: this module had **no test of any kind**. Everything
above rested on device runs and on tao's documentation, which is real evidence for the wiring and no
evidence at all for the decisions the code makes about what it receives.

Two things were pulled out of the handler closure so they could be asked directly. `in_front` is the
event-to-answer mapping, generic over the user-event type — the closure could not name its own
argument, because the event carries `dioxus_desktop::ipc::UserWindowEvent` from a private module,
and that is precisely what made it untestable. `Lifecycle::observe` is the transition rule. Both are
now covered by `crates/frontbox-dioxus/src/native/tests.rs`:

- `Resumed` and `Suspended` map to in-front and not, which is the whole of the iOS answer.
- Every other event says **nothing** rather than "not in front". A `false` for an unrelated event
  parks a healthy loop, which is the failure this module refuses to risk.
- A new `Lifecycle` starts in the foreground.
- The wake fires on a transition **to** the front, and not on leaving.
- A repeated signal is not a second wake. This is not hypothetical: Android emits `onResume` *and*
  `onWindowFocusChanged`, so on that platform every resume arrives twice, and waking per event
  rather than per transition would release a wait the second event had not invalidated.

**The `WindowEvent::Focused` arm cannot be tested, and the reason is upstream rather than an
omission.** `Event::WindowEvent` is `#[non_exhaustive]`, so no crate but tao may construct one — the
arm can be matched and not built. The desktop half of the table above therefore still rests on tao's
documentation and on the raw-stream dump recorded in `## Desktop, And Why It Is Not Built`, which is
the evidence it always had, now stated rather than implied. It compounds with the finding there:
`dioxus-desktop` filters those events out before a handler sees them, so the arm is unexercised at
runtime as well as untestable at compile time. Nothing about that is worth working around — a
constructed `Focused` would prove the `match` arm and not the delivery, and the delivery is the part
that is broken.

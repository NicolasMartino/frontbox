# D4c: The Same Application On Four Platforms

Document Class: Plan
Status: Built 2026-08-30
Date: 2026-08-30
Category: Extraction
Scope: What it cost to run the D4a/D4b trial application unchanged on web, macOS desktop, iOS and Android; what the platform seam turned out to contain; and the three findings, one of which is a platform defect this project cannot fix.
Sources: `examples/todo-app/src/platform/mod.rs`, `examples/todo-app/src/main.rs`, `examples/todo-app/Cargo.toml`, `examples/todo-core/src/backend.rs`, `crates/frontbox-dioxus/src/sync/`
Related: `wiki/plans/d4a-offline-todo-trial.plan.md`, `wiki/roadmaps/extraction.roadmap.md`, `wiki/decisions/001-single-threaded-core.decision.md`, `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/decisions/032-opaque-row-store.decision.md`, `wiki/references/open-decisions.reference.md`

## What This Was

The D4a trial asked whether frontbox's API expresses a real flow without churn. D4b asked whether
swapping the durable backend touches only the place the backend is named. **This asks the version
of that question the library has never been asked: whether the same application runs on a platform
nobody built it for.**

It is a cheap question to ask and an expensive one to skip. `frontbox-dioxus`'s hooks had exactly
one consumer, on one platform, and every previous time a second consumer appeared in this project it
produced a finding. That record held.

## What Was Built

One UI crate — `examples/todo-app` — now targets **web, macOS desktop, iOS and Android**, and
**nothing below `main.rs` is conditionally compiled.** `rows.rs`, `sync.rs`, every component and
every event handler are byte-identical across all four.

Everything a platform decides is in `examples/todo-app/src/platform/mod.rs`, and the list is four items:

| What | Web | Desktop | iOS | Android |
| --- | --- | --- | --- | --- |
| Clock | `WebClock` (`Date.now()`) | `SystemClock` | `SystemClock` | `SystemClock` |
| Timer | `gloo-timers` | `tokio::time` | `tokio::time` | `tokio::time` |
| Storage | a database *name* | `dirs::data_dir()` | `dirs::data_dir()` (the sandbox) | JNI `getFilesDir()` |
| Server address | `127.0.0.1` | `127.0.0.1` | `127.0.0.1` | **`10.0.2.2`** |

D4d added a **second** server address and changed nothing about that row: `USER_BASE_URL` sits
beside `BASE_URL` in the same module and takes the same `10.0.2.2` substitution, because "which
loopback reaches the host" is a platform decision and a second service must not get a second answer
to it. The list is still four items; one of them now has two values.

**Both mobile targets have compiled under `scripts/verify.sh` since 2026-09-01**, opt-in on the
toolchain being present. This plan's own claim that they were "not gated at all" held for two
deliverables, through a UI rewrite that touched most of `examples/todo-app` — see `wiki/log.md`.

The first two arrive through seams that already existed and were not touched: `Clock` in core
(decision 002) and `Sleeper` in the adapter. **Neither needed a change**, which is the single
strongest result here — `Sleeper` was drawn for "gloo on web, tokio on desktop" and turned out to
cover two platforms it was not written for.

## Finding 1 — The Storage Seam Held, And Android Was The One Real Cost

`todo-core`'s backend module already chose SQLite or IndexedDB by target, so **all three native
platforms got durable storage with no new code at all**. What they did not get is an answer to
*where*, and that is genuinely per-platform:

- **Desktop and iOS** are one line of `dirs`. On iOS `data_dir()` lands inside the application
  sandbox, which is exactly right, and it works on the simulator and a device alike.
- **Android cannot be answered from the environment.** There is no `HOME`, `TMPDIR` points at
  nothing writable, and `dirs::data_dir()` therefore returns `None`. The per-application directory
  is a property of the running `Context`, reachable only by calling `getFilesDir()` on the
  `Activity` over JNI. A build that reused the desktop path would not fail to compile — it would
  fail at the first write, on a device.

That is ~25 lines in one function, using `ndk-context` to reach the activity wry already installed.
**It is application code, not library code**, and it stayed that way: nothing about it reached
`frontbox-sqlite`, whose `open` takes an `AsRef<Path>` and does not care who chose the path.

`10.0.2.2` is the same shape of finding one layer up. An Android emulator's `127.0.0.1` is the
emulated device, not the host, so the demo would have started permanently offline — and *looked*
fine, because starting offline is a state this application renders deliberately.

## Finding 2 — Mobile Needed Four CSS Changes, And Only One Was Cosmetic

Recorded because "make it mobile friendly" sounds like styling and three quarters of it was not:

- **`font-size: 16px` on every input.** Below that, the iOS webview zooms the page on focus and does
  not zoom back. It presents as the layout breaking on tap.
- **`env(safe-area-inset-*)`.** Without it the title sits under the status bar and the dynamic
  island.
- **44px touch targets under `@media (pointer: coarse)`.** Keyed off the pointer type rather than
  the viewport width, because a touchscreen laptop is wide and still being tapped.
- **A single column below 30rem**, so the composer stacks instead of squeezing its field to nothing.

Both mobile screenshots confirm all four. Neither platform needed a component change.

## Finding 3 — The Drain Loop Dies On Dioxus Desktop

**The serious one, and it is not frontbox's bug.** `use_sync_loop` runs the drain inside
`use_future`, on the VirtualDom's task executor. On Dioxus desktop that executor stops being polled
after a few seconds, and **the queue never drains again** — silently, with the UI still responsive
and writes still landing in SQLite.

Measured in one process over 95 seconds at a five-second cadence:

| Task | Where it runs | Ticks | Expected |
| --- | --- | --- | --- |
| The sync loop | Dioxus `use_future` | **3** | ~19 |
| A `use_future` touching no signal | Dioxus `use_future` | **2** | ~19 |
| A bare `tokio::spawn` loop | The tokio runtime | **18** | ~19 |

Row two rules out the loop and rendering; row three rules out the timer, the runtime and OS
throttling. Reproduced under `cargo run` and `dx serve` alike, window occluded and frontmost.

**Web, iOS and Android are unaffected**, and both mobile platforms were tested against the idle
window specifically, because "it worked when I poked it" is exactly the observation the desktop
build would also have produced:

- **iOS.** A mutation written into the running application's SQLite file from outside, after 120
  seconds of no interaction. Drained in ~12 seconds.
- **Android.** Sharper, because the device could be left alone entirely: a todo queued through the
  UI **with the server stopped**, then **170 seconds untouched**, then the server restarted from the
  host. It drained ~10 seconds later with **no device input at all** — so the loop had gone on
  retrying through the whole idle window, which is precisely what the desktop build stops doing.

The full analysis, the likely mechanism in `dioxus-desktop`, and the three options are register
entry 20. One consequence belongs here: **the obvious workaround is closed by decision 001.** The
tokio task in row three could carry the drain, and cannot, because `tokio::spawn` requires `Send`
and every store in this project is `!Send` on purpose. The posture chosen so an IndexedDB future
could exist at all is what removes the desktop escape hatch. That is a cost worth recording, not an
argument — the alternative was never available.

## How It Was Verified

Not by compiling. Every platform was run, and the evidence is storage the application wrote:

- **Desktop.** `~/Library/Application Support/frontbox-todo/todo.sqlite3`, five tables, both server
  rows merged. A mutation injected into the file from another process drained and reached the
  server — which also incidentally proves cross-process WAL visibility, checked separately.
- **iOS 26.1 simulator.** `…/Data/Application/…/Library/Application Support/frontbox-todo/`, five
  tables, four rows. A mutation injected after 120 seconds of idle drained in ~12 seconds.
- **Android 34 emulator, arm64.** `/data/data/com.example.TodoApp/files/frontbox-todo/`, five
  tables, six rows over `10.0.2.2`. A todo typed into the emulator's own UI reached the server, and
  the offline path was exercised end to end from the device: ticking *offline* queued a write with
  the green "saving…" marker and `pending 1 … last drain: Offline`, and unticking it drained inside
  six seconds. That screenshot is the whole library working on a phone in one frame.
- **Web.** Unchanged and still green through `build ui wasm` and the running container.

`rusqlite` with `bundled` cross-compiled to `aarch64-linux-android` and `aarch64-apple-ios-sim`
without any configuration beyond `ANDROID_NDK_HOME`, which was the toolchain risk this exercise was
supposed to be about and turned out not to be.

## What This Does Not Claim

- **No physical device was used.** Both mobile runs were emulators. Nothing here has been signed,
  submitted, or run on hardware, and background suspension — the thing a real phone does that an
  emulator does not — is untested. See register entry 20's neighbours: whether a backgrounded
  application deserves the bfcache treatment `Wake` gives a restored tab is an open question, not a
  built feature.
- **No adapter change was made.** Finding 3 argues for one and does not take it; that needs the
  go-ahead `AGENTS.md` requires, and the register is where the evidence now sits.
- **`dx` warns that its 0.7.6 disagrees with `dioxus` 0.7.10** on every build. Everything worked;
  it is recorded because a version skew is the first thing to rule out if any of this stops
  reproducing.

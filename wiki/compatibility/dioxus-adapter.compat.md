# The Dioxus Adapter Puts A `0.x` Crate In A Public Surface

Document Class: Compatibility Note
Status: Active
Date: 2026-08-29 (the `web` feature and the closure-taking hook, 2026-08-30)
Category: Public API Shape
Scope: What `frontbox-dioxus` commits to, why its compatibility story is weaker than core's on purpose, and what that does and does not mean for anyone depending on core.
Sources: `crates/frontbox-dioxus/Cargo.toml`, `crates/frontbox-dioxus/src/`, `Cargo.toml`
Related: `wiki/compatibility/public-dependencies.compat.md`, `wiki/decisions/028-drain-loop-boundary.decision.md`, `wiki/decisions/001-single-threaded-core.decision.md`, `wiki/roadmaps/extraction.roadmap.md`

## The Claim This Page Qualifies

`wiki/index.md` records that **no `0.x` crate is in the semver contract**, which is what makes
frontbox's remaining release obligations small. That statement is about core and it is still true of
core, verified by `scripts/verify.sh` rather than by reading a manifest: the resolved graph of
`-p frontbox` is `serde`, `serde_json`, `thiserror` and `uuid`, all major `1`.

It stops being true one crate over. `frontbox-dioxus` re-exports `dioxus_signals::Signal` in three
public fields and takes Dioxus hooks throughout, so **Dioxus 0.7 is unavoidably in its public
surface**, and by Cargo's pre-1.0 rules the *minor* position is where its breaking changes live.

That is not a defect to be engineered around. An adapter for a framework has that framework in its
API by definition; an adapter that hid it would be an adapter you could not hand a `Signal` to.

## What It Depends On, And Why Not The Facade

| Crate | Version | Why |
| --- | --- | --- |
| `frontbox` | path | The runtime it adapts |
| `dioxus-core` | `0.7` | Task spawning |
| `dioxus-hooks` | `0.7` | `use_context`, `use_signal`, `use_future` |
| `dioxus-signals` | `0.7` | `Signal`, `UnsyncStorage`, `WritableExt` |
| `js-sys` | `0.3`, optional | `Date.now()` for `WebClock` — `web` feature only |
| `wasm-bindgen` | `0.2`, optional | The `pageshow` closure — `web` feature only |
| `web-sys` | `0.3`, optional | `Window`, `EventTarget`, `PageTransitionEvent` — `web` feature only |

The first three are what the `dioxus` facade re-exports, named individually rather than through it.
This is a hook library, not a component library: it emits no RSX and needs no renderer *in its base
set*, and the facade would pull both. Cargo unifies these with whatever `dioxus` the application
already resolves.

**"Needs no renderer" acquired an exception on 2026-09-01**, and it is worth stating rather than
quietly widening: the `native` feature below depends on `dioxus-desktop`, which brings wry, tao and
a webview. Nothing about the base set changed — the exception is one optional, default-off feature —
but the sentence above was written as an unqualified claim and is no longer one.

**The last three are the `web` feature, and it is default off** (added 2026-08-30, register entries
15 and 16). Dioxus desktop is a supported target of the framework this crate is named after, and a
desktop build must not carry JavaScript glue it can never call. `scripts/verify.sh` gates it from
both sides: `clippy adapter` and `build adapter wasm` run with the feature *off*, which is what
proves the three stay optional, and two further gates compile it for wasm so a feature nothing
builds cannot rot.

The feature adds `WebClock` and `use_bfcache_wake` to the public surface. `Wake` and
`Sleeper::wakeable` are public *without* it, so an application on a platform this crate does not
cover can drive the same seam from its own listener.

## The `native` Feature (2026-09-01)

`dioxus-desktop = { version = "0.7", optional = true }`, behind `native`, default off. It adds
`use_lifecycle_wake` and `Lifecycle` — the focus and OS-suspend events that say whether the
application is in front, which arrive on tao's event stream and nowhere else
(`wiki/decisions/043-in-front-on-every-platform.decision.md`).

**Named `native` rather than `desktop` on purpose.** `dioxus` aliases `mobile` to `dioxus_desktop`,
so one feature serves macOS, iOS and Android, and a feature an iPhone build has to switch on should
not be called `desktop`.

Gated from both sides exactly as `web` is: the feature-*off* pair proves a browser build never links
a webview, and `clippy adapter native` / `build adapter native` prove the feature still compiles.

**This is the crate's first 0.x dependency with a renderer in it**, which raises the semver exposure
the section above describes: a `dioxus-desktop` breaking change now reaches this crate's public
surface through `Lifecycle`. It is confined to one optional feature, and `Wake` and
`Sleeper::wakeable` remain usable without it.

## `use_sync_loop` Takes Closures (2026-08-30)

Changed from `use_sync_loop(handle, cadence, sleeper)` to
`use_sync_loop(sync, counts, cadence, sleeper)`, where `SyncStep` and `CountsStep` are the two
capabilities the loop actually needs. **This is a breaking change to a `0.0.0` crate with
`publish = false`, so it costs nothing today and would have cost a release later** — which is the
whole argument for taking register entries 14, 15 and 16 in one touch rather than three.

`use_sync_loop_on(handle, cadence, sleeper)` kept the old behaviour under a new name, for an
application that lets Dioxus own its runner.

**Both it and `FrontboxHandle` were removed on 2026-09-01**, along with `use_frontbox`,
`use_frontbox_context` and the two `from_handle` constructors. The class of application they were
kept for never acquired a member, and entry 14 already recorded that their one obvious use —
`use_frontbox(|| SyncRunner::new(app.outbox().clone(), ..))` — type-checks and is wrong: two loops
over one queue, each believing itself alone. A convenience nobody uses, whose natural use is a
trap, is not worth a `0.x` surface.

The reason is in `wiki/references/open-decisions.reference.md`, entry 14: the hook called `drain` on
the runner inside the handle, and the only application in this repository could give it neither the
handle nor a `drain` that meant what the application means by syncing. `examples/todo-app` now uses
the hook, which is the evidence the change worked — a hook that merely compiles proves only that its
signature is well-formed.

## What Survives From Core, And What Does Not

**The `!Send` guarantee survives, and it is not incidental.** Decision 001 keeps every `Send` bound
out of core because an IndexedDB future is `!Send` and cannot be wrapped into one. Dioxus agrees:
`spawn` takes a plain `Future<Output = ()> + 'static`, and `use_signal` returns
`Signal<T, UnsyncStorage>`. `scripts/verify.sh`'s `Send`-bound check now greps `crates/*/src/`
alongside `src/`, so the adapter cannot quietly reintroduce one.

`Sleeper` boxes an unsent future — `Pin<Box<dyn Future<Output = ()>>>` — for the same reason.

**The conformance guarantee does not survive, and the plan should not pretend otherwise.** Cases
57-60 prove the drain loop against every backend through `StoreFactory`. Nothing here can be reached
that way: there is no headless Dioxus runtime in this repository. The adapter's gate is that it
compiles for native and `wasm32-unknown-unknown` under `-D warnings`, plus five unit tests over
`Wake` and `Sleeper::wakeable` — a latch and a hand-written race, which are plain Rust and testable
with a counting waker.

**`examples/todo-app` is now the consumer this predicted**, and it is evidence rather than a plan:
it drives the loop through `use_sync_loop` on four platforms, and the `pageshow` wake is exercised
by a browser rather than by a test. It could not use either hook when it was written — the reason
`use_sync_loop` takes closures today is that the trial could not give up a `SyncRunner` it does not
own (register entry 14). The adapter's usability claim is the one thing here an application, and
only an application, could check.

That asymmetry is the reason `SyncRunner::drain` lives in core rather than here
(`wiki/decisions/028-drain-loop-boundary.decision.md`). The rule it suggests for anything added to
this crate later: **if a behaviour can be stated as a property of a store or a runner, it belongs on
the side of the boundary the suite can see.**

## Consequences For Versioning

- **Core and the adapter version independently.** Both are `0.0.0` with `publish = false` today. When
  they are published, a Dioxus minor bump forces a `frontbox-dioxus` release and forces nothing on
  `frontbox`.
- **A consumer of core alone inherits none of this.** Depending on `frontbox` does not put Dioxus in
  the graph, which is exactly what the rewritten `no dioxus` gate asserts on every run.
- **Coverage is gated on `-p frontbox` only.** Averaging one floor across core and a crate the suite
  cannot reach would let a regression in either hide behind the other.

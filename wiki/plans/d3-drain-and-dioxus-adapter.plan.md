# D3: The Drain Loop And The Dioxus Adapter

Document Class: Plan
Status: Completed 2026-08-29
Date: 2026-08-29
Category: Extraction
Scope: The execution plan for D3, split into a framework-neutral drain loop in core and a Dioxus binding crate, with what the build changed about the plan.
Sources: `src/runner/drain.rs`, `src/testing/cases/drain.rs`, `crates/frontbox-dioxus/`, `scripts/verify.sh`, `Cargo.toml`
Related: `wiki/roadmaps/extraction.roadmap.md`, `wiki/decisions/028-drain-loop-boundary.decision.md`, `wiki/decisions/029-drain-termination.decision.md`, `wiki/decisions/030-serializable-reports.decision.md`, `wiki/decisions/018-single-flight-drain-mode.decision.md`, `wiki/decisions/020-observability-surface.decision.md`, `wiki/proposals/offline-todo-trial.proposal.md`

## Why D3 Was Next

Two accepted decisions converged on one unbuilt mechanism from unrelated directions, which is
usually a sign the mechanism is in the right place.

- **Decision 018 owed it for liveness.** At `batch_limit = 1` the poll cadence multiplies the
  backlog: 200 records at the source's five-second interval is roughly seventeen minutes against
  the roughly twenty seconds a back-to-back drain costs. The gap is cadence, not round-trip time,
  and it is a factor of fifty.
- **Decision 020 owed it for observability.** At limit 1 every count in a `SyncReport` is 0 or 1, so
  200 records produce 200 near-empty reports and aggregation moves out of the library into every
  caller. The drain loop is the natural boundary to aggregate at.

Neither had been built, and nothing else in the backlog blocked evidence the way this did: no
application had ever run against this API.

## What Was Built

### The drain loop, in core

`SyncRunner::drain` in `src/runner/drain.rs`, returning a `DrainReport` that folds every pass. No
new dependency, no timer, no executor: the loop waits for nothing, so it needs no clock, and core
keeps its four runtime crates. `src/runner/mod.rs` stays at 310 lines and the split follows the
`record/mod.rs` plus `record/terminal.rs` pattern already established.

The boundary decision — core rather than the adapter — is
`wiki/decisions/028-drain-loop-boundary.decision.md`. The termination rule, which is the part with
teeth, is `wiki/decisions/029-drain-termination.decision.md`.

### Four conformance cases, 55 to 59

| # | Case | What only it proves |
| --- | --- | --- |
| 57 | A drain empties what one pass cannot | Same seed and limit twice: one `sync_once` leaves three of five queued, one `drain` leaves none |
| 58 | A drain stops when a pass drains nothing | A wedged head behind a bound of three, drained, sends **one** request |
| 59 | An offline drain stops at the first pass | One attempt, not a retry storm; and an empty queue costs no request at all |
| 60 | A drain clears a single-flight backlog | Five records at `batch_limit = 1` in one call, in enqueue order |

57 through 59 joined the outbox suite; 60 joined the single-flight profile, where it belongs
because that is the profile whose whole subject is what limit 1 makes observable.

### `Serialize` on the report types

Seven types, one way only (`wiki/decisions/030-serializable-reports.decision.md`). Folded into this
slice because `DrainReport` is new, so a new report type either joined the gap decision 020 found
or closed it.

### The workspace, and the adapter

Root package `frontbox` gained a `[workspace]` section, initially for `crates/frontbox-dioxus` and
later for D4a's three trial crates under `examples/`. `src/` did not move: the wiki cites
`src/<file>.rs:<line>` in hundreds of places.

The adapter provides a context handle, the sync loop's *cadence*, the three store counts as signals,
and an invalidation state. Roughly 400 lines across five files, with Dioxus 0.7 in its public
surface — see `wiki/compatibility/dioxus-adapter.compat.md`.

## What The Build Changed About The Plan

The house pattern, and it earned itself again.

**Decision 018's claim that 017 prevents the spin is wrong, and the plan was written before that was
known.** The plan already proposed stopping on no-progress, on the grounds that repeating a
fruitless pass cannot help. Writing case 58 turned that from a preference into a finding: 017's
bound counts verdicts *received*, so a back-to-back loop consumes a bound of eight inside a second
and terminates a record the server might have resolved on the next poll. The bound is a policy about
time, and the naive loop collapses it into a policy about nothing. Decision 029 carries the
correction; decision 018 is amended.

**The `no dioxus dependency` gate broke, and the fix was an improvement rather than a workaround.**
It ran `grep -ni dioxus Cargo.toml`, which matches a workspace member named `frontbox-dioxus`. A
`members` entry is not a dependency, so the gate would have failed the one arrangement it existed to
permit. It now asks `cargo tree -p frontbox --edges normal --all-features`, which is the argument
`scripts/verify.sh` already makes in its own comment for the date-library check beside it.

**Every gate had to name its package.** A bare `cargo clippy` or `cargo llvm-cov` in a workspace
silently changes which crates it covers. That is the failure mode the conformance macros' own
documentation exists to prevent, arriving through the build system instead of the case list.

**The `docs resolve` gate earned itself for the second time.** Two doc links in `drain.rs` carried
section anchors on associated-function paths, which rustdoc rejects; the gate caught them before
they shipped. It also caught a stale link the day it was added, during decision 027.

**The invalidation stream stayed unbuilt, deliberately.** The roadmap lists "SSE or websocket
invalidation stream adaptation" under D3. What that needs is an HTTP client and a reconnect policy,
both the application's — and the stream *type* would be a dependency choice this crate has no
business making for a caller. The seam is a method instead: an application's own handler calls
`InvalidationState::apply` and the signals update. What is left is glue, not a decision, and D4's
example is what should shape it.

## Verification

`bash scripts/verify.sh` reports **ALL GATES PASSED**:

- fmt, clippy on native and `wasm32-unknown-unknown` with `-D warnings`, both wasm builds, and the
  documentation build with `-D warnings`, all named to `-p frontbox`.
- Clippy and a wasm build for `-p frontbox-dioxus`, which is the whole of its gate.
- 59 conformance cases, plus the unit, oracle, and doctest suites.
- Coverage 89.03% regions / 96.18% lines / 93.63% functions against an 80% floor, on `-p frontbox`.
- `cargo tree -p frontbox --edges normal --all-features` mentions neither Dioxus nor a date crate,
  with `frontbox-dioxus` in the same workspace. That is the real test of the rewritten gate, and it
  is what keeps the core dependency claim honest now that a sibling crate contradicts it.

## What This Unlocks, And What It Does Not

D4 can now be planned against something an application can hold. What D3 does **not** deliver is
durability: storage is still in-memory, so a demo built on this proves the API and not the offline
story. That split is the subject of `wiki/proposals/offline-todo-trial.proposal.md`.

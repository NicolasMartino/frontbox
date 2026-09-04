# The Drain Loop Belongs To Core; Only The Cadence Belongs To The Adapter

Document Class: Decision
Status: Accepted 2026-08-29; implemented 2026-08-29
Date: 2026-08-29
Category: Public API Shape
Scope: Which deliverable owns the loop that repeats a sync pass, and what that leaves for a framework adapter to do.
Sources: `src/runner/drain.rs`, `src/runner/mod.rs`, `crates/frontbox-dioxus/src/sync/mod.rs`, `scripts/verify.sh`, `Cargo.toml`
Related: `wiki/decisions/029-drain-termination.decision.md`, `wiki/decisions/018-single-flight-drain-mode.decision.md`, `wiki/decisions/020-observability-surface.decision.md`, `wiki/decisions/001-single-threaded-core.decision.md`, `wiki/decisions/002-error-model.decision.md`, `wiki/roadmaps/extraction.roadmap.md`, `wiki/proposals/extraction-boundary.proposal.md`

## Decision

`SyncRunner::drain` ships in core. The Dioxus adapter gets the **cadence** — how long to wait before
draining again — and nothing else of the loop.

D3 therefore splits: **D3a** is the core drain loop, framework-neutral and covered by the conformance
suite; **D3b** is `crates/frontbox-dioxus`, which provides context, signals, and the wait.

## Why

Both decisions that asked for this loop filed it under D3, and D3 was written as "the Dioxus
adapter". That was a filing convenience rather than a judgment, and it does not survive being asked
what the loop actually needs.

**It needs nothing a framework supplies.** No timer, because a drain runs its passes back to back
and never waits. No executor, because it is an `async fn` the caller already drives. No signal, no
context, no component. It is a `loop` around `sync_once` and an accumulator, and every line of it
compiles against the same four runtime dependencies core already has.

**Decision 020 named it the aggregation boundary, and an adapter cannot be one.** That page's
argument is that at `batch_limit = 1` a 200-record backlog produces 200 near-empty reports, so
"aggregation moves out of the library and into every caller, and unless they all do it identically
the converged dashboards disagree with each other." Putting `DrainReport` in the Dioxus crate does
not fix that — it fixes it *for Dioxus applications only*, and leaves a native SQLite consumer
writing its own aggregate. The type has to sit where every consumer already is.

**The conformance suite can prove a core loop and cannot prove an adapter one.** Cases 57-60 run
against every backend through `StoreFactory`, including the durable ones D5 will add. Nothing in
`crates/frontbox-dioxus` can be reached that way: there is no headless Dioxus runtime here, and the
crate's whole gate is that it compiles for both targets under `-D warnings`. A loop whose
termination rule is correctness-adjacent — see `wiki/decisions/029-drain-termination.decision.md` —
does not belong on the side of the boundary where the suite cannot see it.

**The termination rule is a claim about the store contract, not about a host.** It holds because
`Disposition::Delete` and `Disposition::DeadLetter` remove a row, so a pass that continues the loop
strictly shortens the queue. That is core's own invariant, stated in core's own documentation. An
adapter asserting it would be an adapter asserting something it does not own.

## What Is Left For The Adapter, And Why It Is Not Nothing

The cadence is a real deliverable and it is the half that genuinely needs a host.

A drain returns as soon as it stops draining, so **how often a caller drains is the only thing left
deciding how hard a client pushes.** That is a policy about a wall clock, and core has no clock by
construction: decision 002 injects time through `Clock`, and decision 011 keeps every date library
out of the runtime graph, checked by `scripts/verify.sh` against `cargo tree` rather than against
the manifest.

`SyncCadence` is where a caller states it, per ending — five seconds after a drained queue, thirty
after a stalled one, and the difference between those two numbers is load-bearing rather than
cosmetic. A stalled queue means the server is not resolving what it was given, and each further
drain burns one attempt against any retention bound. The stalled interval is what spreads those
attempts across time; see `wiki/decisions/029-drain-termination.decision.md`, which is the same
argument one level down.

The sleep itself is injected rather than depended on. Dioxus has no portable sleep and the platform
answers differ — `gloo-timers` on web, `tokio::time` on desktop — so `Sleeper` takes whichever the
application already has. That is the seam `Clock` already draws, applied to the other end of the
same problem.

## The Counter-Argument, And Why It Loses

A loop in core is public surface on a crate that wants a small one, and every method added before
publication is a method that costs a major version to remove.

It loses on size and on inevitability. `drain` is one method and two types, adds no dependency, and
is what every consumer would otherwise write for itself — differently, which is precisely decision
020's complaint. The alternative to shipping it is not a smaller API; it is the same API with the
loop copied into each caller, aggregated inconsistently, and unverified by any suite.

## Consequences

- **The repository is a Cargo workspace**, with the root package `frontbox`, the Dioxus adapter,
  and the D4a trial crates. `src/` does not move into `crates/`: the wiki cites
  `src/<file>.rs:<line>` in hundreds of places and those citations are load-bearing. The full
  `frontbox-core` split remains separate from the adapter boundary.
- **The `no dioxus dependency` gate had to change, and the change is an improvement.** It ran
  `grep -ni dioxus Cargo.toml`, which matches a workspace member named `frontbox-dioxus` — a
  `members` entry is not a dependency, so the gate would have gone red for the one arrangement it
  was written to permit. It now asks the resolved graph:
  `cargo tree -p frontbox --edges normal --all-features | grep -i dioxus`. That is the argument
  `scripts/verify.sh` already makes in its own comment for the date-library check, applied to the
  gate beside it.
- **Every gate now names its package.** A bare `cargo clippy` or `cargo llvm-cov` in a workspace
  silently changes which crates it covers, and a gate that quietly stops checking core is worse
  than no gate. The coverage floor stays scoped to `-p frontbox`, because averaging one number
  across core and an adapter the suite cannot reach would let a regression in either hide behind
  the other.
- **The adapter puts a `0.x` crate in a public surface for the first time.** `wiki/index.md` records
  that no `0.x` crate is in the semver contract; that stays true of core and stops being true of
  `frontbox-dioxus`, which re-exports Dioxus 0.7 types. It needs its own compatibility note.
- **`src/runner/mod.rs`'s module documentation was wrong and is fixed.** It said "the periodic loop
  that calls this is an adapter concern". Half of that is still true — the *periodic* part — and the
  loop itself is no longer.

## Revisit If

An adapter appears whose host wants to own the loop rather than the cadence — a runtime with its own
work-stealing scheduler, say, where draining back to back on one task is the wrong shape. Nothing in
core stops that: `sync_once` is still public and a host that wants its own loop writes one. What
this decision fixes is where the *default* lives, not what is possible.

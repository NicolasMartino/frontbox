# D4a: The Offline Todo Trial, API Half

Document Class: Plan
Status: Completed 2026-08-29
Date: 2026-08-29
Category: Extraction
Scope: The migration trial run against the in-memory backend — what was built, what the ten original observations proved, what the post-review regressions now pin, and the findings the trial produced.
Sources: `examples/todo-core/`, `examples/todo-server/`, `examples/todo-app/`, `scripts/verify.sh`
Related: `wiki/proposals/offline-todo-trial.proposal.md`, `wiki/roadmaps/extraction.roadmap.md`, `wiki/proposals/extraction-boundary.proposal.md`, `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/019-verdict-synthesis.decision.md`, `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/decisions/027-dead-letter-reason.decision.md`, `wiki/decisions/029-drain-termination.decision.md`, `wiki/references/open-decisions.reference.md`

## The Result

**No line of `src/` or `crates/` changed.** The roadmap puts the migration trial before the durable
backends because it "is the only real test of whether the extracted API is right", and the test's
answer is that four writes, an optimistic projection, a dead-letter surface, a retention bound, and
direct dispatch all expressed against the published API with no core change at all.

That is the headline, and it is worth stating as a falsifiable fact rather than a feeling. The
post-review repair kept the same boundary: it changed examples, verification, and documentation,
but still did not change core or the adapter.

Findings came out of it anyway. None requires a core change before D5; all are things a second
consumer would rediscover, and one of them is a gap in the report surface that nothing before this
had a reason to notice.

## What Was Built

| Crate | Lines | What it is |
| --- | --- | --- |
| `examples/todo-server` | ~750 | axum over sqlx/SQLite. Batch sync endpoint, read endpoint, a direct-write endpoint, an idempotency table, and Swagger UI over a generated OpenAPI document |
| `examples/todo-core` | ~1,450 | The application. Outbox wiring, optimistic projection, HTTP transport, direct dispatch, ten observations, and eight post-review regressions |
| `examples/todo-app` | ~575 | The Dioxus web UI over `todo-core`. See `## The UI, And What It Cost To Wire` |

**`todo-server` deliberately does not depend on `frontbox`.** Importing `MutationBatchRequest` would
have been one line and would have made the trial worthless: two ends sharing a type cannot disagree
about it, so the end-to-end run would have proved that serde round-trips. The server's shapes are
hand-written in `examples/todo-server/src/wire.rs` from
`raw/initial/2026-08-25T083750Z/sources/08-offline-sync.spec.md` and decision 010. **The ten
observation tests are therefore the wire-format oracle**: a disagreement of one field name fails all
of them.

Maintenance note, 2026-08-30: `todo-server` now derives an OpenAPI document with `utoipa` and serves
Swagger UI at `/swagger-ui/`, plus the raw document at `/api-docs/openapi.json`. The documented
schemas are still the hand-written server shapes from `examples/todo-server/src/wire.rs`; the
server still does not import `frontbox`.

**`todo-core` deliberately does not depend on Dioxus.** If the application logic needed the adapter,
the adapter would be leaking. Keeping the crate framework-free is what makes that falsifiable, and
it is also what lets the observations run as ordinary tests against a real server rather than as
"start it and look".

## The Observations

The ten original observations run in `examples/todo-core/tests/observations/main.rs`, over real HTTP
against a server spawned in-process on an ephemeral port. Eight post-review regressions live in
`examples/todo-core/tests/regressions/main.rs`, and both share `tests/common/mod.rs`. They are gated by
`scripts/verify.sh` as a **primary** gate, not as an example build, because they are the only
end-to-end evidence the project has.

| # | Claim | What it pins |
| --- | --- | --- |
| 1 | Offline, three writes queue and the UI still shows them | `DrainEnd::Offline` at one pass; the projection renders work no server has seen |
| 2 | One drain empties the queue | `passes == 2` — one that sends, one that finds it empty |
| 2b | The same queue at `batch_limit = 1` is four passes and **three** requests | The confirming pass reads an empty batch and never touches the transport |
| 3 | In-memory storage does not survive a restart | Asserted in the negative. See below |
| 4 | A refusal becomes a readable dead letter | `DeadLetterReason::Rejected { error: Some(_) }`, code and details intact |
| 5 | A resend after a lost verdict is a `Duplicate` | The idempotency key doing the only job it exists for |
| 6 | A wedged record terminates at the bound | `DeadLetterReason::RetentionBound`, and **one** request per drain |
| — | Direct dispatch falls back under one id | Two of three paths: applied, and queued under the same id. The third is `direct_dispatch_refusal_is_terminal` below |
| — | A silent server retains without complaining | Conformance case 33 against a real server rather than a scripted one |
| — | The application runtime is `!Send` | A negative compile-time assertion and compile-fail doctest, in the first application that has had to live with it |

### The regressions

Each one is a defect the review found, pinned so it cannot come back. They are listed as claims
rather than as fixes, because a fix is an event and a claim is a property.

| Claim | What it pins |
| --- | --- |
| A direct write obeys the same offline switch as batch sync | `request_attempts` does not move while offline; the toggle queues instead |
| A row with queued work is ordered through the queue, not raced past it | A toggle behind an unsent `create` never reaches a server that has not heard of the row |
| A terminal direct refusal is visible and unqueued | `Direct::Refused` carries `code` and `details`; the queue stays empty |
| A malformed `done` is refused, not read as `false` | The server has no default for a boolean it was not sent |
| A colliding create is refused, not a replacement | The first title survives; `already_exists` says why |
| Deleting an absent row is applied, not refused | The one method whose replay is idempotent — see Finding 3 |
| An unplaceable envelope is reported, not raised | The `DrainReport` survives and the index still publishes what it could place |
| A capped index scan reports its shortfall | `PendingIndexGap::beyond_scan_cap`, from a scan limit of one |

**Observation 3 is asserted in the negative, and that is the point.** The trial cannot show that a
queue survives a reload, so instead it proves that this one does not: a second application over a
second in-memory backend starts empty. `AGENTS.md` calls this library "a durable local queue of
writes that survives restarts", and *durable* is the adjective D5 supplies. When D5 lands, this test
inverts — same scope, new process, work still queued — and that inversion is D4b's deliverable.
Owing the observation openly beats quietly not testing it.

## Finding 1 — Nothing Names The Mutations That Drained

**Resolved 2026-08-30 by decision 035**, which added `DrainReport::drained` — every record that left
the queue, how it left, and the row it was bound to. `TodoApp::sync` now decrements the index by
exactly what drained, and `refresh_pending` became startup reconciliation instead of the thing that
runs after every drain. The finding is kept in full below, because it is the argument that produced
the decision and the plan is retained as evidence.

**The sharpest finding, and it was not predicted.**

A row rendering "saving…" has to answer *is there a queued mutation for row X*. Nothing answers that
cheaply: `pending_count` and `OutboxCounts` are cardinalities, and `pending_batch` is a queue scan —
fine once per drain, not once per render. So the application keeps a row-to-pending index, which
`wiki/proposals/offline-todo-trial.proposal.md` §6 said it would have to.

What §6 did not anticipate is that **the index cannot be maintained incrementally.** `SyncReport`
and `DrainReport` carry counts and anomalies, and the only mutation ids they name are the ones
something went *wrong* with: `AnomalyKind::UnknownMutation`, `RepeatedVerdict`, `UnknownStatus`.
**No report names an id that drained successfully.** There is no event to decrement an index on.

`TodoStore::set_pending_rows` therefore rebuilds the whole index from `pending_batch` after every
drain. That is affordable — once per drain, not per render — and it is the right shape for a todo
list. It is the wrong shape for a queue of ten thousand records on a phone, which is a real target.

**The review sharpened this into a second, smaller finding: a rebuild can be incomplete, and there
is nowhere to say so.** `pending_batch` takes a limit, so a scan can be capped; an envelope's row id
is recovered by parsing, so a scan can fail to place a record. Both leave an index that is right
about the rows it holds and silent about the ones it does not. The first attempt raised an `Error`
on either — which threw away a `DrainReport` for work the server had already committed, and left
the *previous*, older index on screen. `refresh_pending` now returns
`PendingIndexGap` (`examples/todo-core/src/app/index.rs`), the application caches it, and the
status bar renders it. The shape is worth noting for entry 12: whatever core eventually offers, an
application needs to be able to say *my index is n records short*, and today it can only discover
that by counting the queue twice.

This is now register entry 12. It is deliberately *not* a decision: the trial's job is to find, and
the fix has at least three candidate shapes (ids on the report, a callback per outcome, or a store
query for "pending ids only") whose costs differ enough to need arguing rather than picking.

## Finding 2 — The Direct-Dispatch Bet Holds, And Is Incomplete

`wiki/proposals/extraction-boundary.proposal.md` keeps direct-dispatch-then-fallback out of core on
the bet that a caller-supplied `MutationId` is enough, and says to revisit if D4 cannot express a
real flow that way. Toggle was the flow to try it on, and **the bet holds**: one id, generated once,
used for the direct `PUT` and for the queued replay, and the server answers `Duplicate` if both
arrive. No second send path, no third status vocabulary, no terminal-refusal error variant — the
three things that proposal priced and refused.

The bet is incomplete in one specific way. Deciding *whether* to fall back means deciding whether
the direct failure was terminal, and that is `wiki/decisions/019-verdict-synthesis.decision.md`'s
problem sitting in an application. `examples/todo-core/src/app/direct.rs`'s `classify` is fifteen
lines and encodes 019's own rule — `4xx` is not automatically terminal, because `401` and `429`
are both retryable. Fifteen lines is cheap. **Every caller writing them differently is not**, and
the ones who get it wrong will queue a permanent refusal forever or drop a write that would have
succeeded.

Register entry 13. The likely answer is a documented pattern rather than an API, since the mapping
is transport-specific by construction — but that is a decision, and this is a finding.

Post-review repair, 2026-08-30: the direct path now goes through `HttpTransport`, so the same
offline switch and HTTP client are used by both batch sync and direct dispatch. A row that already
has pending work is queued rather than raced directly, because the server cannot toggle a row before
the queued create reaches it. A terminal direct refusal carries a structured
`RemoteRejection` and rolls back the optimistic toggle.

## Finding 3 — The Envelope's Routing Cost Falls On Both Ends, And Was Priced On Neither

The durable queue persists `method`, `path`, and a JSON body, and that is what makes it
domain-neutral. `wiki/proposals/extraction-boundary.proposal.md` records the benefit. Neither it nor
decision 010 records the cost, which the trial met twice in one afternoon:

- **On the server**, the routing a framework normally supplies has to be written by hand:
  `examples/todo-server/src/domain.rs`'s `route` matches on `(method, path segments)` to recover a
  domain effect. An axum `Router` cannot do it, because the request is a batch of envelopes rather
  than a request per envelope.
- **On the client**, recovering *which row* a queued envelope is about means parsing the path back:
  `examples/todo-core/src/app/index.rs`'s `row_id_of`. The queue is domain-neutral, so the
  application supplies the domain reading — and it does so twice, once when composing the path and
  once when taking it apart.
- **On the server again, and this one the review found: replay-safety is a per-method judgment the
  envelope does not carry.** A queued `PUT /todos/{id}` that arrives after the row is gone must be
  refused — it carries a title with nowhere to land. A queued `DELETE /todos/{id}` in the same
  position must be *applied*, because it names no state to lose and two devices deleting one todo
  offline is the ordinary case rather than an error. Same envelope shape, same missing row, opposite
  correct answers. Nothing in `method` plus `path` plus a body says which, so every server
  replaying this queue decides it again by hand, exactly as every client decides `classify` by hand
  in Finding 2.

Neither is a defect and neither suggests changing the envelope. Both are worth writing down, because
"the envelope is domain-neutral" reads as free and is not.

## Finding 4 — Two Predicted Tensions Were Not Reached

Recorded so that "no problem found" is not mistaken for "tested and fine".

- **Preconditions against a batch.** The proposal's §4 and §6 both flag that `precondition` is
  `#[serde(skip)]` and travels as a header, so a batch of N records with N different preconditions
  has one request and one header. The todo flow never sets a precondition — last-write-wins on a
  checkbox is correct — so the tension did not arise. It is untested, not absent.
- **Cache invalidation.** `InvalidationRunner` and decision 023's row-level markers are not
  exercised: the trial refetches the whole list. That is exactly the "refetch everything or refetch
  nothing" the proposal predicted, and it is invisible on three rows. It would be the first thing to
  build if D4a were extended rather than the second.
- **Concurrency, and this one is a property of the harness rather than a choice.** The server's
  pool is `max_connections(1)`, because a SQLite `:memory:` database is private per connection.
  Everything therefore serializes, so the trial cannot put two batches carrying the same
  `mutation_id` in flight at once. The duplicate check reads inside the transaction and
  `applied_mutations.mutation_id` is a primary key, so the protection is there by construction —
  but it is argued, not observed, and a test written against this harness would be theatre. D4b
  runs against a file-backed database and can do better.

## The UI, And What It Cost To Wire

The UI works and is not the interesting part. What it cost to wire is.

`examples/todo-app` is ~540 lines over `todo-core`, built with `dioxus 0.7.10` at
`default-features = false` plus `macro, html, signals, hooks, web, launch`. It builds and lints
clean for `wasm32-unknown-unknown` under `-D warnings`, which is the standard D3b set for itself,
and it is now gated (`build ui wasm`, `clippy ui wasm`).

### The adapter's hooks do not compose with a real application

This is the finding, and it was the point of building the UI at all.

`frontbox-dioxus` offers two hooks. **Neither could be used.** Both failures have the same root and
either one alone is fatal.

**`use_frontbox` requires the runner by value.** Its signature is
`use_frontbox<S, T>(init: impl FnOnce() -> SyncRunner<S, T>) -> FrontboxHandle<S, T>`, because the
handle had to own the one `Rc` every
consumer shares. `TodoApp` holds its `SyncRunner` as a private field and exposes `store`,
`transport`, `outbox`, `sync`, `refresh_pending`, `refresh_from_server` and `dead_letters` — no
`runner()` and no `into_runner()`. There is nothing to hand over.

**There is a near-miss that compiles, and it must be named because someone will write it.**
`InMemoryStore` is `Clone` and its clones share one backend — `InMemoryBackend` holds
`Rc<RefCell<State>>` (`src/memory/mod.rs:113`) — so

```rust
use_frontbox(|| SyncRunner::new(app.outbox().clone(), HttpTransport::new(BASE_URL)))
```

type-checks. It is wrong, and it is wrong in the specific way this crate exists to prevent. The
in-flight flag is `Cell<bool>` on the *runner* (`src/runner/mod.rs:46`), not on the store, so the
second runner shares the queue and shares nothing else: two drain loops over one outbox, each
believing itself alone. `SyncPass::AlreadyRunning` becomes unreachable — the guard is still there,
and it is guarding the wrong thing.

**Corrected 2026-08-30.** The last sentence is now the fixed defect rather than the live one. The
flag moved off the runner and onto the scope (decision 031), so the near-miss above stands down with
`AlreadyRunning` instead of racing. It is still not what this application wants — a second runner
drains behind `TodoApp`'s back and skips the half of `sync()` that rebuilds the pending index — but
it no longer double-sends, and conformance case 61 is what keeps that true.

**Second, even given a handle, `use_sync_loop` drains behind the application's back.** It calls
`handle.runner().drain()` (`crates/frontbox-dioxus/src/sync/mod.rs`). But `TodoApp::sync()` is
`drain()` **followed by** `refresh_pending()`, and the second half is load-bearing: it rebuilds the
row-to-pending index that `TodoStore::is_saving` answers from. That index cannot be maintained
incrementally, for Finding 1's reason — a `DrainReport` names no drained ids. Route the loop
through `use_sync_loop` and every row's "saving…" marker is set on enqueue and **never cleared**.
The UI renders correctly on first paint and is permanently wrong after the first drain.

So the app writes `use_todo_sync` (`examples/todo-app/src/sync.rs`), ~45 lines that transcribe
`use_sync_loop` and call `ui.app.sync()` instead of `runner.drain()`. Same cadence, same signals,
same "refresh the counts even when the drain failed" argument.

### What did compose, unchanged

- **`Sleeper`** — `Sleeper::new(gloo_timers::future::TimeoutFuture::new)` is a direct fn-item
  coercion. The injected-sleep seam is exactly right and cost nothing.
- **`SyncCadence` and `SyncCadence::after(DrainEnd)`** — the whole backoff policy, reused verbatim.
- **`OutboxCounts::read(app.outbox())`** — takes any `&S`, needs no handle.

### The shape of it

**The adapter's plain values and free functions survived contact with a real application; both of
its hooks did not.** The hooks assume `SyncRunner` is the top of the object graph and that Dioxus
owns it. `TodoApp` is the shape an application actually has: it owns the runner, wraps `drain` in
something larger, and hands out only what it means to. `use_sync_loop` is the one thing
`frontbox-dioxus` offers that core does not, and the only application in this repo cannot call it.

The smallest honest fix is to stop requiring a `FrontboxHandle`. The hook needs two capabilities —
"run one sync, tell me how it ended" and "read the counts" — and both can be closures, leaving
`FrontboxHandle` a `use_sync_loop_on(handle, ..)` convenience for applications with no wrapper of
their own. **That is an API change to a built deliverable, so it is recorded here and not made**;
it belongs to D3b rework, not to D4a. Register entry added.

### Four smaller costs, each of which will be paid by every Dioxus consumer

- **There is no `Clock` for wasm.** `SystemClock` is `#[cfg(not(target_arch = "wasm32"))]` and its
  doc says web callers supply one "through their adapter" — but `frontbox-dioxus` *is* the adapter
  and supplies none. The app writes `WebClock` over `Date.now()` and takes a `js-sys` dependency to
  do it. Five lines, and every browser consumer writes the same five.
- **`OutboxCounts` counts dead letters; nothing lists them.** The count is what makes someone build
  a panel, and the panel needs `DeadLetterStore::list`. The app polls it as a fourth signal.
- **`DrainEnd` has `Debug` but no `Display`.** The status bar renders `{end:?}`, showing the user
  the word `Stalled`. Real words mean a hand-written match over a `#[non_exhaustive]` enum — the
  second such match in the app, after `DeadLetterReason`.
- **`frontbox::Error` is not `Clone`, so it cannot live in a signal.** `frontbox-dioxus` already
  stringifies into `Signal<Option<String>>`; the app hit the same wall and made the same choice.
  Documented, so not a surprise — but every Dioxus app loses the error's structure at exactly the
  boundary where a retry button would want to know whether the failure was transport or storage.

## Finding 5 — The Framework-Neutral Half Had Never Been Compiled For The Browser

`todo-core` exists to be the half that runs anywhere. It did not.

Building the UI failed on one line, in `todo-core` rather than in the UI:

```
error[E0599]: no method named `is_connect` found for struct `reqwest::Error`
  --> examples/todo-core/src/transport.rs:65
```

`reqwest::Error::is_connect` is `#[cfg(not(target_arch = "wasm32"))]` — checked across 0.12.0
through 0.12.28, gated in all of them, so no version pin helps. The cause is not an oversight in
reqwest: its wasm backend is `fetch`, and a browser collapses every network-layer failure into one
opaque `TypeError` deliberately, so that a page cannot probe the user's network topology by timing
the difference between refused and unroutable.

**Why the gates did not catch it.** `clippy trial` and `tests trial` are host-only. Core and the
adapter each had a wasm build gate; the trial did not, so the crate whose entire claim is
portability was the one crate never compiled for the target that claim is about. Fixed:
`build trial wasm` now runs `cargo build -p todo-core --target wasm32-unknown-unknown`.

**Why the mapping matters more than the compile error.** `Error::Offline` yields
`SyncPass::Offline` — a clean `Ok`, queue untouched — while every other error propagates as `Err`
and *aborts a drain*, discarding the aggregate for every pass that already committed
(`src/runner/mod.rs:161-173`). Getting this wrong is not a lint.

The first fix attempted here was to map every wasm send failure to `Offline`, and it is wrong: it
cannot tell a wrong endpoint from an unreachable one, so a typo in a base URL would present as a
permanent, silent offline state — queue retained forever, no error ever surfaced, and the condition
a developer most needs to see rendered as the condition the UI treats as normal.

What survives on both platforms is the *layer* the failure came from. In reqwest's wasm client the
fetch yields `Kind::Request` while URL and header construction yield `Kind::Builder`
(`src/wasm/client.rs:256,280` against `:203,243`), so `is_request` names on the web what
`is_connect` names natively: the request left the builder intact and the network refused it. The
mapper is `cfg`-split on that basis, and the reasoning is on the native arm.

**The general shape is what D5 should take from this**: the web platform offers strictly less
information than the native one, and the seam is a `cfg` rather than an abstraction, because the
missing method is absent from the type rather than merely unhelpful. IndexedDB against SQLite will
meet this repeatedly.

## Finding 6 — The Read Path Was Never Wired, And No Test Could Have Noticed

**Found 2026-08-30, from a bug report rather than from a review.** The UI showed an empty list after
every reload while `GET /api/v1/todos` returned rows.

**The UI was write-only against the server.** `TodoApp::refresh_from_server` existed the whole time
and `examples/todo-app` never called it, so the browser projection only ever held what was typed in
that session. Every reload built a fresh `TodoStore::default()` and rendered nothing.

The give-away is the count. `refresh_from_server` had **six callers, and every one of them was a
test**:

| Caller | What it was doing |
| --- | --- |
| the `observations` suite, three places | Asserting the server agrees with a projection that ran ahead of it |
| the `regressions` suite, three places | Reading back what a direct dispatch or a replay committed |
| `examples/todo-app` | — |

**So the tests proved the method works, and nothing proved the application calls it.** Every test
drove the startup sequence itself, as a setup step, because a test naturally does — it is arranging
a world, and arranging includes fetching. `examples/todo-core/tests/observations/queue.rs:69` even narrates it: *"The server is the
authority, and it agrees with the projection that ran ahead of it."* That line is the trial asserting
the read path is correct, in a file that could never observe whether anybody used it.

**This is Finding 5's shape a second time, and the repetition is the finding.** There, the trial's
framework-neutral half had never been compiled for the browser, because the gates on either side of
the seam were each green. Here, `todo-core` is tested and `todo-app` compiles, and **the startup
sequence lives in the gap between them** — in the one crate with no tests, whose only gates are
`build ui wasm` and `clippy ui wasm`. Twice now, the defect has been in the seam rather than in
either thing the seam joins.

**Which is why the fix moves the sequence rather than only adding the call.** `TodoApp::start`
hydrates and then rebuilds the pending index, and returns a [`Startup`] saying whether the server
was actually read. A test can assert on that value; nothing can assert on a sequence a UI was
supposed to remember. The regression test builds a *second* application on the same scope with an
empty projection — a reloaded tab — hands it nothing but `start`, and requires the rows to appear.
It was confirmed to fail against a `start` with the hydrate removed, which is the only thing that
makes it a regression test rather than a second copy of what the six callers already proved.

**An offline start is a start.** `start` maps `Error::Offline` to `hydrated: false` and a clean
`Ok`, matching the runner's own posture exactly — `SyncPass::Offline` is an `Ok` and every other
transport error is an `Err`. Refusing to open when the network is down would be refusing to do the
one thing an offline-first cache promises. Every other transport failure still propagates, so a
wrong `TODO_SERVER_URL` stays loud rather than presenting as a quiet empty list, which is the
distinction `map_send_error` was written to preserve in Finding 5.

**What it opens.** `TodoStore::replace` clears and reinserts, which is correct only because the
outbox is empty at mount — true of an in-memory queue and false the day D4b repoints at a durable
one. Register entry 19 holds it, and it is decision 023 arriving somewhere new: the read model is
the application's, so reconstructing it over a surviving queue is the application's problem, and
D4b owes the demonstration that it can be done.

**What it does not close.** `examples/todo-app` still has no tests. A `verify.sh` grep asserting
that `main.rs` mentions `start(` was considered and rejected: it would catch this one line and
nothing else in its class, while reading like coverage. The residual risk is recorded here instead.

## Verification

`bash scripts/verify.sh` reports **ALL GATES PASSED**, with five gates covering the trial and UI:

- `clippy trial` — `-p todo-core -p todo-server`, all targets, `-D warnings`.
- `tests trial` — nineteen checks: ten original observations, eight post-review regressions, and
  the compile-fail doctest. A **primary** gate: they are the wire-format oracle.
- `build trial wasm` — `todo-core` on `wasm32-unknown-unknown`.
- `build ui wasm` — `todo-app` on `wasm32-unknown-unknown`.
- `clippy ui wasm` — `todo-app` on `wasm32-unknown-unknown`, all targets, `-D warnings`.

The `!Send` textual check now also covers `examples/todo-core/src/`. `examples/todo-server` is
excluded on purpose — it is a server, axum requires `Send` handlers, and nothing about a server has
to satisfy decision 001. The client half is not excluded, and the `observations` suite asserts it
with a real negative assertion while `todo-core` also carries a compile-fail doctest. The grep
stays secondary: a type-level assertion can show one concrete type is `!Send`; it cannot prove the
absence of every accidental `Send` bound in source.

Core's own numbers are unchanged, because core is unchanged: 59 conformance cases inside 116 tests,
89.03% regions / 96.18% lines on `-p frontbox`. Nineteen gates, all passing, as of 2026-08-30.

**One gate was considered and not written.** A build of `todo-app` with `TODO_SERVER_URL` set, to
exercise the `option_env!` branch, would be theatre: Cargo does not track arbitrary environment
variables for rebuild purposes, so the second build returns the first one's artifact and reports
`ok` for work it did not do. The script's own comment already says that is worse than no gate.

## What This Does Not Prove

The trial answers the API question and cannot answer the durability one. Storage is in memory on
both sides of the seam — the outbox *and* the application's read model — so the demo shows a retry
buffer with a good report surface, not an offline-first client. Observation 3 is the proof of that
rather than a caveat about it.

D4b is the other half: the same application, repointed at D5's SQLite and IndexedDB backends at the
single construction site, with observation 3 inverted. Its result is the diff, and if the diff
reaches into a component then the seam is in the wrong place — which would be worth more than the
demo.

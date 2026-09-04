# Single-Flight Is Per Scope, Not Per Runner

Document Class: Decision
Status: Accepted 2026-08-30; in-realm half implemented as case 61; **cross-realm half implemented 2026-08-30** in `frontbox-indexeddb` via Web Locks, with one claim about the mechanism corrected below; **the cross-realm guarantee itself observed 2026-08-31** by a two-realm browser fixture, which is the last thing D5 owed
Date: 2026-08-30
Category: Storage Contract
Scope: What single-flight has to mean once a scope's queue outlives the object that drains it, why the current guard does not deliver it, and who is obliged to.
Sources: `src/runner/mod.rs`, `src/memory/mod.rs`, `src/store.rs`, `examples/todo-core/src/app/mod.rs`
Related: `wiki/decisions/018-single-flight-drain-mode.decision.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/024-scope-storage-encoding.decision.md`, `wiki/decisions/025-quarantine-storage-shape.decision.md`, `wiki/proposals/browser-background-services.proposal.md`, `wiki/references/open-decisions.reference.md`, `wiki/roadmaps/extraction.roadmap.md`

## Decision

**At most one drain may be in flight per scope, across every handle on that scope — not per
`SyncRunner`.** This takes the shape decisions 024 and 025 established, and for the same reason:

- **The obligation is core's.** Two handles opened on one scope must never have overlapping drains.
  Nothing else about how that is achieved is core's business.
- **The mechanism is the backend's.** No new runtime dependency enters core, and no lock primitive
  appears in the `OutboxStore` signature until a backend proves it needs one there.
- **A conformance case is the enforcement**, because the obligation is otherwise prose.

**Unlike cases 49 and 50, this case has real content on the backend that exists.** Those two pass
vacuously on in-memory and exist to constrain a durable backend. This one **fails on in-memory
today**, in-process, with no browser involved. That difference is the whole reason this is a
decision now rather than a line in the D5 plan.

## Why The Current Guard Does Not Deliver It

`in_flight` is a `Cell<bool>` on the runner (`src/runner/mod.rs:46`). `InMemoryBackend::open`
(`src/memory/mod.rs:142`) hands back a handle over a shared `Rc<RefCell<State>>`
(`src/memory/mod.rs:113`), so two `open` calls on one scope are two views of one queue. Two runners
over those two handles each hold their own flag, each observe it false, and each proceed.

The roadmap's 2026-08-27 scope note says the crate "was already single-flight at the pass level and
never grew claim-leases". That is accurate and it is the source of the gap: **single-flight at the
pass level is single-flight per runner.** Nothing ever claimed it held per scope, and until the
queue was in-process and short-lived, nothing needed it to.

A drain pass is *read pending batch* → *POST* → *apply outcomes*: two storage transactions with a
network round trip between them. **Transactional atomicity cannot close this**, because the two
transactions never overlap. Whatever excludes the second drainer has to outlive a transaction.

## What Actually Breaks, And What Does Not

The failure is not general corruption, and saying so precisely is what makes the fix affordable.

| Property | Under two concurrent drains | Why |
| --- | --- | --- |
| Idempotency | **Holds** | The server dedupes on `mutation_id` and answers `duplicate` (decision 005). Double-sending is wasted bandwidth, not divergence |
| Ordering | **Breaks** | At `batch_limit = 1` the second drainer ships record 2 while record 1 is still in flight. That is exactly the guarantee decision 016's monotonic `seq` exists to provide, and decision 018's profile exists to observe |
| Retention bound | **Breaks** | Two drainers spend decision 017's `attempts` budget in parallel, so a wedged head reaches its bound in half the wall-clock the bound was chosen to represent |

So the envelope's idempotency key is doing real work here — it is why this is a liveness and
ordering defect rather than a data-loss one. It is not a substitute for the obligation, because
ordering is a first-class guarantee and not an optimisation.

## Why This Stops Being Cheap At D5

Today the exposure needs a developer to construct two runners over one scope, which the D4a plan
recorded as a near-miss under register entry 14.

**It stops needing anybody to make a mistake the moment the store is durable.** Two browser tabs on
one origin are two realms over one IndexedDB database, each with its own `SyncRunner`, each drawing
from the same queue. Nobody has to write the near-miss; the user opens a second tab. A service
worker running Background Sync would be a third such realm, and that is why
`wiki/proposals/browser-background-services.proposal.md` treats a service worker as gated on this
decision rather than as an independent feature.

The cost of being late is the usual D5 cost: a fix applied to data already on user devices.

## The Recommendation, Which Is Not The Decision

For IndexedDB, the **Web Locks API** (`navigator.locks.request`) with a lock name derived from the
same injective scope encoding decision 024 requires. It is available in window *and* worker scopes,
which is what lets one lock arbitrate between a page and a service worker.

Two things about it are worth recording so the dependency posture is not re-argued:

- It is a mutual-exclusion primitive, **not a timer and not a date library**. It does not disturb
  decision 002's clock injection or the `no date library` gate in `scripts/verify.sh`.
- It lives in the backend, not in core, so a native SQLite backend is free to use a transaction, an
  advisory lock, or a process-local mutex — whichever actually matches its concurrency model.

For SQLite the equivalent is unremarkable, because a native application usually has one process.
**That asymmetry is the point**: the obligation is uniform, the mechanism is not, and a conformance
case that only ever ran natively would report success without testing anything.

## Alternatives Rejected

- **Leave the guard on the runner and document that consumers must not build two.** This is the
  status quo restated. It fails because with a durable store the second runner is not a mistake — it
  is a second tab, which the consumer does not build and cannot prevent.
- **Put a lock handle in the `OutboxStore` signature.** Names a mechanism in core, and forces every
  backend to model a lock even where its concurrency story makes one meaningless. Decisions 024 and
  025 both declined the equivalent move.
- **Rely on idempotency alone.** Defensible if ordering were advisory. It is not: decision 016 made
  `seq` the primary sort key precisely so that order is a guarantee rather than an artifact.

## What Was Built, And The Claim It Corrects

**Implemented 2026-08-30, the same day this page was written, and one sentence above is now wrong.**
"The mechanism is the backend's" holds for the cross-realm half and does not hold for the in-realm
half, which core now owns outright.

**The two promises this page made could not both be kept.** Register entry 17's recommended option
promised *no public signature changes*; the roadmap's proof line promised *the second observing
`AlreadyRunning`*. `OutboxStore` has no channel to say "busy elsewhere" — `pending_batch` returns
rows or an error, and an empty batch makes the runner report `Idle`, which is a lie while records
are pending. A backend cannot deliver `AlreadyRunning` without core gaining a way to ask it.

So the obligation splits, and each half sits where it can actually be enforced:

- **In-realm, core's.** The flag left `SyncRunner` for the scope. `src/runner/exclusion.rs` holds a
  thread-local set of `ScopeKey`s with a drain in flight; `sync_once` claims the scope and releases
  it on drop, cancellation included. No public signature moved, no dependency arrived, and the
  near-miss under entry 14 now stands down with `AlreadyRunning` instead of racing.
- **Cross-realm, the backend's**, exactly as this page argued. Two tabs are two wasm instances with
  their own memory, and no process-local structure can see across them. The obligation is stated on
  `OutboxStore` in the shape decisions 024 and 025 use, naming Web Locks as the recommendation and
  leaving the mechanism open.

**The case is honest about which half it covers.** Case 61 opens two handles on one scope, suspends
the first drain mid-flight, and asserts the second stands down without sending. It fails against the
old runner-local guard — verified by reverting `sync_once` and watching it report `Completed` — so
it is not vacuous on the backend that exists. But it runs in one realm, because a conformance suite
drives a backend through one process, and **a durable backend will therefore pass it without holding
a cross-realm lock**. Case 61's own doc comment says so rather than letting a green suite imply more
than it proves.

## Consequences

- ~~**A conformance case is owed**~~ **Case 61 landed 2026-08-30** — two handles on one scope,
  concurrent drains, and the second observing `AlreadyRunning` rather than sending. It is in the
  single-flight profile, which is the natural home since limit 1 is where an ordering violation is
  observable at all.
- ~~**Where the flag lives is now an open question**, recorded as register entry 17~~ **Entry 17 is
  closed**, on a fourth option its own list did not contain: neither candidate it named could
  produce the `AlreadyRunning` this page promised.
- **`SyncPass::AlreadyRunning` gained a second meaning**: not only "this runner is busy" but "this
  scope is busy". The variant did not change; its documentation did.
- **What D5 still owes is the cross-realm half**, and the conformance suite cannot ask for it. A
  two-realm test — two tabs, or a page and a worker, over one IndexedDB database — is a browser
  fixture rather than a case, and the D5 plan owes it.

## Revisit If

- A backend appears whose storage layer genuinely cannot express cross-handle exclusion. The
  fallback would be to weaken the guarantee to idempotency-only and say so in the ordering policy —
  which would mean retracting part of decision 016, and should be that expensive.
- Evidence shows real consumers never open two realms on one scope. This would be surprising: the
  web platform's default is that they can, and the trial's own UI is a browser tab.


## The Cross-Realm Half, As Built (2026-08-30)

**Core states the obligation and names no mechanism**, exactly as this decision said it should.
`OutboxStore::claim_drain` returns an optional `DrainLease`; `None` means another realm holds the
scope and the pass reports `AlreadyRunning`, the same word the in-realm claim already produced. The
default grants immediately, which is the honest answer for a backend with one realm by
construction — an in-memory store dies with its process and a SQLite file is normally opened once.

**The mechanism is Web Locks**, as recommended above, and two details of it were not obvious.

- **`web-sys`'s binding sits behind `--cfg=web_sys_unstable_apis`**, a *workspace-wide* rustflag.
  Setting it would change how every crate in the tree compiles to reach one method, so the API is
  bound directly instead and the build stays ordinary.
- **Releasing a Web Lock is asynchronous, and that broke every drain.** A lock is held while the
  request callback's promise is pending, so the lease resolves that promise on drop — but the lock
  manager hands the lock back a turn later. A drain is a loop of passes, so pass N+1 asked
  microseconds after pass N released and was refused. Seven conformance cases failed, including the
  one written for this feature. The claim now retries once after a `setTimeout(0)`, which is enough
  for a release to land and far too short to mask a lock genuinely held by another tab — whose
  pass is a network round trip, not a task turn. **The retry lets the claim tell "my own
  release has not landed" from "somebody else is draining"**, which is the distinction it exists
  to draw.

**What the suite can and cannot prove.** A conformance case runs in one realm, so the cross-realm
guarantee itself stayed unasserted — as this decision predicted, and for a day it was the only
promise in D5 with no witness. What the suite does assert, through
`the_drain_lock_is_taken_and_released` in the IndexedDB suite, is the half that can fail silently
here: that the lock is really taken and really released. A lock never taken makes the mechanism a
no-op that every in-realm test would still pass; a lock never released stops the queue syncing in
every tab forever. Both fail a test. The half it cannot reach is the section below.

## The Two-Realm Observation (2026-08-31)

`crates/frontbox-indexeddb/tests/cross_realm.rs`, two tests in headless Chrome. **This is the
evidence the roadmap's D5 status had been owing since 2026-08-30**, and the reason it was owed is
worth keeping: everything green up to that point was compatible with a mechanism that did nothing.

**The second realm is a dedicated worker, not a second tab.** A tab needs a driver session that
opens one, and the harness drives a single page. A dedicated worker is reachable from inside the
test and is sufficient for what is under test: Web Locks are managed per *origin*, shared across
every agent on it, and a dedicated worker is a separate agent with its own global scope, its own
event loop, and no view of this page's memory — which is the isolation two tabs have and core
cannot bridge.

What a worker does not reproduce is a second *wasm instance*: it is plain JavaScript, so it
exercises the lock manager rather than a second copy of frontbox. **Two tests are written so that
costs nothing**, and the composition is the argument:

1. `another_realm_draining_stops_this_one_from_sending` — the worker holds the scope's lock, and
   this realm must report `AlreadyRunning`, send nothing, and **leave the queue where it was**.
   That last assertion is the one that matters: `AlreadyRunning` over an emptied queue would be the
   double-send this decision exists to prevent, wearing the right label.
2. `a_pass_in_flight_holds_the_lock_against_another_realm` — the worker asks for the same lock from
   inside `send_batch`, which is an instant the pass provably holds the lease, and must be refused.
   The probe runs inside the transport rather than in a second future, so there is no timing
   assumption and nothing that could pass by racing.

Compose them and the guarantee follows for two frontbox realms: realm A's pass holds the lock (2),
and realm B's claim refuses while it is held (1). Neither half is assumed.

**The lock name is spelled out in the test, from outside the crate.** `locks.rs::lock_name` is
`pub(crate)`, and importing it would make the fixture assert only that the backend agrees with
itself. Writing `frontbox:drain:<scope>` in the test makes it an origin-wide contract — the kind a
service worker or another library could hold the other end of.

**Three sabotage runs, because a green test that cannot fail is what produced this gap.** Each was
applied to `locks.rs`, run, and reverted:

| Sabotage | Result |
| --- | --- |
| `try_claim` grants unconditionally — the mechanism is a no-op | Both tests fail: pass 1 reports `Completed` where `AlreadyRunning` was required, and the worker takes the lock mid-pass |
| The lock name gains a `v2:` segment — a lock no other realm asks for | Both tests fail identically, which is the point: a name only this crate knows is indistinguishable from no lock |
| The lease's release closure never resolves — the lock leaks | Test 2 fails on its final assertion; **test 1 still passes**, because its release comes from the worker rather than from a lease |

The third row is why the release assertion is written where it is. It is also the failure that
would be worst in production: the first drain succeeds and every later drain, in every tab,
stands down forever.

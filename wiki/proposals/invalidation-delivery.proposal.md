# Invalidation Delivery: The Seam frontbox Never Drew

Document Class: Proposal
Status: Proposed 2026-08-31; produced decisions 037 and 038 and the D4d plan the same day
Date: 2026-08-31
Category: Cache Versioning
Scope: Why "SSE is out of scope" understated a real hole, what the inbound path actually needs, and why the first implementation should be polling rather than a stream.
Sources: `src/cache/`, `src/transport.rs`, `crates/frontbox-dioxus/src/sync/wait.rs`, `src/runner/mod.rs`, `examples/todo-core/src/invalidation.rs`
Related: `wiki/decisions/014-pull-gating.decision.md`, `wiki/decisions/015-cache-version-persistence.decision.md`, `wiki/decisions/021-cache-version-identity.decision.md`, `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/decisions/032-opaque-row-store.decision.md`, `wiki/decisions/034-no-service-origin-in-core.decision.md`, `wiki/decisions/036-no-sub-scope-partitions.decision.md`, `wiki/decisions/037-multi-service-routing.decision.md`, `wiki/decisions/038-invalidation-delivery-in-the-trial.decision.md`, `wiki/plans/d4d-multi-domain-trial.plan.md`, `wiki/roadmaps/extraction.roadmap.md`

## The Challenge, And Why It Lands

Put to the project on 2026-08-31: *"The whole cache invalidation hinges on it. Not having it is like
having built a cache that can't be invalidated."*

The standing answer was D3b's — "SSE glue deliberately unbuilt" — and the reasoning in the
adapter's invalidation module was sound as far as it goes: a stream needs an HTTP
client and a reconnect policy that belong to the application, and the stream *type* would be a
dependency choice (`futures`, `async-stream`, a channel) an adapter has no business making for a
caller.

**That answer defends the wrong boundary.** It argues core should not own an `EventSource`, which is
true and uncontested. It does not explain the asymmetry:

| | Outbound (mutations) | Inbound (invalidation) |
| --- | --- | --- |
| Wire contract | Decision 010, stated and gated | the type exists; no server obligation is stated |
| Seam | `SyncTransport` | **nothing** |
| Driver | `SyncRunner::drain` | **nothing** |
| Cadence policy | `SyncCadence`, `use_sync_loop` | **nothing** |
| Application supplies | the bytes | *everything* |

frontbox never said "HTTP is out of scope, bring your own." It defined the protocol, the trait, the
loop and the backoff, and left the application one method. For invalidation it kept only the
arithmetic and left the application all of it.

The empirical form of the complaint is worse than the structural one. `use_invalidation`,
`InvalidationState` and `InvalidationEvent` appear **nowhere outside the crates that define them**.
The D2 invalidation runtime — made durable on both backends on 2026-08-30 — has never been invoked
by an application. It is a library that has never run.

## What "SSE" Was The Wrong Name For

Calling the hole "SSE glue" is part of why it stayed open: it sounds like a demo detail. Three
separable things hide under the label.

1. **The bytes** — `EventSource`, WebSocket, reconnect, backoff. Genuinely the application's, on the
   same footing as `reqwest`. Not in dispute.
2. **The seam** — a trait, mirroring `SyncTransport`. Missing, and the asymmetry above has no
   principle behind it that this project can name.
3. **Reacting to staleness** — a loop that asks a source what changed and tells somebody to refetch.
   Missing.

The practical consequence of the distinction: **a trait covers polling, and "SSE" does not.** A
server with no push exposes `GET /api/v1/versions` and the client polls it. Both satisfy one seam.
Committing to SSE specifically would bake in a push model many servers cannot offer, which is worse
than what exists today rather than better.

## The One Real Difference Between Reads And Writes

Steelmanning the current boundary, because it is not empty:

**A mutation is a replayable envelope; a refetch is not.** `MutationIntent` carries method, path and
body because the application composed the request before queueing it. A refetch has no pre-recorded
request — reads have pagination, filters, per-entity endpoints, query parameters. **frontbox can
replay your writes and cannot compose your reads.**

That is true, and it is the honest defence. It justifies a much narrower boundary than the one in
place. It says core cannot *perform* the refetch. It says nothing about owning the seam that
delivers events, the loop that asks, or the cursor that remembers where the asking got to.

## Decision 014's Premise Moved

Decision 014 justified core's position in one sentence: *"Core does not gate refetches, because core
does not perform them."* Written 2026-08-27.

On 2026-08-30, decision 032 made core own the row store, `merge_rows`, and the skip rule. Core now
holds the read model; it merely does not fetch it. **The premise moved and the conclusion was not
re-examined.**

This is the same shape as decision 023, which said "frontbox does not store read-model rows" and was
superseded in part by 032 under the same argument from the same person. Recording the parallel
because it is evidence about how this project's boundaries fail: not by being wrong when drawn, but
by outliving the reason they were drawn.

The line that survives, and should be stated deliberately rather than inherited: **frontbox owns the
storage of the read model and the knowledge of whether it is stale; it does not own its
acquisition.**

## Level-Triggered Beats Edge-Triggered For A First Implementation

The proposal is to build **polling first, SSE second**, and the reason is not effort.

A versions endpoint — `GET /api/v1/versions` → `{ "todo": "a3f8", "user": "b21c" }`, computed
from current state — is **level-triggered**. It cannot be lost and cannot be phantom. A missed
poll costs latency; the next poll still tells the truth. There is no delivery to get wrong.

SSE is **edge-triggered**, and every edge-triggered design pays for it:

- **A dropped event is permanent.** The client believes stale data is fresh, forever, with nothing
  to detect it. That is precisely the failure decision 015 exists to prevent, relocated to the
  server.
- **It therefore needs a server-side transactional outbox** — the invalidation record written in the
  same transaction as the domain change, or a rolled-back write publishes a phantom and a committed
  write publishes nothing.
- **And a resume token.** SSE's `Last-Event-ID` over a monotonic sequence is what makes a reconnect
  something other than "mark everything stale and refetch the world on every flaky connection."

None of that is an argument against SSE. It is an argument that SSE's correctness *depends on*
machinery a versions endpoint does not need, and that shipping the endpoint first gives the stream
something to reconcile against later. Robust systems generally end up with both: the stream for
latency, the endpoint for truth on reconnect — which also demotes `Last-Event-ID` from load-bearing
to an optimisation.

For a multi-domain client the arithmetic sharpens, though less dramatically than the folklore
suggests. Polling N endpoints is unremarkable. **N long-lived SSE connections** is the cost worth
weighing: the classic six-per-origin browser cap is an HTTP/1.1 limit and HTTP/2 multiplexes streams
over one connection, so on a modern stack the client side is usually fine — **the durable cost is on
the servers**, which each hold an open response per connected user for as long as they are
connected. That is a capacity question rather than a correctness one, and it is a reason to want the
endpoint anyway rather than a reason to avoid the stream.

## The Argument That Pulls The Cursor Into frontbox

If the stream ever arrives, a client must durably remember *"I have seen up to event 4711 from the
user service."* Per source. Across restarts.

That is **durable, per-scope client state**, and frontbox is the only thing in the stack that owns
any. Three backends' worth. Nothing else is positioned to hold it.

So the sharper question is not "should frontbox speak SSE." It is **"who owns the resume cursor"**,
and the answer is not seriously in doubt. It is the strongest argument for pulling the inbound path
in, and it is an argument about storage rather than about transport — which is the kind frontbox is
already in the business of settling.

## Focus Gating And Per-Entity Cadence

Two requirements arrived with the proposal, and both land better than expected.

**Repoll on focus is already expressible with no API change.** `Wake` is public and so is
`Wake::wake` (`crates/frontbox-dioxus/src/sync/wait.rs:88`). It was built for a bfcache restore —
a frozen page's timers do not fire, so a restored tab must not serve out the remainder of an
interval chosen for a live one — and a `visibilitychange` listener is the same shape of event for
the same reason. `use_bfcache_wake` is a convenience, not the mechanism. **A wake that fires while
nothing is waiting is latched rather than lost**, which is what makes it safe against a focus event
arriving mid-poll.

**Suppression while hidden needs no new primitive either**, though it needs the loop to know. A
cadence that answers "hidden" with a very long interval, cut short by the focus wake, *is* stop-and
-resume. What it is not is free: something has to observe visibility, and that observer is
platform-specific — `visibilitychange` on web, and on iOS and Android an application lifecycle
callback this project has never touched (`wiki/plans/d4c-multi-platform-trial.plan.md` records that
mobile background suspension is untested).

**Per-entity polling frequency is in tension with a batched versions endpoint**, and the tension is
worth stating before someone designs around it. One `GET /api/v1/versions` returns every entity
that server knows about; per-entity intervals then have nothing to control. Three ways out:

| Option | Cost |
| --- | --- |
| Poll each source at `min(interval)` of its entities | Slower entities get fresher data than asked for. Free, and strictly better than requested |
| One endpoint per entity | Honours the knob exactly; N times the requests |
| Per-source interval only | Simplest; drops the knob the caller asked for |

The recommendation is the first, with the knob **restated as a staleness budget rather than a
request schedule**: "this entity may be up to 60 seconds out of date" is what a caller actually
means, it is satisfiable by any source, and over-delivering against it is never wrong. A literal
"poll every 60s" is a schedule the caller cannot verify and a source cannot always honour.

## What Is Proposed

1. **Build it in the trial, not in core.** Invalidation has had zero consumers; designing
   `InvalidationSource` in the abstract is what this project has avoided everywhere else. Recorded
   as its own decision, with the promotion criteria stated.
2. **Three sources, one seam** — manual (a refresh button), polling, and later SSE. A seam with one
   implementation is a wrapper. If it holds all three without deforming, promote it.
3. **A second domain server**, so "multiple sources" is real rather than hypothetical, and so the
   outbound ordering guarantee decision 036 paid for becomes testable. See
   `wiki/plans/d4d-multi-domain-trial.plan.md`.
4. **Polling first, SSE later**, for the level-triggered reasons above.

## What Would Change The Recommendation

- **RepForge's servers already push.** Then SSE is not aspirational and the server-outbox question
  becomes urgent rather than hypothetical; the polling source stays as the reconnect reconciler.
- **The seam only fits streams.** If a polling source cannot be expressed without deforming the
  trait, that is evidence the trait is a stream wrapper and should be named one.
- **The trial shows the cursor is not needed.** If reconnect-and-reconcile against a versions
  endpoint is cheap enough, `Last-Event-ID` and the server outbox may never be worth building, and
  the strongest argument for pulling the inbound path into core weakens with it.

# RepForge Proposal: Single-Flight Drain And Its Consequences For D5 (v1 2026-08-27, v2 2026-08-28)

Document Class: Reference
Status: Recorded from conversation; not filed in `raw/`. Two revisions: 2026-08-27, 2026-08-28
Date: 2026-08-27
Category: External Input
Scope: The proposal RepForge architecture addressed to the frontbox project on 2026-08-27, recorded so that the response and the decisions it produced have something to cite.
Sources: RepForge architecture, "Proposal To The frontbox Team: Single-Flight Drain And Its Consequences For D5", received in conversation 2026-08-27.
Related: `wiki/proposals/single-flight-drain.proposal.md`, `wiki/proposals/repforge-read-model-convergence.proposal.md`, `wiki/decisions/021-cache-version-identity.decision.md`, `wiki/decisions/022-durable-trace-context.decision.md`, `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/018-single-flight-drain-mode.decision.md`, `wiki/decisions/019-verdict-synthesis.decision.md`

## Provenance Note

**This page is a paraphrase, not the original.** It carries the weakest provenance of any reference
page in this wiki, and its `Status` says so rather than borrowing the `Sourced` label the others
use. The other three reference pages cite a `raw/` path and can be checked against bytes on disk.
This one cannot: the document arrived in conversation, and `raw/` is human-curated immutable
provenance that this project's agent does not write to (`AGENTS.md`, Agent Role).

What that means for anything citing this page: the section numbering below is the original's and is
reliable, and the claims are recorded in good faith, but **no wording here should be quoted as
RepForge's own**. Where the response quotes the proposal, it quotes what was said in conversation,
and that quotation is unverifiable against a filed artifact.

**A human should file the verbatim original under `raw/`** if any of this needs to be durably
citable — which it will, because five accepted decisions now rest on it. Until then this is the
best available record and not a good one.

**Amended 2026-08-28.** The verbatim original was supplied a second time, unchanged. Two things
follow. The passages under `## Verbatim Passages` below are now quoted rather than paraphrased, so
anything citing them is on firmer ground than when this page was first written. And the provenance
gap is *narrower but not closed*: the text was transcribed from conversation both times, and this
project's agent still does not write to `raw/`. A human filing it remains the only thing that makes
it durably citable.

## Verbatim Passages

Quoted exactly, because decisions 016 through 019 turn on their precise wording.

The ask (§2):

> Make `batch_limit = 1` the supported default, and let D5 shed the scope that only existed to serve
> larger batches.

On why the ask is smaller than it sounds (§3), quoting frontbox's own documentation back at it:

> `limit = 1` is the endpoint of that reasoning, not a departure from it. Blast radius of exactly
> one record. The hazard decision 005 was written to contain stops being a policy question and
> becomes structurally unreachable.

On `Retain` losing its producers (§4), the sentence decision 017 contradicts:

> | `Blocked` | `Retain` | unreachable |
> | `Pending` | `Retain` | unreachable |

...set against, four rows later in the same section:

> **012 — Unknown mutation status — *more valuable, not less*.** Forward-compatible handling of
> unrecognised statuses becomes **more** important during a taxonomy migration, not less.

The structural ask (§5):

> The ask: a monotonic sequence assigned at enqueue, persisted, and used as the primary sort key —
> the `seq` in `(seq, mutation_id, method, path, body, state)`.

...with the scope constraint decision 016 declines:

> It must be monotonic **per scope**, consistent with decision 009's scope isolation.

The round-trip estimate decision 018 corrects (§7):

> At 200 mutations and 100 ms RTT that is ~20 s of background drain. RepForge's position is that
> this is acceptable for a resumable background process, but this is the number most worth
> challenging.

And the pull-gating flag (§4), which the response answers rather than accepts as open:

> This is not an objection to single-flight — it is an interaction the team is better placed to
> resolve than RepForge is, and it should be settled before D5 rather than discovered in D4.

The response is `wiki/proposals/single-flight-drain.proposal.md`. Where this page and the response
disagree about a fact, the response is the checked version — it was written against the code.

## What Changed On The RepForge Side (Original §1)

RepForge is removing its BFF. Five decisions, of which the last two reach into frontbox:

| # | RepForge decision | Stated consequence for a client |
| --- | --- | --- |
| 1 | No BFF. A dumb gateway does TLS, auth verification, and path routing only. | No central endpoint accepting mutations for multiple services. |
| 2 | Services are `user`, `workout`, `coaching`, `billing`. Exercise merges into workout. | Mutation targets are per-service origins behind one gateway host. |
| 3 | Writes become `PUT` with client-generated ids in the path. | A retry is the identical request; idempotency is structural. |
| 4 | `mutation_jobs`, waiter state, and internal HTTP dispatch are deleted. | `Pending` has no producer. |
| 5 | Drain is sequential, one mutation per request. | `Blocked` has no producer. |

`Pending` meant "the BFF durably accepted this and did not observe a terminal outcome before its
timeout" — an intermediate custodian that no longer exists. `Blocked` meant "skipped because an
earlier mutation *in the same ordered batch* failed terminally" — and under one request per
mutation there is no same batch.

## The Ask (Original §2)

> Make `batch_limit = 1` the supported default, and let D5 shed the scope that only existed to
> serve larger batches.

Explicitly not a rewrite, and not a request to remove batch machinery from core.

## Claims Made About frontbox's Code (Original §3)

Three, all verified correct in the response:

- `pending_batch(self.batch_limit)` already takes a bound, so `batch_limit = 1` is configuration
  today rather than a new code path.
- `sync_once` is already single-flight at the pass level via `in_flight: Cell<bool>` and
  `InFlightGuard`. There are no claim-leases to remove.
- `apply_outcomes` is already atomic and already the only mover of records.

The proposal also argues that `limit = 1` is the endpoint of `pending_batch`'s own documented
reasoning about bounding the blast radius of a server verdict, not a departure from it.

## Decision-By-Decision Impact As RepForge Read It (Original §4)

- **005** — rationale narrows, mapping survives. `Blocked` and `Pending` lose their producers but
  both variants and both mappings should stay, for forward compatibility during migration and
  because `Retain` is the right disposition for an unexpected arrival. `Duplicate` is *not*
  promised to disappear under `PUT`.
- **010** — the premise is expiring. Passthrough compatibility was chosen to spare D4 a translation
  layer; the server it is compatible with is being replaced, so passthrough would make D4 a
  translation layer in the opposite direction. Recommendation: treat as a live D4 question, do not
  pre-emptively rewrite `protocol.rs`. The envelope *fields* are affirmed as correct.
- **012** — more valuable, not less. A client will meet servers on both sides of a taxonomy
  migration.
- **003** — unchanged requirement, smaller span. One outcome per transaction rather than N.
- **014** — flagged, not answered. Single-flight drains more slowly, so the outbox stays non-empty
  longer and a pull gated on "outbox empty" may never fire.

## The Structural Ask (Original §5)

A durable **monotonic sequence assigned at enqueue**, persisted, and used as the primary sort key,
replacing `(created_at, mutation_id)` as the ordering basis.

The argument: `created_at` is a client clock reading and same-millisecond ties break on a v4 UUID,
which is arbitrary. RepForge's worked example is a rapid three-write chain —
`PUT /workouts/{w}`, `PUT /workouts/{w}/exercises/{e}`, `PUT /workouts/{w}/exercises/{e}/sets/{s}` —
that can share a millisecond, where a set sorting before its session arrives against a session that
does not exist.

Stated scope constraints: per-scope monotonicity consistent with decision 009; must survive
restart; `(created_at, mutation_id)` retained as a tiebreak for pre-column rows. Priced explicitly
as a **net addition** to D5, traded against bounded-concurrency concerns the proposal believes go
away.

## Explicit Non-Asks (Original §6)

Not removing `Blocked` or `Pending` from the enum; not removing batch types from `protocol.rs`;
not removing `pending_batch`'s `limit`; not changing decisions 001-004, 006-009, 011, 013, 015; not
touching the RFC 3339 work; no Dioxus-specific change; no change to D2.

## Objections RepForge Pre-Answered (Original §7)

- **Bounded batches bound memory.** Answered by separating the *wire* bound (becomes 1) from the
  *storage* bound (stays). D5's "bounded pending queries" should stay exactly as written.
- **N round trips is a regression.** Estimated at 5-15 sequential round trips per workout, and
  ~20 s for a 200-deep queue at 100 ms RTT. Named as the number most worth challenging.
- **This discards D1 work.** Argued to affect only the batch-size default and the multi-result
  response walk.
- **This couples frontbox to RepForge.** Answered by `batch_limit` staying configurable — a
  default, not a constraint — with conformance coverage so single-flight is first-class rather than
  a degenerate configuration.
- **Shouldn't D4 discover this?** Partly conceded for decision 010; refused for §5 because a
  storage-schema property discovered in D4 still changes schema in D5, with less time.

## What Would Falsify It (Original §8)

Realistic queue depths making sequential drain unacceptable; the sequence proving materially harder
in IndexedDB than SQLite; ordering being better solved application-side; or D4 showing the
per-service `PUT` shape does not work end to end.

## Questions Asked Of The Team (Original §10)

1. Is a durable per-scope monotonic sequence feasible in IndexedDB without a second round trip on
   every enqueue?
2. Does the pull-gating starvation case already have an answer that was missed?
3. What queue depths did the source system actually see in practice?
4. Would `batch_limit = 1` be better as a default, or as a distinct explicitly-named mode?

Answered in the response's `## Answers To The Four Questions`.

## The 2026-08-28 Revision

A second version arrived on 2026-08-28. Everything recorded above is the 2026-08-27 document and is
carried forward unchanged in it; this section records only what is new. The response is
`wiki/proposals/repforge-read-model-convergence.proposal.md`.

**The headline is a withdrawal.** The v1 document's §6 said "Not asking D2 to change". v2 strikes it:

> ~~**Not** asking D2 to change.~~ **Withdrawn.** The 2026-08-27 version said this and it is now
> false. See section 5b.

The revision's own preface calls that "the most load-bearing claim in the document", which is
correct. It also states that D2's plan is "dated 2026-08-27 and unauthorized" — **D2's code was
built, tested, and shipped on 2026-08-27**, so §5b.1's changes land on public types that exist
rather than on a plan. The response's §5 takes that up.

### New RepForge decisions (v2 §1, rows 6-9)

| # | Decision | Consequence for a client |
| --- | --- | --- |
| 6 | Per-service state endpoints with an XOR set hash per entity type. | No single cross-service state pull. |
| 7 | SSE `invalidate` carries the new set hash. | The client can skip a pull when hashes match. |
| 8 | Public read-model DTOs are shared Rust types in a `contracts` crate. | Server and client hash byte-identical input. |
| 9 | One OTel pipeline with W3C trace context end to end. | The outbox record carries a `traceparent`. |

### §5.2 — a `traceparent` on the record

> The ask: **frontbox generates a `traceparent` at enqueue and stores it on the record**, sending it
> as a header on the drain request. Generating it at drain time instead would lose the
> enqueue-to-send interval, which is exactly the span that matters for an offline queue.

Answered by decision 022: storage accepted, generation declined.

### §5b.1 — set hash rather than counter

> `row_hash  = H(id ‖ canonical_dto)`
> `set_hash ^= old_row_hash        // remove`
> `set_hash ^= new_row_hash        // add`

With the reasons given: a counter is bumped by idempotent `PUT` replays that change nothing, and
`max(updated_at)` never notices a delete. Answered by decision 021.

### §5b.2 — the skip, and its own honest limit

> Honest limit: the skip only fires when the client's optimistic projection is byte-exact with what
> the server computed. **This can only help; it should not be designed around.** Expect it to fire
> rarely at first.

### §5b.3 — the shared hash input, and the silent failure it guards

> If server and client hash different shapes, hashes never match, the skip never fires, and
> **nothing fails loudly** — the optimisation silently never works.

The suggested row shape is `(seq, id, entity_type, blob, hash, stale, …indexed columns)`, with
"share the DTO, not the storage schema". Decision 023 accepts the DTO half and declines the row half.

### §5b.4 — the permanent data hole

> row has pending mutation  → pull skips it
> mutation is Rejected      → dead-lettered
> server state never changed → NO second invalidation ever fires
> local row keeps the optimistic value the server refused — forever

The trigger proposed is the outbox emptying "(applied, rejected, OR dead-lettered)". Decision 023
accepts it and notes decision 017 widens the hole it closes.

### §5b.5 — dead-letter causes

> | `Rejected` | The server refused | No — explainable to the user |
> | Timeout | The client gave up; the server never refused | Yes — the user may retry by hand |

### §5b.6 — the sink boundary

> **emit `tracing` spans and events; let the application choose the sink.**

Accepted; decision 020 amended, including a withdrawal of two arguments it had made against.

### The eight questions (v2 §10)

Write side: (1) per-scope sequence in IndexedDB without a second round trip; (2) real queue depths;
(3) default versus named mode. Read side: (4) does decision 015 assume ordering; (5) should the hash
live in core or the contracts crate; (6) does decision 007's registry take URL-segment names or
expect an enum; (7) is a persisted row-level `stale` flag compatible with D2. Cross-cutting: (8)
will frontbox state kafkaman's sink boundary.

All eight answered in the response's `## 6. Answers To The Eight Questions`.

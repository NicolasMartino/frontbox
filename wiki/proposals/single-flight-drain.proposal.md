# Response To RepForge: Single-Flight Drain

Document Class: Proposal
Status: Proposed
Date: 2026-08-27
Category: Sync Semantics
Scope: frontbox's answer to RepForge's 2026-08-27 single-flight proposal — what is accepted, what is refused, and the two places the trade was mispriced.
Sources: `wiki/references/repforge-single-flight-proposal.reference.md`, `src/runner.rs`, `src/store.rs`, `src/record.rs`, `src/transport.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/mutations.rs`
Related: `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/018-single-flight-drain-mode.decision.md`, `wiki/decisions/019-verdict-synthesis.decision.md`, `wiki/decisions/005-mutation-outcome-policy.decision.md`, `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/decisions/012-unknown-mutation-status.decision.md`, `wiki/decisions/014-pull-gating.decision.md`, `wiki/roadmaps/extraction.roadmap.md`

## 0. Summary

The proposal was read against the code rather than against a description of the code. Its three
factual claims about frontbox all check out, its reading of decision 010 is right and understated,
and the direction is accepted.

The ledger in its §5 is wrong in both directions, and the error in each direction changes what
should be built.

| Ask | Answer | Where |
| --- | --- | --- |
| Durable monotonic sequence at enqueue | **Accepted** — but as a bug fix, not as the price of single-flight | decision 016 |
| `batch_limit = 1` as the default | **Refused** | decision 018 |
| Single-flight as a first-class supported mode with conformance coverage | **Accepted in full** | decision 018 |
| Keep `Blocked`/`Pending` variants and mappings | **Agreed** | no change |
| Don't pre-rewrite `protocol.rs`; let D4 answer decision 010 | **Agreed** | decision 019 |
| D5 sheds multi-item batch scope | **Partly** — it gains two columns, not one | decisions 016, 017 |

And one thing frontbox is adding that the proposal did not ask for, because it is the precondition
for everything else: **retention has to be bounded before `batch_limit = 1` is safe** (decision
017). That is a second net addition to D5, and unlike the sequence column it has policy content.

## 1. What Checks Out

All three claims about frontbox's runtime are correct.

- `pending_batch(self.batch_limit)` takes a bound today and `with_batch_limit` clamps with
  `limit.max(1)`, so 1 is already the floor rather than a degenerate value. No new code path.
- `sync_once` is already single-flight at the pass level. `in_flight: Cell<bool>` refuses re-entry
  and `InFlightGuard` releases on every exit including cancellation. There are no claim-leases,
  because the crate never grew them.
- `apply_outcomes` is atomic and is the only mover of records.

The reading of decision 010 is also right: passthrough compatibility was chosen to spare D4 a
translation layer, and a server that no longer exists cannot be the thing worth being compatible
with. Decision 019 takes that further than the proposal does — see §4.

## 2. First Correction: The Ordering Ask Is A Bug Fix, Not A Price

§5 argues that a wrong sort order was survivable under batching because the server received the
whole batch and evaluated it in request order, and becomes fatal under single-flight because each
request commits independently.

The premise does not hold. `runner.rs:263-264` builds the request from `pending_batch` order:

```rust
let request =
    MutationBatchRequest::new(records.iter().map(|record| record.to_intent()).collect());
```

A mis-sorted queue is therefore a mis-*ordered batch*. A server evaluating in request order meets
the set before the session at `batch_limit = 100` exactly as it would at 1, and the set is refused
against a parent that does not exist either way. Batching never bought causality. It bought one
round trip.

This does not weaken the ask — it strengthens it. The sequence column is owed whether or not
single-flight ever ships, `pending_batch`'s own doc comment already concedes the gap in as many
words, and the item has been open in this wiki since D0a. It should come off the cost side of the
trade, because RepForge is offering to pay for something frontbox already owed.

**One change to the shape as specified.** §5 asks for per-scope monotonicity "consistent with
decision 009's scope isolation". Don't make it per-scope. Reads are scope-filtered already, so a
globally monotonic sequence is monotonic within every scope for free. The per-scope framing is what
makes the IndexedDB question in §10 hard — it implies a durable counter row per scope,
read-modify-written inside every enqueue. Drop it and an `autoIncrement` object store supplies the
whole guarantee in the insert's own transaction.

Recorded as decision 016.

## 3. Second Correction: The Cost §5 Does Not Carry

**`batch_limit = 1` converts bounded head-of-line blocking into total head-of-line blocking.**

`pending_batch` is oldest-first with no cursor and no skip. At 100, a retained record at the head
starves its window while the other ninety-nine still go out. At 1, the window *is* the head: one
retained record freezes the entire queue permanently, and every subsequent pass reads it, gets the
same verdict, and sends nothing else ever again.

§4 argues both that `Retain` becomes unreachable — `Blocked` and `Pending` lose their producers —
and that decision 012 becomes *more* important during the taxonomy migration. Those cannot both
hold. Decision 012's `Unknown` is a reachable and *permanent* `Retain`: `Blocked` self-clears in one
round, `Pending` resolves when the server settles, and `Unknown` resolves only when the client is
rebuilt with a vocabulary it does not have. Saying 012 will earn its keep during the migration is
the same as saying the permanent case will fire — and it fires at exactly the moment single-flight
is running against a server whose status taxonomy is mid-change.

It cannot be fixed by skipping the stuck head, because §5 asks for causal ordering and skipping is
precisely the reordering that ordering exists to forbid. The two asks are in tension, and the only
resolution that keeps both is to *terminate* the stuck record rather than step around it: an attempt
bound, dead-lettering at the bound.

That forces the open item this wiki has carried since D1 — *"whether retained work needs aging,
attempt tracking, or last-error metadata"* — to be answered on D5's timeline. Recorded as decision
017, which also rejects aging on the merits: a record queued for a week because the user was offline
has aged without ever being evaluated, and dead-lettering it would punish the operating mode the
crate exists to support.

**So the trade is: D5 gains two columns, `seq` and `attempts`, and sheds bounded-concurrency
concerns it never had.** That is still a favourable trade. It is not the one that was offered.

## 4. `Blocked` Loses Its Producer But Keeps Its Hazard

With no BFF, nothing produces a `MutationBatchResponse`. The transport adapter synthesizes
`MutationResult` values from HTTP responses, which means **the client now decides terminal versus
transient** — a responsibility the server used to hold.

The condition `Blocked` named does not disappear with the word. "This write failed only because a
predecessor had not landed" still happens, and now arrives as a bare `404` that nothing
distinguishes from a genuine refusal. Decision 005's closing caution was written about a peer
library and now describes frontbox's own transport layer:

> `Rejected` is a server verdict, not an HTTP status class, and it must stay that way. Redux
> Offline's default treats all 4xx as permanent, and its own documentation immediately overrides
> that for `401`.

This is §2's hazard seen from the other side. Correct ordering *prevents* the missing-parent `404`;
verdict synthesis is what has to be right when ordering is violated anyway — and it can be even with
a perfect sequence, because a drain interleaved across four service origins does not stop origin B
when origin A fails. Neither substitutes for the other.

Recorded as decision 019, which also states the stronger reading of decision 010: it is not that the
envelope becomes one transport's choice, it is that `SyncTransport::send_batch` returning a
*server-produced* response stops describing anything real. The types survive as the runner's input
vocabulary. What changes is who authors them and on what evidence.

**This is the one request frontbox would make of the redesign rather than accommodate.** If the
services return a structured error body that classifies terminality explicitly — "this will never
succeed" versus "retry after the parent exists" — the judgment goes back to the side that has the
information, and decision 019 collapses into a mapping table. That is cheap to specify while the
services are still being designed and expensive to retrofit after.

## 5. The Twenty-Second Estimate Is Off By The Sync Cadence

§7 estimates a 200-record backlog at ~20 s, from 200 sequential round trips at 100 ms RTT. That
arithmetic assumes back-to-back requests.

`sync_once` sends one batch per call and returns `SyncPass::AlreadyRunning` on re-entry, so the
drain rate is set by how often the adapter calls it. The source's loop sleeps between passes:

```rust
const SYNC_INTERVAL_MS: u32 = 5000; // 5 seconds
```
(`persistence/mutations.rs:731`)

At that cadence, 200 records at `batch_limit = 1` is roughly **seventeen minutes**, not twenty
seconds.

The estimate is recoverable, but only by adding something that does not exist: a D3 adapter that
keeps calling `sync_once` until it reports `Idle`. That is a real deliverable and it has its own
prerequisite, because a drain-until-idle loop against a frozen head is a spin. The honest sequence
is decision 017, then the drain loop, then the estimate. Flagged in decision 018.

## 6. §4's Pull-Gating Question Already Has An Answer

It was settled by decision 014 on 2026-08-27, the same day the proposal was written, so this is a
crossing rather than an oversight.

**Core does not gate the pull, because core does not perform the pull.** The starvation case in §4
describes the source's loop, not frontbox's — there is no `if outbox.is_empty() { pull() }` anywhere
in the crate to starve.

More usefully, D2 already dissolves the problem rather than refining it. `InvalidationRunner`
reports staleness and pending-write conflict together, and `stale_classified` attributes the
conflict **per entity** through a caller-supplied classifier. So the application gates on "a
mutation for *this* entity is queued", not on "the outbox is empty". Both directions §4 suggests —
gate on no-terminal-failures-pending, or allow a pull when the queue is draining cleanly — keep a
whole-queue condition and therefore inherit a whole-queue starvation. The condition to stop using is
the global one.

Without a classifier the answer degrades to `PendingConflict::Unattributed`, which is a false
positive in the conservative direction and never wrong in the unsafe one.

## 7. Agreed Without Reservation

- **Keep `Blocked` and `Pending` in the enum with their mappings.** `Retain` is the correct
  disposition for an unexpected arrival, and a client will meet servers on both sides of the
  migration. No code change.
- **`Duplicate` is not dead under `PUT`.** Agreed, and decision 019 adds the reason it is *harder*:
  a replay and a first write can both answer `200`, so a transport often cannot distinguish them.
  Both map to `Delete`, so nothing breaks — but a transport that can tell should say so.
- **Do not pre-emptively rewrite `protocol.rs`; let D4 answer decision 010 empirically.**
- **Decision 012 is more valuable during a taxonomy migration.** Agreed, and §3 above is the
  consequence of taking that seriously.
- **Decision 003 is unchanged in requirement and smaller in span.** With decision 017 the span grows
  slightly again — a `Retain` becomes a write, because it increments an attempt count — which the
  existing atomicity requirement already covers.
- **The bound separation in §7 is the sharpest thing in the document.** Wire batch size and pending
  query size are different bounds and only one should move. D5's "bounded pending queries with
  deterministic ordering policy" stays, with "deterministic" upgraded to "faithful to enqueue order"
  by decision 016.

## 8. Answers To The Four Questions

**1. Is a durable per-scope monotonic sequence feasible in IndexedDB without a second round trip?**
Probably not as specified, and it does not need to be. Make it globally monotonic instead — scope
filtering on read gives per-scope monotonicity for free — and `autoIncrement` supplies it in the
insert's own transaction with no read-modify-write. The per-scope requirement is the whole
difficulty and it buys nothing. See decision 016.

**2. Does the pull-gating starvation case already have an answer?** Yes — decision 014, plus D2's
per-entity classifier. See §6.

**3. What queue depths did the source system actually see?** Unknown, and this project cannot
override the estimate. The copied corpus has a `pending_count` counter and no telemetry, no
histogram, and no logged depth distribution. If RepForge has production data it is better evidence
than anything in `raw/`.

**4. Default, or a distinct explicitly-named mode?** Neither, precisely. `with_batch_limit(1)`
already works, so nothing is blocked and no new mode name is needed. What is granted is the
conformance coverage that makes it first-class rather than merely permitted. What is refused is the
default, because a default is what an application gets when it has expressed no opinion, and at
limit 1 the liveness argument depends on decision 017 being switched on. Defaults belong at the safe
end of a setting. See decision 018.

## 9. What frontbox Is Asking For In Return

- **A structured terminality signal in the service error bodies** (§4). The single highest-value
  thing the redesign could add, and it is cheap now.
- **Confirmation that enqueue order is causal order in RepForge's flows.** Decision 016 makes the
  drain faithful to enqueue order; it cannot make enqueue order correct. If any RepForge flow
  enqueues a child before its parent, the sequence column will not save it.
- **Production queue-depth data if it exists**, per §8.
- **Early sight of the per-service `PUT` shape for D4.** Decision 019's obligation is either
  followable or it is not, and the first real synthesizer is what shows which.

## 10. What Would Falsify This Response

- **§3 is wrong if `pending_batch` gains a cursor.** The frozen-head argument rests on the runner
  reading the same oldest-first window every pass. A skip-past would dissolve it — and decision 016
  is what rules a skip-past out, so if that decision is reversed, decision 017 loses its main
  justification and should be re-argued rather than kept.
- **§5 is wrong if D3's adapter drains until idle.** The seventeen-minute figure is a statement
  about a loop that does not exist yet, not about the crate.
- **The refusal in §8.4 is wrong if D4 shows every consumer sets 1.** That would be evidence about
  the population rather than about the default, and the default is cheap to change while the crate
  is unpublished.
- **All of it is wrong if decision 010's envelope survives D4 intact**, because then the BFF-less
  premise did not reach as far into the protocol as §4 and decision 019 assume.

## Implementation Status

Nothing here is built. Decisions 016 through 019 are recorded and unimplemented, and implementation
is a design change gated by `AGENTS.md`. The dependency order is decision 017 first — it is the
prerequisite for single-flight being safe and for a drain-until-idle loop not spinning — then 016,
then the 018 conformance profile, with 019 landing as documentation whenever `SyncTransport` is next
touched.

One estimate in this response was wrong and is corrected here. It described the 018 profile as the
outbox case list re-run at `batch_limit = 1`, which the suite does not support: four cases assert
multi-record batch semantics that limit 1 removes, several construct their own runner and would run
at their own limit regardless, and the remainder assert counts taken from the seed size. The profile
is new fixtures plus a visible exclusion list, and it has to follow 016 and 017 rather than
accompany them. Decision 018's `## What The Profile Actually Costs` carries the breakdown. The
acceptance in §8.4 is unchanged — the coverage is still owed and still worth it. Only its price is.

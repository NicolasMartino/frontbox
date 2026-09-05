# A Queued Body May Be Replaced Only Before The Request Exists

Document Class: Decision
Status: Accepted 2026-09-05; implemented 2026-09-05; amended 2026-09-05 after review (see *Every Verdict Spends It*), and again the same day after a second review, which found no defect but a public contract still describing the pre-amendment rule
Date: 2026-09-05
Category: Public API Shape
Scope: What makes a queued write safe to coalesce, where that fact is written, and why it is not derived from how a send failed.
Sources: `src/store/mod.rs`, `src/store/coalescing.rs`, `src/transport.rs`, `src/runner/mod.rs`, `src/error.rs`, `src/record/mod.rs`, `crates/frontbox-sqlite/src/schema.rs`, `crates/frontbox-indexeddb/src/convert/mod.rs`
Related: `wiki/proposals/queued-write-coalescing.proposal.md`, `wiki/plans/queued-write-coalescing.plan.md`, `wiki/references/repforge-queued-write-coalescing-request.reference.md`, `wiki/decisions/004-transport-auth-and-offline.decision.md`, `wiki/decisions/006-corrupt-record-policy.decision.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/022-durable-trace-context.decision.md`, `wiki/decisions/026-replayable-preconditions.decision.md`

## Decision

`OutboxStore::enqueue_coalescing` may replace a queued record's body only while one durable fact,
`transport_started`, is false. **That fact is written inside the transaction that reads a batch for
sending, before the transport request is built, and is never cleared.**

`SyncTransport` gains `offline_now`, asked before the runner reads anything, so an application that
knows it is offline does not spend the head batch's eligibility on a pass that was never going to
reach the network.

## What This Rejects

**`attempts == 0` is not "never sent".** RepForge proposed it and it is unsafe.
`src/record/mod.rs`'s `attempts` counts verdicts *received* — decision 017 made it so deliberately,
because an offline pass and an attempted transport failure produce no verdict this client can read.
A record can therefore have reached the server, been applied, and still carry `attempts == 0`.

**And neither is `Error::Offline`.** This is the subtler one, and the first version of the design
made it: it released a pass ending in `Offline` with `transport_started` still false, so an offline
application could keep coalescing. But `src/transport.rs` tells implementors to return `Offline`
when "a browser `fetch` fails for lack of connectivity", and a browser `fetch` rejects with an
opaque `TypeError` whether the request never left the device or reached the server and lost its
response. **An adapter cannot tell those apart**, so the classification cannot carry a safety rule.

The failure is concrete and silent:

1. Edit 1 is queued as `M`.
2. A pass sends it. The server applies it under `M`. The response is lost. The adapter returns
   `Offline`.
3. The rule preserves "never sent".
4. Edit 2 coalesces: the body is replaced, `M` is kept.
5. The next pass sends `M` carrying edit 2's body. The server dedupes on `mutation_id`, answers
   `Duplicate`, and the record is deleted.

Edit 2 is gone, having never been applied, and nothing anywhere reports it. Today that same
ambiguity costs nothing — the record stays queued and idempotency covers the resend. Coalescing is
what converts a tolerated under-count into loss of the user's most recent edit.

## Why Before Rather Than After

Recording the fact ahead of the request removes the inference entirely. There is no failure to
classify, because the fact is written before there is anything to classify.

It also collapses the machinery. The first design carried two facts — `in_flight` and
`transport_started` — a three-way release taxonomy, and a recovery pass for rows abandoned by a
crashed or cancelled drain. All of it existed to serve the unsound offline rule. Under this ordering
a claimed row is already a started row, so:

- there is no lease to release;
- there is nothing to recover, because a crash, a cancelled `use_future`, or a lost response all
  leave the mark already set, which is the reading each of them needs;
- cancellation stops being a hazard, which matters because
  `wiki/decisions/031-cross-realm-single-flight.decision.md` and `src/runner/exclusion.rs` both
  record that cancellation is *routine* on the target runtimes.

One durable boolean, written once, never cleared, replaced all of it.

## Every Verdict Spends It, Not Only Every Read

`read_for_send` is not the only way a record can reach the server, and the first implementation
behaved as though it were. **`apply_outcomes` applying a `Retain`
also sets the mark** (`src/store/mod.rs`).

The hole was found by review. `OutboxStore`'s own documentation says a backend "cannot assume the
runner is its only caller", and `apply_outcomes` is public — so a caller can read with
`pending_batch`, send the batch through its own transport, and apply the verdicts itself. That path
never touches `read_for_send`. Before the fix it left `attempts` incremented and `transport_started`
false, so the record still read as coalescible *after the server had certainly seen it*, and the next
edit would rewrite its body under an identifier the server would dedupe against.

The rule that closes it is a one-liner, and it is the same reasoning as the rest of this decision
rather than a patch on top: **a `Retain` is a verdict, and a verdict cannot exist without a
request.** By the time one is applied, the question this decision exists to answer has already been
settled by the server. `Retain` is the only disposition that needs it, because every other one
removes the record from the outbox.

Conformance case 80 drives exactly that sequence — `pending_batch`, `apply_outcomes(Retain)`,
`enqueue_coalescing(RequireExisting)` — and it fails on all three backends without the fix.

It is worth saying what this was *not*: not a hole in the argument above, but a hole in its
application. The principle was "a record the server may have seen is not coalescible"; the
implementation had checked only one of the two ways a record gets seen.

**The contract has to say so too, and at first it did not.** A second review found `OutboxStore`,
`CoalescingRefusal::TransportStarted`, and the runtime spec still defining eligibility as "not read
for sending" — the rule as it stood before this section was written. Behaviour and description had
diverged with nothing failing, which is the state that manufactures the next bug: a backend written
against the doc rather than against the suite would reintroduce precisely what case 80 catches. All
three now name the durable mark and both of its writers.

### The five paths that end with the record still queued

Written out because the eligibility rule is uniform across them and nothing else is, which is the
case for storing the fact rather than deriving it:

| How the pass ended | `attempts` | Marked? | Case |
| --- | ---: | --- | --- |
| `offline_now` answered `true`, so nothing was read | 0 | **no** — the one case where that is correct | 78 |
| `Error::Offline` from a `send_batch` that already held the batch | 0 | yes | 79 |
| `Error::Transport`: a request was attempted and failed | 0 | yes | 81 |
| The response omitted this record's verdict (decision 019 synthesizes a `Retain`) | 1 | yes | 82 |
| A direct caller applied a `Retain` without ever calling `read_for_send` | 1 | yes | 80 |

Three different values of `attempts` across five paths that agree exactly on whether the server may
have seen the record. Any rule read off the first column gets at least one of these rows wrong.

## Why The Probe Is Not The Same Mistake

`offline_now` is a caller-implemented predicate that a safety property depends on, which is the
shape of thing this decision has just rejected twice. It is admissible because **its failure mode is
one-sided**:

- answering `true` while online costs one skipped pass, which the next tick recovers;
- answering `false` while offline costs the batch's coalescibility, because the runner then goes on
  to mark before sending.

**There is no answer it can give that permits a body to be rewritten under an identifier the server
may hold.** A wrong answer can only make the feature engage less often. That asymmetry is what
distinguishes it from reading `Error::Offline`, where a wrong answer silently loses a write.

The default is `Ok(false)`, honest in the way `claim_drain`'s granted default is: an adapter that
ignores it gets today's behaviour and *no* coalescing, never unsafe coalescing.

Without the probe the feature would be sound and nearly useless — a cadence loop polling while
offline would mark the head batch on its first tick, and coalescing exists precisely to collapse
edits made while offline. It is load-bearing rather than convenience.

## What A Replacement Keeps

The queued slot and its guard are the queue's; the content is the caller's latest.

| Kept from the queued record | Taken from the new intent |
| --- | --- |
| `seq`, `mutation_id`, `precondition` | `body`, `op`, `traceparent`, `created_at` |

`seq` keeps the record where decision 016 put it. `mutation_id` keeps idempotency stable. The
precondition is the point of the feature: decision 026 makes it the last state the server confirmed,
which is the only state a conflict can honestly be detected against.

Everything in the right column *describes the body*, and the body is being replaced. Keeping the
older `op` would put a stale operation name and a stale `version` — documented as "what wrote the
body" — over content it did not write, on the surface a human reads after a refusal. Keeping the
older `traceparent` would point the trace at the discarded user action, which is the link decision
022 exists to preserve. Keeping the older `created_at` would tell the server the newer body was
written at the earlier time. None of the four is ordered on, because `seq` is the only sort key.

`attempts` and `last_error` need no rule: a record that is not `transport_started` has received no
verdict, so both are already `0` and `None`.

## Refusal Is Never Silent

`CoalescingPolicy::AppendIfMissing` **cannot** return `NotQueued`. Every non-replacement case — no
`RowRef`, no match, several matches, an already-started match — appends. A caller reaching for that
policy has said it holds a precondition valid now, so appending is what `enqueue` would have done.

The first draft returned `NotQueued` for an intent with no `RowRef`, which would have handed back an
`Ok` for a write that was silently dropped: a forgotten `with_row` turning an enqueue into a no-op.
That is the failure `wiki/decisions/006-corrupt-record-policy.decision.md` exists to refuse, and
this crate spends a whole trait method — `sweep_corrupt` — refusing it elsewhere.

`RequireExisting` is the only policy that refuses, which is what it is for: a caller that cannot
honestly compute a precondition for an appended write must be told nothing was queued rather than
have one guessed for it.

## What The Caller Still Owes

**Replacement discards the queued body.** For a full-state `PUT` that is last-write-wins and
correct. For a partial update it is not: coalescing two `PATCH` bodies drops every field the first
changed that the second does not mention.

The obligation is stated on the method rather than left to the caller because the caller cannot
always discharge it — under `RequireExisting` the application does not know what body is queued, and
not knowing is the reason it cannot compute a precondition. Core cannot check it either, for the
reason decision 008 gives about bodies: it does not interpret them.

## Storage Cost

One durable boolean per pending row, and pending rows are the shortest-lived thing this crate
stores.

SQLite gained its first schema-versioning step to add it — `PRAGMA user_version` plus one
`ALTER TABLE`, guarded by `PRAGMA table_info` because a fresh database has the column and still
reports version zero. The version stamp only ever moves **forward**: an older binary opening a newer
database must not write the marker backwards, or the next new-binary open re-runs migrations against
a schema that already has their effects. Reading a future database stays permitted rather than
refused, because these migrations add columns and a newer schema is a superset — refusing would
strand queued work behind a version downgrade, which is the more expensive failure. A migration that
ever removed or retyped a column would invert that trade. The column is nullable and carries no `DEFAULT`, which keeps
`crates/frontbox-sqlite/src/schema.rs`'s stated rule intact: every writer still names it, and `NULL`
means "written before the column existed" and reads as `true`. No `UPDATE` walks user data.

IndexedDB needed no version bump — no store and no index was added — but it needed the crate's one
exception to its rule against `#[serde(default)]` on durable rows. The bound is stated with it: **a
default is admissible exactly when the defaulted value is the conservative one, the value that turns
a feature off.** A missing key reads as `true`, so such a row becomes non-coalescible and stays
resendable. Contrast `precondition`, the field that rule was written about, where absent means
conflict detection silently stops.

## Revisit If

- A native adapter turns out to be offline in a way it cannot observe before attempting a request,
  making `offline_now` one-sided only for browsers.
- Coalescing is wanted across rows, or for create/delete annihilation, neither of which this covers.
- A sealed internal trait lets the two runner-facing methods leave the public surface without
  weakening the rule.

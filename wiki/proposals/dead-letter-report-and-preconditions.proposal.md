# To RepForge: The Dead-Letter Report Body, And One Question About `If-Match`

Document Class: Proposal
Status: **Accepted 2026-08-29**; produced decisions 026 and 027, both built 2026-08-30
Date: 2026-08-29
Category: Architecture
Scope: frontbox's contribution to the dead-letter report schema RepForge invited input on, and a question about write preconditions that decides whether the outbox gains a column.
Sources: `src/record/mod.rs`, `src/store.rs`, `src/runner/mod.rs`, `wiki/references/repforge-section-c-answers.reference.md`
Related: `wiki/proposals/repforge-read-model-convergence.proposal.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/022-durable-trace-context.decision.md`, `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/decisions/019-verdict-synthesis.decision.md`, `wiki/references/open-decisions.reference.md`

## 0. Two Items

Your section C answers closed all four questions and opened two things that come back to us. This
answers one and asks the other.

Thank you for adding the `retry` field. It was the highest-value cheap thing on our list and you
turned it around in a day. Two of its properties — the status-line fallback surviving as specified
rather than as dead text, and an unrecognised value degrading to terminal while round-tripping
verbatim — are better than what we asked for.

## 1. The Dead-Letter Report Body

You wrote that only the endpoint path exists, that `mutation_id`, the cause, both timestamps and the
`traceparent` are its known contents, and that *"now is free and later is not — which is the same
argument your register makes about your own D5 columns."* Correct, so here is the field list.

### What frontbox holds

`DeadLetterRecord` is what a parked mutation actually is on our side:

| Field | Type | Send? |
| --- | --- | --- |
| `mutation_id` | `MutationId` (v4 UUID) | **Yes** — the join key |
| `method` | `String` | **Yes** |
| `path` | `String` | **Yes** — but see the caution below |
| `body` | `serde_json::Value` | **No, by default** — see below |
| `created_at` | `i64`, epoch ms, client clock | **Yes** — this is your `occurred_at` |
| `rejected_at` | `i64`, epoch ms, client clock | **Yes** — when the client parked it |
| `op` | `Option<OperationMeta>` = `{ name, version }` | **Yes** — caller's own label for what the operation was |
| `error` | `Option<RemoteRejection>` = `{ code, message, details }` | **Yes**, including when absent |
| `scope` | `ScopeKey` | **No** — see below |
| `attempts` | `u32` *(decided, unbuilt — decision 017)* | **Yes**, once it exists |
| `traceparent` | *(decided, unbuilt — decision 022)* | **Yes**, once it exists |

### Four notes that change the schema rather than fill it in

**`error: None` is information, not a missing field.** Decision 017 makes this load-bearing:
`Some(rejection)` means the server refused on the merits, `None` means **the client gave up at its
own retention bound and no server ever refused this**. Those are different incidents and they want
different handling on your side — the second is not a rejection to explain to a user, it is a client
that ran out of attempts. Please model the cause as a discriminated field rather than as "error
present or absent", so the distinction survives a JSON round trip and does not read as a null.

Under your new taxonomy the causes we can report are: **server-refused** (your `terminal`),
**conflict** (your `conflict`), and **client-gave-up** (no server verdict at all). We would rather
send that discriminator explicitly than have you infer it.

**`scope` must not be sent, and this is the one hard "no".** It is a caller-composed opaque string
whose whole purpose is local storage identity (decision 009). In your deployment it will contain a
user identifier and a tenant, and it is not authenticated — the client composes it. Putting it in a
report body invites someone to treat it as an identity claim. The request is already authenticated;
use that.

**`body` should be opt-in, not default.** It is the most useful field for debugging and the most
dangerous one to retain: it is the full request payload of a write, which for `workout` and
`billing` is user data. We would send it only where an application explicitly opts in per report,
and we would rather your schema make it nullable and your retention treat it as sensitive than have
it arrive by default and become permanent.

**`path` carries ids and sometimes more.** `PUT /workouts/{w}/exercises/{e}/sets/{s}` is fine.
Anything with a query string is not necessarily. Worth a note in your schema that the path is
client-supplied and unsanitised.

### Timestamps

Your `occurred_at` / `received_at` split maps cleanly and we would keep all four:

- `created_at` — when the user's action happened. **This is the interesting number**, and for an
  offline queue it can be days before anything else.
- `rejected_at` — when the client parked it. `rejected_at - created_at` is how long the mutation was
  alive locally.
- your `received_at` — server receipt, authoritative for ordering, as you specified.

Both of ours come from the caller's injected `Clock` and are exactly as trustworthy as the device.
Your skew-measurement observation applies to `rejected_at` versus `received_at` more usefully than to
`created_at`, since the former pair are close together in intent and the latter can legitimately
differ by a week.

### What we are not asking for

No acknowledgement semantics, no delivery guarantee, no ordering requirement between reports. This
is diagnostics, it is fire-and-forget as you specified, and a report that is lost costs a dead letter
nobody hears about — which is the status quo. Making it reliable would put a second queue behind the
queue.

## 2. The Question: How Does Your Client Attach `If-Match` Today?

Your `conflict` value is defined as *"`If-Match` failed against the current row hash"*, so writes
carry a precondition header. **frontbox's outbox has no field for one.** `MutationIntent` is
`mutation_id`, `method`, `path`, `client_datetime`, `body`, plus an optional `op`; there is no header
map and no precondition concept anywhere in the crate.

**Why we cannot guess the answer.** An `If-Match` is only meaningful if it carries the hash the user
was looking at **when they enqueued**. Computed at drain from current local state it is vacuous — it
would match whatever the client holds at that moment and detect nothing. So the value has to be
captured at enqueue and replayed at drain, unchanged, possibly days later.

That is *exactly* the shape of decision 022's `traceparent`: caller-supplied, stored durably,
replayed as a header, never interpreted by core. Two instances of one shape is where we would stop
adding bespoke columns and consider the general case.

**But it may not be ours at all.** Your transport is application-owned. It could hold the intended
base hash in your read model keyed by `mutation_id` and attach the header at drain, without frontbox
knowing. That works — at the cost of a parallel per-mutation structure alongside the outbox, which
is what the outbox exists to be.

**So the question is narrow:** *today, where does your client get the `If-Match` value from at drain
time, and is it the hash from enqueue or the current local one?*

The answer decides a schema column, and we would rather ask than design. If it comes from the outbox,
this is a fourth field landing with `seq`, `attempts` and `traceparent` in one change. If it comes
from your read model, we record that as the supported pattern and close the question.

**One asymmetry worth flagging either way.** Losing trace context degrades diagnostics. Losing a
precondition **silently disables conflict detection** — the write succeeds and clobbers. So if these
do end up as one mechanism, the precondition cannot be the optional half of it.

## 3. Two Things Settled On Our Side Since Your Reply

Neither needs anything from you; recorded so your assumptions stay current.

- **Zero-versus-never-computed is closed on the client.** `EntityState::version` is
  `Option<CacheVersion>`, so absence is `None` and is representable separately from every value
  including 32 zero bytes; conformance case 44 asserts that a client which has never synced still
  fetches a collection you report as empty. Your suggested remedy — *"a nullable stored value rather
  than a sentinel, since zero is genuinely taken"* — is what we built, on 2026-08-28. The remaining
  half is yours: `SetHash` deriving `Default` means a server that never computed an accumulator
  reports the same bytes as one whose collection is empty, and no client representation reaches that.
- **The ~17 min figure is cadence-bound, not RTT-bound.** 200 records at one per pass against the
  source's `SYNC_INTERVAL_MS = 5000` is 1000 s. It does not imply 10,000 queued mutations, and it was
  never a competing estimate: **~20 s is the floor a back-to-back drain reaches and ~17 min is what
  the source's loop actually delivers.** The gap between them is the entire value of the
  drain-until-idle loop we owe D3. Our register's phrasing invited the misreading and has been fixed.

# Quarantine Is A State, Not Necessarily A Table

Document Class: Decision
Status: Accepted 2026-08-29; case 50 landed the same day; discharged by both backends 2026-08-30
Date: 2026-08-29
Category: Storage Contract
Scope: Whether a durable backend stores quarantined rows in a separate table or as a state in the outbox, and which properties core requires either way.
Sources: `src/store.rs`, `src/record/mod.rs`, `src/memory/outbox.rs`, `wiki/decisions/006-corrupt-record-policy.decision.md`
Related: `wiki/decisions/006-corrupt-record-policy.decision.md`, `wiki/decisions/003-atomic-outcome-application.decision.md`, `wiki/decisions/024-scope-storage-encoding.decision.md`, `wiki/references/open-decisions.reference.md`, `wiki/roadmaps/extraction.roadmap.md`

## Decision

**The backend chooses the shape. Core requires four properties and the suite proves three of them.**

Decision 006 settled the policy and explicitly deferred the shape: *"The exact store shape can be
either a quarantine store or a status column in the outbox backend."* That deferral is upheld rather
than resolved, because the choice is genuinely backend-local — but the properties it has to satisfy
are named here rather than inferred from the trait signatures.

A backend may implement quarantine as a separate table or as a state on the outbox row, provided:

1. **Quarantined rows are invisible to `pending_batch` and `pending_count`.** Already proven by
   case 15.
2. **They still occupy storage and remain enumerable** through `QuarantineStore::list` and `count`.
   Already proven by cases 18 and 26.
3. **The quarantine transition is atomic with everything else in the same operation.** Decision 003
   requires deletions, dead-letter inserts, and quarantine transitions to commit together or not at
   all. **Not currently proven for the quarantine leg.**
4. **A row that fails to decode must still be movable.** `sweep_corrupt` acts on rows whose contents
   are unreadable, so the identifier and the scope must be readable independently of the payload.

**A non-normative recommendation:** prefer the state-on-the-row shape. Argument below.

## Why The Shape Genuinely Is The Backend's

Core never sees the table. `sweep_corrupt` returns a count, `QuarantineStore::list` returns
`QuarantinedRecord` values, and nothing in `src/` expresses where those rows live. The two shapes are
indistinguishable through the API, which is the definition of an implementation detail — and
decision 006 was right to leave it.

What was missing is that **the properties were implicit in the trait docs rather than stated as a
contract**, so a durable backend had to infer them from prose written about a different concern.
Property 4 in particular appears nowhere: it is a consequence of `sweep_corrupt` existing at all, and
it constrains the schema in a way neither shape escapes.

## Property 4 Is The One That Actually Constrains

`sweep_corrupt` moves rows *that cannot be decoded*. Whichever shape a backend picks, it must be able
to find and move such a row — which means **the row identifier and the scope cannot live inside the
payload blob**.

That is already forced by other requirements: ordering needs `mutation_id` in a sortable column
(decision 016 will add `seq` beside it), and decision 009's scope filtering needs `scope` readable on
every read. So property 4 costs nothing new. It is stated because a backend that stored the whole
record as one serialized blob would satisfy every other property here and be unable to implement
`sweep_corrupt` at all — and it would discover that late.

`QuarantinedRecord` shows the shape this implies: `mutation_id: Option<MutationId>` beside
`raw_mutation_id: String`, and `raw_body: String` rather than a parsed value. The type already
assumes the identifier is stored separately and may be individually unparseable.

## The Recommendation, Which Is Not The Decision

**A state on the outbox row, rather than a separate table.**

**Atomicity is the argument, and it bites hardest where it is hardest.** Decision 003 requires the
quarantine transition to commit with the deletions and dead-letter inserts around it. As a state
change that is a single-row `UPDATE` in the table the row already occupies. As a separate table it is
a cross-table `DELETE` plus `INSERT` — trivial in SQLite's one-transaction model, and in **IndexedDB
it requires both object stores named in a single transaction, declared upfront at
`transaction()` time**. That is the fiddliest part of that backend and the easiest place to get
atomicity subtly and silently wrong.

**The cost is real and smaller.** Every pending read gains a predicate and wants an index on it, and
`pending_count` must exclude quarantined rows — but `pending_count` is already scope-filtered
(decision 009), so it already carries a predicate and an index. The marginal cost is one column in an
existing index rather than a new access pattern.

A backend with a reason to prefer separate tables — a storage engine where partial indexes are
expensive, or one where the quarantine set is expected to be large and cold — satisfies this decision
by taking the other option and proving property 3.

## Consequences

- **A conformance case is owed for property 3**, which is the only one of the four not currently
  proven. Shape: with fault injection configured to fail the outcome application, sweep a corrupt row
  in the same operation as a dead-letter insert and assert that neither took effect — the quarantine
  leg of case 9's argument, which today covers the outbox and dead-letter legs only.
- **Property 4 belongs in `sweep_corrupt`'s documentation**, not only here. It is a constraint a
  backend author needs at the moment they design the schema, and the doc comment is where they will
  be looking.
- **D5's plan specifies the four properties and the case**, and each backend records its choice in
  its own compatibility note alongside its storage-name choice (decision 024). The two decisions are
  the same shape deliberately: core states obligations, the backend picks mechanisms, conformance is
  the enforcement.
- **`QuarantineStore` needs no change.** Neither shape requires a different signature, which is the
  evidence that decision 006's deferral was correct rather than merely convenient.

## Implementation Outcome

**Case 50 landed 2026-08-29**, closing the one property of the four that was unproven. It injects a
failing `apply_outcomes`, applies a `Quarantine` disposition, and asserts that nothing reached
quarantine and the record is still queued — the third leg of decision 003's requirement, which case
9 covers for the outbox and dead-letter legs and says nothing about.

Property 4 — that a row which fails to decode must still be movable — is unchanged and remains a
constraint on D5's schema rather than something a case can assert against an in-memory backend whose
rows are structs.



Quarantine grows a transition that core drives rather than the backend — a requeue, most plausibly,
returning a repaired row to the outbox. `QuarantineStore` deliberately has no `insert` and no
`remove` today, and adding either would make the shape observable through the API and therefore no
longer the backend's alone.

## Outcome: Both Backends Chose A Separate Table

Which is the option that makes the transition a **cross-store write** rather than a column update —
the harder of the two to get right, and the one case 50 exists to check.

On SQLite it is an insert plus a delete inside the one `IMMEDIATE` transaction that already covers
`apply_outcomes`. On IndexedDB it means naming the outbox and the quarantine store in one
transaction, declared before the first request, and it is the fiddliest place in that backend to be
subtly wrong: a write that throws after an earlier delete used to leave the two inconsistent,
because a dropped `IdbTransaction` commits. See decision 003's outcome note.

Both pass case 50, and case 50 was written before either backend existed.

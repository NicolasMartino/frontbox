# Response To RepForge's 2026-08-28 Revision: The Read Side And The Convergence

Document Class: Proposal
Status: Proposed
Date: 2026-08-28
Category: Architecture
Scope: frontbox's answer to the read-model, trace-context, and observability sections added in RepForge's 2026-08-28 revision, and to the eight questions it asks.
Sources: `src/cache/`, `src/record.rs`, `Cargo.toml`, `scripts/verify.sh`, `wiki/references/repforge-single-flight-proposal.reference.md`
Related: `wiki/proposals/single-flight-drain.proposal.md`, `wiki/decisions/020-observability-surface.decision.md`, `wiki/decisions/021-cache-version-identity.decision.md`, `wiki/decisions/022-durable-trace-context.decision.md`, `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/decisions/014-pull-gating.decision.md`, `wiki/decisions/015-cache-version-persistence.decision.md`

## 0. Summary

`wiki/proposals/single-flight-drain.proposal.md` answered the write side on 2026-08-27 and stands
unchanged. This page answers what the revision added.

| Ask | Answer | Where |
| --- | --- | --- |
| §5.2 — `traceparent` stored on the record | **Accepted**; generation declined | decision 022 |
| §5b.1 — versions are hashes, not counters | **Accepted**, and it is worse than stated | decision 021 |
| §5b.2 — invalidation carries the hash, client skips on match | **Accepted**; falls out of 021 | decision 021 |
| §5b.3 — shared DTO as the hash input | **Agreed, and it is not frontbox's to hold** | decision 023 |
| §5b.4 — row-level skip and a verification pass | **Accepted with the boundary redrawn** | decision 023 |
| §5b.5 — dead letters need a reason and a destination | **Already built; needs documenting** | §4 below |
| §5b.6 — emit `tracing`, application owns the sink | **Accepted** | decision 020 |
| §2 read side — authorize D2 against the new model | **Refused as stated** | §5 below |

The withdrawal of "not asking D2 to change" is the right correction and was made at the right time.
D2's plan is dated 2026-08-27 and unauthorized; the code, however, is **built and shipped**, which
the revision does not appear to know. That changes the price of §5b.1 and is the subject of §1.

## 1. §5b.1 Is Correct, And The Cost Is Higher Than The Question Implies

Q4 asks whether decision 015 assumes cache versions are ordered. **It does, and so does the code that
shipped on 2026-08-27.** Four public types carry a `u64`, and one enum variant exists for no other
purpose:

```rust
if server_version > local.version      { VersionUpdate::Updated }
else if server_version < local.version { VersionUpdate::NeedsReset }
else                                   { VersionUpdate::NoChange }
```
(`src/cache/mod.rs:142-149`)

`NeedsReset` is reachable only through `<`. Under a set hash there is no "behind", so the three-way
comparison collapses to two and a public variant loses its producer — the same shape as `Blocked`
under single-flight, and it should be handled the same way rather than deleted.

**One thing the proposal does not notice, and it is a silent-failure bug.** XOR's identity element is
zero, so the set hash of an empty collection is zero. `EntityState::unknown()` is documented as
"version zero, not stale". Under §5b.1 those become the same stored value: *never synced* and
*legitimately empty* are indistinguishable, and `compare` answers `NoChange` to both. This is the
same class of defect §5b.1 uses to reject `max(updated_at)` — a deleted row that is never
invalidated and stays cached — reappearing inside the replacement. The sentinel has to become
explicit.

The reasons for hashing are good and, unusually, they are not RepForge-specific: any counter behind
an idempotent write path is bumped by replays that change nothing, and structural idempotency is
what frontbox is steering its consumers toward. Decision 021 accepts the change and makes the
version opaque rather than swapping one concrete type for another.

## 2. §5b.4 Is Right, And Decision 017 Makes It More Right

The permanent-data-hole argument is the strongest passage in the revision. It is also incomplete in
frontbox's favour: decision 017 bounds retention and dead-letters at the bound, so a record can now
terminate **without the server ever having refused it**. That opens the same hole for writes nobody
rejected. Triggering verification on the outbox emptying *however it emptied* covers both, which
means the trigger RepForge proposes is more load-bearing than the argument for it.

Two adjustments in decision 023.

**frontbox tracks staleness per row; it does not store rows.** §5b.3's shape — `(seq, id,
entity_type, blob, hash, stale, …indexed columns)` — makes frontbox a read-model store, which D5's
scope excludes in one line. The proposal does not flag this as a scope change and it is the largest
one in the document. The justification §5b.4 actually gives is about the *marker* ("the `stale` flag
must be persisted, or a restart between skip and drain loses the marker silently"), and a marker
table grants that without the blob.

**frontbox reports the trigger; it does not run the verification pass.** Decision 014 settled that
core does not perform the refetch and therefore cannot gate one. A verification pass is a refetch.

## 3. §5b.6 Is Accepted, And It Corrected Us

RepForge's boundary — a library emits, the application owns the subscriber and sink — is accepted in
full. It also caught a real error: decision 020, written on 2026-08-28 before this revision arrived,
argued that emission was *structurally blocked* by frontbox's `!Send` and no-date-library gates.
Both claims were wrong and were checked before withdrawal. `no_send_bound` greps `src/` for `Send`
bounds and a `tracing::info!` adds none; `tracing` pulls `pin-project-lite`, `tracing-core`, and
`once_cell`, none of them a date crate. The clock argument was also wrong: a subscriber stamps at
emit time, in-process, so an offline pass is stamped during the offline pass.

What survives is one limit, and it is the one that makes §5.2 right: **a span cannot model
enqueue-to-send**, because a span is in-process and that interval spans restarts and days. That is
why durable trace context on the record is the correct mechanism and a drain-time span is not.

## 4. §5b.5 Is Already Built

The two causes already map onto a field that exists:

| Cause | `DeadLetterRecord::error` | Meaning |
| --- | --- | --- |
| `Rejected` | `Some(RemoteRejection)` | The server refused, with its own payload |
| Bound reached | `None` | The client gave up; no server refused it |

Decision 017 chose `None` deliberately — "synthesising a refusal would put words in the server's
mouth" — which is exactly the distinction §5b.5 asks for, decided before the ask arrived. What is
owed is documentation, not a field.

On the destination: agreed, and RepForge already draws the line in the right place. frontbox exposes
dead letters through `DeadLetterStore` and the application reports them. The one thing worth adding
is that a parked mutation should carry its `traceparent` (decision 022) and its attempt count
(decision 017), or the report arrives at the owning service with no way to join it to the request
that produced it.

## 5. On Authorizing D2 Against The New Model

Refused as stated, and the reason is sequencing rather than disagreement. D2 is not a plan awaiting
authorization — **it is built, tested, and shipped**, at 96 tests and 95% line coverage. §5b.1 and
§5b.4 are changes to shipped public types, which no decision since D1 has made. That is affordable
only because `publish = false`, and it should be done as a deliberate amendment with its own
conformance work, not folded into an authorization as though the code were still hypothetical.

The revision's §9.2 — "before D2 is authorized, settle 5b.1 and 5b.3" — has the right instinct and
the wrong deadline. The correct one is *before D5*, because that is when the version column acquires
a type in a durable schema.

## 6. Answers To The Eight Questions

**1. Per-scope monotonic sequence in IndexedDB without a second round trip?** Yes, by dropping the
per-scope requirement. Decision 016 makes the sequence *globally* monotonic; reads are scope-filtered
already, so global monotonicity gives per-scope for free and an `autoIncrement` object store
supplies it with no read-modify-write. Answered on 2026-08-27 and unchanged.

**2. Queue depths in the source system?** Unknown, and this project cannot override the estimate.
The corpus has a `pending_count` and no telemetry. If RepForge has production data it beats anything
in `raw/`.

**3. Default or named mode?** Neither. `with_batch_limit(1)` works today, so nothing is blocked and
no mode name is needed. Granted: conformance coverage. Refused: the default, because at limit 1 the
liveness argument depends on decision 017 being switched on, and defaults belong at the safe end.

**4. Does decision 015 assume ordering?** Yes — and so does shipped code. See §1 and decision 021,
including the zero-sentinel collision the question does not anticipate.

**5. Should the hash live in core or the contracts crate?** RepForge's assumption is right: core
defines *where* it is stored and *when* it is compared; the application supplies the function. This
is the same split as decision 007's registry and decision 014's classifier, and decision 021 makes
the stored value opaque so core cannot accidentally depend on its shape.

**6. Does decision 007's registry accommodate URL-segment names, or expect an enum?** It already
accommodates them. `EntityKey` is implemented for `String` and `&'static str` (`src/entity.rs:29-39`),
so `exercises`, `workout-templates`, `workout-series`, `planned-workouts`, and `sessions` work as
they are. An enum is available and not required. Note one existing constraint: names are stored in
exactly one textual form — case-sensitive, no normalization — so the spelling must be stable, which
RepForge says it is.

**7. Is a persisted `stale` flag compatible with how D2 models cache entries?** Partly, and the
difference is the subject of decision 023. D2 persists `stale` per *entity type* already, as half of
decision 015's atomic `(version, stale)` pair. Row-level staleness is a genuine extension. It is
compatible if frontbox stores markers and the application stores rows; it is not compatible with
frontbox holding the blob.

**8. Will frontbox state kafkaman's sink boundary explicitly?** Yes. Decision 020, amended.

## 7. What frontbox Asks In Return

- **Confirm the empty-collection case** against §5b.1's zero-valued hash before either side builds.
  If RepForge's accumulator seeds with something other than zero, §1's collision does not arise and
  decision 021 gets simpler.
- **A structured terminality signal in service error bodies** — still the highest-value cheap thing
  the redesign could add (decision 019), and still unanswered from the 2026-08-27 round.
- **The `contracts` crate's wasm constraints in writing.** §5b.3 says "serde and pure types only",
  which is right; frontbox's experience is that the constraint erodes silently — decision 011 removed
  `chrono` only after a compatibility note exposed it as public surface *by behaviour*.
- **Confirmation that enqueue order is causal order in RepForge's flows.** Decision 016 makes the
  drain faithful to enqueue order; it cannot make enqueue order correct.

## 8. What Would Falsify This Response

- **§1 is wrong if RepForge's set-hash accumulator does not start at zero**, which would remove the
  sentinel collision entirely.
- **Decision 023 is wrong if row markers without rows prove unusable** — if every application ends
  up joining markers to its own store in the same way, the join belongs in the library.
- **§3's acceptance is wrong if `tracing` costs materially on wasm.** Binary size is the plausible
  complaint and it is measurable; nobody has measured it.
- **§5 is wrong if D2's shipped code is more provisional than this response assumes.** It is behind
  no feature flag and is covered by the conformance suite, but the crate is unpublished and the
  distinction between "shipped" and "written" is softer here than the word suggests.

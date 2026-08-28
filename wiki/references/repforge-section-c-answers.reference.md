# RepForge's Answers To Decision Register Section C

Document Class: Reference
Status: Recorded from conversation; not filed in `raw/`
Date: 2026-08-29
Category: External Input
Scope: RepForge's reply to the four questions in the decision register's section C, what it settles, and the three things it opens.
Sources: RepForge architecture, "Answers To frontbox's Decision Register, Section C", received in conversation 2026-08-29.
Related: `wiki/references/open-decisions.reference.md`, `wiki/references/repforge-single-flight-proposal.reference.md`, `wiki/decisions/019-verdict-synthesis.decision.md`, `wiki/decisions/020-observability-surface.decision.md`, `wiki/decisions/021-cache-version-identity.decision.md`, `wiki/decisions/022-durable-trace-context.decision.md`

## Provenance Note

Same weak provenance as the single-flight proposal reference: received in conversation, not filed in
`raw/`, which this project's agent does not write to. Quoted passages are quoted from the message as
received. The RepForge-side paths it cites — `wiki/decisions/read-model-and-invalidation.decision.md`,
`code/contracts/api/src/hash.rs`, and the rest — are **in RepForge's repository, not this one**, and
have not been checked from here. They are recorded as claims with addresses, which is better than
claims without.

## What Was Settled

### C1 — the accumulator seeds at zero

> **Yes. Zero, 32 bytes, and it is executed rather than promised.**

BLAKE3, 256-bit output; a 256-bit XOR accumulator per `(user, entity_type)`; the empty set is all
zeroes; lowercase hex on the wire; the hash input is a canonical byte encoding rather than the JSON
wire bytes. Cited to `code/contracts/api/src/hash.rs:96` (`pub const EMPTY: Self = Self([0u8; 32])`)
and a property test asserting 64 hex zeroes on native and `wasm32-unknown-unknown`.

**Consequence for this project: conformance case 44's scenario is live**, and the simplification
decision 021 would have taken if the seed were elsewhere is not available. RepForge volunteered this
against their own interest — *"We would rather tell you that than have you find it in a test."*

### C2 — the terminality signal now exists

> **They did not. They do now — this question caused the change.**

A required `retry` field on every service-produced error, with three values:

| Value | RepForge's definition |
| --- | --- |
| `terminal` | Refused on the request's merits. It will be refused again unchanged. |
| `transient` | The condition will stop being true without the user changing anything — a missing prerequisite, a dependency that is down, a lock held elsewhere. |
| `conflict` | `If-Match` failed against the current row hash. Terminal for this attempt, resolvable by the user rather than a sync failure. |

Two properties they call out. The status-line table (`5xx`/`429`/`408` retryable, other `4xx`
terminal, `412` conflict) **survives as a documented fallback** for gateway and transport failures
that never reach a service and so carry no envelope. And an unrecognised `retry` value **degrades to
terminal and round-trips verbatim** — explicitly modelled on decision 012's handling of an unknown
`MutationStatus`.

They credit decision 019 for the shape being three values rather than a boolean, and name the
obligation a status line cannot discharge: *"A `404` for a resource that will never exist and a
`404` for a parent that has not drained yet are the same code and the same status with opposite
correct behaviour."*

### C3 — the convergence, in three parts

**Settled before the question:** the target is OpenTelemetry, one OTLP collector, Elasticsearch for
all three signals. Push-versus-return is answered by the sink boundary decision 020 already accepted
— *"kafkaman neither pushes nor returns — it emits into a subscriber we installed."* Client
telemetry rides HTTP to `POST /api/v1/{service}/telemetry/dead-letters` rather than OTLP, because
the collector is never publicly exposed.

**Decided in response:** `mutation_id` maps to a **span attribute**, not a trace id and not baggage,
on the drain span and the dead-letter span event. And **clock authority**: the client stamps
`occurred_at`, the service stamps `received_at`, the service's clock is authoritative for ordering,
and both travel. Their note that `received_at - occurred_at` is a free skew measurement — stable
across a device's reports means a wrong clock, variable means offline windows — is worth keeping.

**They also correct their own page**: it had recorded `traceparent`-on-the-record as an open D5 ask.
They accept decision 022, including the half that declines core generating it: *"RepForge generates
the `traceparent` at enqueue."*

### C4 — there are no production queue depths

> **We cannot supply these. There is no production and there never has been.**

No deployment, no retained metrics, no dashboards. The corpus's `pending_count` is an `AtomicUsize`
driving a UI badge, not a series. They note their own pages already label ~20 s an estimate and list
measured depths under what would revisit the decision, so nothing is walked back.

They offer a modelled worst case (labelled as a model) or a commitment to instrument once the
collector exists, and state their position plainly: *"~20 s is an unvalidated estimate neither side
should design against."*

## What It Opens

Three items, recorded here and carried into the register.

- **Zero versus never-computed, on their side.** `SetHash` derives `Default`, so
  `SetHash::default() == SetHash::EMPTY` and `is_empty()` cannot distinguish a legitimately empty
  collection from an accumulator nobody has computed. They flag it as open and say it *"touches your
  side as much as ours"*. It does not — see the register's response.
- **The shared event vocabulary is blocked on kafkaman**, who *"has not responded to that proposal
  at all."* They offer a unilateral strawman if a blank is worse.
- **The dead-letter report body is unspecified.** Only the endpoint path exists. They invite input
  and make this project's own argument back at it: *"now is free and later is not — which is the
  same argument your register makes about your own D5 columns."*

## Two Questions Back

1. **Where does ~17 min come from?** Answered in the register; the short version is that it is
   cadence-bound, not RTT-bound, and the register's phrasing invited the misreading.
2. **RepForge asked frontbox the same queue-depth question** (proposal §10 Q2). Neither party has
   the number. Both documents now point at each other, which is worth both sides knowing so it stops
   costing round trips.

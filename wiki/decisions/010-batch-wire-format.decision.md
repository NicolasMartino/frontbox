# The Batch Wire Format Stays Compatible With The Source Server

Document Class: Decision
Status: Accepted; amended 2026-08-27 by decision 011; premise questioned 2026-08-27 by decision 019
Date: 2026-08-26
Category: Public API Shape
Scope: What `MutationBatchRequest` and `MutationBatchResponse` serialize to, and which serialization crates that pins into the public compatibility surface.
Sources: `raw/initial/2026-08-25T083750Z/sources/frontend/dto.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/types.rs`, `src/record/mod.rs`, `src/protocol.rs`
Related: `wiki/decisions/011-owned-rfc3339-rendering.decision.md`, `wiki/decisions/019-verdict-synthesis.decision.md`, `wiki/proposals/single-flight-drain.proposal.md`, `wiki/decisions/012-unknown-mutation-status.decision.md`, `wiki/compatibility/public-dependencies.compat.md`, `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/decisions/002-error-model.decision.md`, `wiki/decisions/008-mutation-envelope-extensibility.decision.md`, `wiki/proposals/extraction-boundary.proposal.md`

## Premise Note

**Added 2026-08-27.** RepForge is removing the BFF this decision is compatible *with*
(`wiki/references/repforge-single-flight-proposal.reference.md`). Writes become per-resource `PUT`
behind a gateway that only routes, so the endpoint `MutationBatchRequest` targets will not exist and
passthrough compatibility would make D4 a translation layer in the opposite direction — the cost
this decision was written to avoid.

Nothing here is rewritten yet, deliberately: D4 is the deliverable that answers it with evidence,
and RepForge asked for exactly that restraint. Two things are worth recording in the meantime. The
envelope *fields* are not in question — `mutation_id`, `method`, `path`, `client_datetime`, `body`
is the right record shape and a single-mutation request is a one-element batch on the wire. And the
expiry reaches further than the payload: decision 019 observes that
`SyncTransport::send_batch` returning a *server-produced* `MutationBatchResponse` stops describing
anything real once the *target* server does not produce one — a statement about RepForge's redesign,
not about every server frontbox might meet. See `wiki/proposals/single-flight-drain.proposal.md` §4.

## Decision

Core's batch protocol types serialize to the payload the source server already accepts. A transport
for that server is a passthrough, not a mapping layer.

Concretely:

- `MutationIntent` serializes with the five keys `MutationIntentDto` defines, in the same order:
  `mutation_id`, `method`, `path`, `client_datetime`, `body` (`frontend/dto.rs:96-108`).
- `client_datetime` is an RFC 3339 instant. **Amended 2026-08-27:** the bytes are unchanged, but
  they are now produced by `src/rfc3339.rs` and *compared* against chrono's serializer by an oracle
  test, rather than produced by delegating to it. "Match rather than merely resemble" is now a
  test that runs on every build instead of a dependency edge. See decision 011.
- `op` (decision 008) is `#[serde(default, skip_serializing_if = "Option::is_none")]`. A caller who
  does not use it produces a payload with no new key, so a server that rejects unknown fields sees
  nothing until the caller opts in.
- `RemoteRejection` deserializes the source's `ApiError { code, message }` unchanged. `code` is
  optional and `details` is added, both `#[serde(default)]`.

Internally, time stays a single `i64` of epoch milliseconds — the representation `Clock::now_ms`
produces and the records store. `client_datetime` is a serialization concern only.

## Why

The alternative was a neutral payload with `created_at` as an integer, pushing the RFC 3339
conversion into each application's transport. That is cleaner in isolation and was the initial
recommendation, but it charges the D4 migration trial for a translation layer that exists only to
avoid a dependency — and the migration trial is the deliverable that validates whether the extracted
API is right at all. Making its transport a passthrough removes a source of noise from the one
experiment the roadmap relies on.

Two properties keep the cost contained.

**One internal time representation.** Storing `DateTime<Utc>` on the record would give timestamps
two forms and two conversion points. Keeping `i64` everywhere and converting once, at the
serialization boundary, means `Clock::now_ms` is the only thing that produces a time value.

**`chrono` without its `clock` feature.** The dependency is declared
`default-features = false, features = ["serde", "alloc"]`. `Utc::now()` therefore does not compile
anywhere in the crate. This turns decision 002's "timestamps come from an injected clock" from a
convention into a build error, which is stronger than the rule it replaces — the source's habit of
calling `Utc::now()` inside a record constructor (`persistence/types.rs:88`) is not merely
discouraged, it is unavailable. It also keeps `js-sys` and `iana-time-zone` out of the wasm build.

**Amended 2026-08-27.** Both properties survive decision 011 and the second is stronger for it.
chrono is absent from `src/` entirely, and the dev-dependency that remains is still declared
`default-features = false`, so `Utc::now()` does not compile in tests either — the rule now covers
ground this paragraph did not reach. The wasm build gains the rest of chrono's absence on top.

## Consequences

- ~~**`serde_json` *and* `chrono` are public compatibility surface.**~~ True as written, and it was
  the reason this decision was revisited. **Superseded 2026-08-27 by decision 011:** chrono is now a
  dev-dependency and is no longer in the semver contract. `serde_json` remains, joined by `uuid` and
  `serde` — see the compatibility note. The general point stands and is what found the others: a
  crate can be public surface *by behaviour* without appearing in a single signature.
- A timestamp the wire format cannot express is treated as a corrupt record and quarantined rather
  than surfacing as a serialization failure mid-batch, consistent with decision 006. Conformance
  case 26 covers it. **Amended 2026-08-27:** the bound is now RFC 3339's four-digit year rather than
  chrono's much wider range, which *narrows* what serializes — chrono rendered years past 9999 in
  ISO 8601 expanded form, which the grammar does not admit. See decision 011.
- The wire format is now pinned by tests rather than by intent: case 27 asserts the exact key set
  and the exact `client_datetime` string, and `tests/dto_oracle.rs` ports the round-trip tests
  from `frontend/dto.rs`.
- A future non-RepForge server that wants a different shape is served by its own transport doing the
  mapping, which is where such a mapping belongs. Nothing about this decision forecloses that.
- **This decision pinned the payload to five statuses and left unknown ones unhandled**, which the
  `# Evolution` note on `MutationStatus` records: a status string this crate has never heard of
  fails the whole response. **Settled 2026-08-27 by decision 012** — tolerated, retained with its
  original spelling, and reported as an anomaly. Implementing it is a public-API change and needs
  its own go-ahead, so the limitation described here is still the shipped behaviour.

## Release Follow-Up

This decision creates obligations that come due before any published release, not at D5. They are
listed here because the natural time to forget them is between now and then.

- ~~**A `wiki/compatibility/` note naming `serde_json` and `chrono` as public dependencies**~~,
  with the major versions this crate's semver contract is tied to. A caller pinned to a different
  major of either cannot use frontbox's types at all, which makes this a compatibility fact rather
  than an implementation detail. **Done 2026-08-27:**
  `wiki/compatibility/public-dependencies.compat.md`.

  Writing it found the list incomplete. `uuid` is public surface too — `MutationId::from_uuid` and
  `MutationId::as_uuid` take and return `uuid::Uuid`, and `<MutationId as FromStr>::Err` is
  `uuid::Error` — and so is `serde`, since implementing a transport means serializing the protocol
  types. Neither was named here because both arrived through decisions 008 and 009 rather than this
  one, which is how a dependency becomes public without any single decision noticing.
- **A changelog entry the first time either major version moves**, since that is a breaking change
  for every consumer regardless of what frontbox's own API does.
- **A statement of which server payload version the wire format targets.** Today it is "whatever
  RepForge's `MutationIntentDto` was on 2026-08-25". A published crate needs that pinned to
  something a reader can check, because a server contract that drifts silently breaks replay for
  queued work rather than at compile time. **Recorded 2026-08-27** in the compatibility note above,
  still as a date — naming a checkable version is a D4 question, since that is when a second
  consumer of the payload first exists.

None of these blocks D2. All of them block a release.

## Revisit If

The D4 migration trial ships and no second consumer wants the source payload. **Amended
2026-08-27:** this originally bundled two questions — the payload shape and the chrono dependency —
and decision 011 has settled the second independently. What remains is only whether the payload
should be a neutral integer `created_at`, and that question no longer has a dependency cost on
either side of it. The change is still breaking, so the window still closes at the first stable
release.

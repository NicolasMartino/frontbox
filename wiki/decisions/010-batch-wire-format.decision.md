# The Batch Wire Format Stays Compatible With The Source Server

Document Class: Decision
Status: Accepted
Date: 2026-08-26
Category: Public API Shape
Scope: What `MutationBatchRequest` and `MutationBatchResponse` serialize to, and which serialization crates that pins into the public compatibility surface.
Sources: `raw/initial/2026-08-25T083750Z/sources/frontend/dto.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/types.rs`, `src/record.rs`, `src/protocol.rs`
Related: `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/decisions/002-error-model.decision.md`, `wiki/decisions/008-mutation-envelope-extensibility.decision.md`, `wiki/proposals/extraction-boundary.proposal.md`

## Decision

Core's batch protocol types serialize to the payload the source server already accepts. A transport
for that server is a passthrough, not a mapping layer.

Concretely:

- `MutationIntent` serializes with the five keys `MutationIntentDto` defines, in the same order:
  `mutation_id`, `method`, `path`, `client_datetime`, `body` (`frontend/dto.rs:96-108`).
- `client_datetime` is an RFC 3339 instant, produced by delegating to `chrono`'s own
  `DateTime<Utc>` serializer rather than formatting by hand, so the bytes match rather than merely
  resemble.
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

## Consequences

- **`serde_json` *and* `chrono` are public compatibility surface.** Their major versions are part of
  this crate's semver contract and belong in `wiki/compatibility/` before any release. Decision 008
  already flagged this for `serde_json`; `chrono` joins it.
- A timestamp outside chrono's representable range cannot be serialized. Rather than surfacing as a
  serialization failure mid-batch, a durable row carrying one is treated as a corrupt record and
  quarantined, consistent with decision 006. Conformance case 26 covers it.
- The wire format is now pinned by tests rather than by intent: case 27 asserts the exact key set
  and the exact `client_datetime` string, and `tests/source_oracle.rs` ports the round-trip tests
  from `frontend/dto.rs`.
- A future non-RepForge server that wants a different shape is served by its own transport doing the
  mapping, which is where such a mapping belongs. Nothing about this decision forecloses that.

## Revisit If

The D4 migration trial ships and no second consumer wants the source payload. At that point the
question becomes whether to keep the `chrono` dependency for one caller, and the answer would be a
neutral payload plus a compatibility transport. That change is breaking, so the window closes at the
first stable release.

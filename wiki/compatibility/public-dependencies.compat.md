# Public Dependencies And Wire-Format Target

Document Class: Compatibility Note
Status: Active
Date: 2026-08-27 (chrono removed the same day, decision 011)
Category: Public API Shape
Scope: Which third-party crates appear in frontbox's public API and therefore in its semver contract, and which server payload the wire format targets.
Sources: `Cargo.toml`, `src/id.rs`, `src/record.rs`, `src/protocol.rs`, `src/error.rs`
Related: `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/decisions/011-owned-rfc3339-rendering.decision.md`, `wiki/decisions/008-mutation-envelope-extensibility.decision.md`, `wiki/decisions/002-error-model.decision.md`, `wiki/specs/frontbox-runtime.spec.md`

## Why This Page Exists

Decision 010 lists three obligations that come due before any published release. This page discharges
the first and the third. Nothing is published yet — the crate is `0.0.0` with `publish = false` — so
this is a record of what the contract *will* be, written while the reasons are still in view.

A public dependency is one whose types a caller cannot avoid naming. Bumping its major version is a
breaking change for every consumer regardless of what frontbox's own API does, because two majors of
the same crate are two unrelated types to the compiler: a caller pinned to `uuid 2` cannot pass a
`Uuid` to a frontbox function expecting `uuid 1`'s.

## The Public Dependencies

| Crate | Major | Where it surfaces | Avoidable by a caller? |
| --- | --- | --- | --- |
| `serde_json` | `1` | `MutationIntent::body` and `OutboxRecord::body` are `serde_json::Value`; `RemoteRejection::details` is `Option<Value>`; `Error::Serialization` carries `serde_json::Error` | No — every enqueue names a `Value` |
| `uuid` | `1` | `MutationId::from_uuid` and `MutationId::as_uuid` take and return `uuid::Uuid`; `<MutationId as FromStr>::Err` is `uuid::Error` | Only with the `v4` feature, which supplies `MutationId::new()` and `FromStr` |
| `serde` | `1` | `Serialize`/`Deserialize` are implemented on every protocol and record type | No — implementing a transport means serializing them |

All three are major `1`. The full runtime dependency graph is `serde`, `serde_json`, `thiserror`,
and `uuid`; `scripts/verify.sh` asserts no date library is anywhere in it.

`thiserror` is **not** public surface. It is a derive macro that generates ordinary
`std::error::Error` impls; nothing in the API mentions it, and its major version can move freely.

### `chrono` was on this list for one day

The first version of this page carried a fourth row. `chrono` appeared in no signature, which made
it easy to mistake for an implementation detail — and it was not one: decision 010 committed
`client_datetime` to whatever chrono's `DateTime<Utc>` serializer emitted, so a chrono release that
changed its RFC 3339 rendering would have changed frontbox's wire format without changing a line of
frontbox. Worse, `0.4` is pre-1.0, so by Cargo's rules the *minor* position carried the breaking
changes.

Writing that paragraph is what prompted the question of whether a more stable crate could hold the
same compatibility. None exists — `time` is `0.3` and `jiff` is `0.2`, so every candidate breaks on
minor. Decision 011 took the other exit: `src/rfc3339.rs` renders the format, chrono stays as a
dev-dependency oracle proving the bytes still match, and the row came off this table.

The lesson worth keeping is the one that found `uuid` and `serde` below: **a crate can be public
compatibility surface by behaviour without appearing in a single type signature.** Reading the API
will not reveal it. Asking "what would break for a caller if this crate's major moved?" will.

### The `v4` feature does not change the contract

`v4` adds `MutationId::new()` and a randomness backend. It does not add or remove a type from the
public API — `uuid::Uuid` is already there through `from_uuid` and `as_uuid`, feature or no feature.
Building `--no-default-features` removes the randomness dependency, not the compatibility surface.

## Wire-Format Target

The batch payload targets **RepForge's `MutationIntentDto` as of 2026-08-25**, copied at
`raw/initial/2026-08-25T083750Z/sources/frontend/dto.rs`.

That is a snapshot of one application's server, not a versioned public contract, which is exactly
the gap decision 010 flags. A published crate needs this pinned to something a reader can check,
because a server contract that drifts silently breaks *replay of already-queued work* — a mutation
written last week against the old shape, sent today against the new one. There is no compile step in
between to catch it.

The five keys, in order: `mutation_id`, `method`, `path`, `client_datetime`, `body`. Plus `op` when,
and only when, the caller set it — `skip_serializing_if = "Option::is_none"`, so a server that
rejects unknown fields sees nothing new until the caller opts in.

What holds this true: conformance case 27 asserts the exact key set and the exact `client_datetime`
string, and `tests/dto_oracle.rs` ports the round-trip tests from the source's own `dto.rs`.

## Still Outstanding Before Release

From decision 010's `## Release Follow-Up`, the item this page does not discharge:

- **A changelog entry the first time any major version above moves.** There is no changelog yet
  because there is no release yet.

And one this page adds:

- **Name a checkable server-contract version.** "RepForge on 2026-08-25" is a date, not something a
  reader can verify against a running server. The D4 migration trial is the natural point to settle
  what it should be, since that is when a second consumer of the payload first exists.

## Revisit If

Decision 010's own revisit condition fires — the D4 migration trial ships and no second consumer
wants the source payload. That question is now narrower than it was: decision 011 already removed
the dependency cost, so what remains is only whether a neutral integer `created_at` is a better
payload than an RFC 3339 string. It would remove no row from the table above. It is still a breaking
change, so the window still closes at the first stable release.

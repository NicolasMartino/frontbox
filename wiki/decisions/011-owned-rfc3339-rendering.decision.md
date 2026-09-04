# The Wire Format's Timestamp Rendering Is Owned, With chrono As A Test Oracle

Document Class: Decision
Status: Accepted
Date: 2026-08-27
Category: Public API Shape
Scope: Which crate produces the `client_datetime` bytes, what range the wire format admits, and what grammar it accepts on the way back in.
Sources: `src/rfc3339.rs`, `src/rfc3339/tests.rs`, `src/record/mod.rs`, `src/memory/mod.rs`, `Cargo.toml`, `scripts/verify.sh`
Related: `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/decisions/002-error-model.decision.md`, `wiki/decisions/006-corrupt-record-policy.decision.md`, `wiki/compatibility/public-dependencies.compat.md`, `wiki/specs/frontbox-runtime.spec.md`

## Decision

`client_datetime` is rendered and parsed by this crate, in the private module `src/rfc3339.rs`.
`chrono` moves to `[dev-dependencies]` and appears in exactly one place: `src/rfc3339/tests.rs`,
where it is the oracle the rendering is compared against.

Decision 010 is otherwise unchanged. The bytes on the wire are the same bytes, byte for byte, and
that claim is now *asserted on every build* rather than inherited from a dependency edge.

Three things are settled alongside it:

- **The representable range is RFC 3339's, not a library's.** `0000-01-01T00:00:00.000Z` through
  `9999-12-31T23:59:59.999Z`, which is `-62_167_219_200_000..=253_402_300_799_999` in epoch
  milliseconds. Outside it, `format` returns `None` and the record is quarantined (decision 006).
- **The emitted shape is fixed.** No fractional part when the millisecond is zero; exactly three
  zero-padded digits when it is not. `…:20Z` and `…:20.100Z`, never `…:20.000Z` or `…:20.1Z`.
- **The accepted grammar is written down** on `rfc3339::parse` rather than being whatever the
  dependency happened to tolerate. It is deliberately stricter than chrono in three named ways.

## Why

The question that prompted this was whether a *more mature* crate could hold RFC 3339 compatibility
instead. Checking the candidates is what changed the answer:

| Crate | Version |
| --- | --- |
| `chrono` | `0.4.x` |
| `time` | `0.3.55` |
| `jiff` | `0.2.35` |

All three are `0.x`, so by Cargo's rules the minor position carries breaking changes for all three.
Swapping one for another trades a breaking-on-minor public dependency for a breaking-on-minor public
dependency and leaves the compatibility problem exactly where decision 010 found it. By the maturity
axis specifically, `jiff` is the youngest of the three — the best-designed, but the newest, which
argues against it rather than for it.

It is also worth being fair to chrono. Its reputational problems — the `localtime_r` soundness
advisory, timezone database handling, DST — all live in the `clock` feature, which decision 010
already excluded. What remained in use was `from_timestamp_millis`, `timestamp_millis`, and an
RFC 3339 serde impl, across two call sites. Nothing was wrong with it.

The actual problem decision 010 identified was subtler and unfixed by any swap: **chrono was public
compatibility surface by behaviour rather than by type.** It appeared in no signature. It could not
be seen by reading the API. But because the payload contract *is* its rendering, a chrono release
that changed that rendering would have changed frontbox's wire format without changing a line of
frontbox — a breaking change to this crate, originating outside it, invisible at compile time.

That is not a dependency you can hold at arm's length. It is either yours or it is a liability.

**The format is one fixed shape, so owning it is small.** Always UTC, always `Z`, always
milliseconds, four-digit years. No timezones, no DST, no calendrical arithmetic, and no leap seconds
— Unix time does not have them. What is left is Howard Hinnant's `civil_from_days` /
`days_from_civil` pair and some formatting: about 130 lines of implementation.

**The oracle is what makes it safe, and it is the whole argument.** "Don't roll your own date
handling" is sound advice, and it is sound because the failure mode is a subtle divergence nobody
notices until data is wrong. Keeping chrono as a dev-dependency answers that directly: the divergence
is what the test looks for. Decision 010 bought "match rather than merely resemble" by delegating;
this buys the same property by comparison, and a comparison is checkable in a way an argument from
delegation is not.

The oracle earned its place immediately. The obvious implementation — always emit three fractional
digits — is wrong: chrono omits the fraction entirely when it is zero, so `2023-11-14T22:13:20Z` and
`2023-11-14T22:13:20.100Z` are both correct and `2023-11-14T22:13:20.000Z` is not. That would have
round-tripped perfectly and sent the server bytes it had never seen. It was caught by probing chrono
before writing a line, and it is now pinned by
`matches_chrono_for_every_millisecond_within_a_second`.

**The timing is the last of it.** The crate is `0.0.0`, `publish = false`, no consumers, no data in
the field. Removing a public dependency is breaking; doing it now costs nothing and doing it after a
release costs a major version.

## Consequences

- **`chrono` leaves the compatibility surface.** The public dependencies are now `serde_json`,
  `uuid`, and `serde`, all at major `1`. The runtime dependency graph is `serde`, `serde_json`,
  `thiserror`, `uuid` — no date library at any depth.
- **The representable range narrows, and that is a fix.** chrono's `DateTime<Utc>` spans roughly
  `-262143` to `+262142`, and outside four-digit years it renders ISO 8601 *expanded* form:
  `+10000-01-01T00:00:00Z`, with a leading sign that RFC 3339's grammar has no production for. Those
  timestamps used to serialize into a payload the server would refuse. They now fail
  `is_representable` and quarantine locally, where the record is readable and recoverable rather
  than lost to a remote rejection. Conformance case 26 covers the path and is unchanged — its
  fixture is `i64::MAX`, unrepresentable under either rule.
- **The parse grammar is now specified rather than inherited**, and is stricter than chrono in three
  places, each documented on `rfc3339::parse`: a space in place of `T` is refused (RFC 3339 allows
  it only "by mutual agreement", which we do not have); a leap second `:60` is refused (chrono
  accepts it and silently moves the value onto the following second); and expanded-year forms are
  refused. Sub-millisecond digits are still accepted and truncated, since milliseconds are the only
  precision `Clock` produces.
- **`Utc::now()` still does not compile anywhere**, which was decision 010's second contained cost
  and is now stronger. chrono is absent from `src/` entirely, and the dev-dependency keeps
  `default-features = false`, so the `clock` feature is unavailable in tests too. Decision 002's
  injected-clock rule remains a build error rather than a convention.
- **A new secondary gate in `scripts/verify.sh`** asks `cargo tree --edges normal` whether any date
  crate is in the runtime graph. Asked of the resolved graph rather than of `Cargo.toml`, because
  the risk this guards against is a date crate arriving transitively, which reading our own manifest
  would never show.
- **Decision 010's revisit clause changes meaning.** It contemplated dropping chrono *and* the
  format together, for a neutral integer `created_at`. The dependency half is now done
  independently, so what remains open at D4 is only the payload-shape question — and that question
  no longer carries a dependency cost as an argument for either side.

## What Holds This True

`src/rfc3339/tests.rs`, eleven tests, all comparing against chrono:

- 200,000 deterministically sampled instants across the whole range, plus every day boundary of
  years 0, 1, 1899, 1900, 1970, 1972, 2000, 2024, 2100, and 9999 at both midnight and the last
  millisecond — 1900 and 2000 because the leap-century rule is where a hand-written calendar goes
  wrong, and 0 and 9999 because the era arithmetic is furthest from its shifted epoch there.
- Every millisecond within a second at four different bases, which is what pins the fraction shape.
- A round-trip over 100,000 samples, so `parse` accepts everything `format` emits.
- The accepted and refused grammars, spelled out case by case.

The oracle was checked by breaking the implementation on purpose: always-three-fraction-digits, and
truncating division in place of Euclidean. Each mutation was caught by four separate tests. A test
that cannot fail is not evidence, so this is recorded rather than assumed.

## Revisit If

A server appears that needs a timestamp form this module does not emit — sub-millisecond precision,
a non-`Z` offset, or a year outside four digits. The answer then is a mapping in that server's own
transport, which is where decision 010 already said such mappings belong. Bringing a date crate back
into the runtime graph to serve one caller would restore exactly the compatibility surface this
decision removed.

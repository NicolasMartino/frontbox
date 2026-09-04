# Storage Naming Is The Backend's Choice And Core's Obligation

Document Class: Decision
Status: Accepted 2026-08-29; case 49 landed the same day; discharged by both backends 2026-08-30
Date: 2026-08-29
Category: Storage Contract
Scope: How a durable backend turns a `ScopeKey` into a physical storage name, who decides, and how the requirement stops being advice.
Sources: `src/scope.rs`, `src/record/mod.rs`, `src/testing/mod.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/native.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/web.rs`
Related: `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/decisions/025-quarantine-storage-shape.decision.md`, `wiki/decisions/006-corrupt-record-policy.decision.md`, `wiki/references/open-decisions.reference.md`, `wiki/roadmaps/extraction.roadmap.md`

## Decision

**Core supplies no encoder, states one obligation, and proves it with a conformance case.**

Decision 009 required that *"backends must derive physical storage identity injectively"* and left
the mechanism open. The mechanism stays open — it belongs to the backend — but the obligation stops
being prose:

- **The obligation is injectivity.** Two distinct `ScopeKey` values must never reach the same
  physical store. Nothing else about the name is core's business: not readability, not reversibility,
  not length, not stability across versions.
- **Core supplies no encoding function**, and no hash dependency enters the runtime graph.
- **A conformance case proves it.** Two scope keys that differ only in characters a naive sanitizer
  would collapse must not see each other's records. This case does not exist today.
- **A non-normative recommendation** is recorded below, because "choose an injective encoding" is
  true and unhelpful.

## Why Core Stays Out

**Structurally, core never names a store.** `StoreFactory::open(scope) -> Store` hands the backend a
`ScopeKey` and gets a store back; nothing in `src/` ever constructs a file path or a database name.
Putting an encoder in core would mean core owning a decision it cannot use, cannot test against real
storage, and cannot make correctly for every backend at once — a SQLite file name and an IndexedDB
database name have different legal alphabets and different length limits.

**And it would cost a dependency for nothing.** A hash-based encoder needs a hash. The runtime graph
is `serde`, `serde_json`, `thiserror`, `uuid`, and the crate has spent real effort keeping it that
short — decision 011 removed `chrono` by owning its rendering rather than by depending on it. Adding
`sha2` or `blake3` to core so that a backend can name a file is the wrong trade, particularly when
the backend already depends on a storage engine that ships hashing.

## Why The Obligation Is Not Optional

The source sanitizes by character replacement (`persistence/native.rs:65`, `persistence/web.rs:33`).
That was safe for the UUIDs it was written for and is unsafe for anything else:

```
tenant/1  ->  tenant_1
tenant_1  ->  tenant_1
```

Two scopes, one database, **silently**. Decision 009 made `ScopeKey` a caller-composed opaque string
precisely so an application can encode a principal, a tenant, and a schema version into it — which
means arbitrary characters are the expected case, not an edge one. The failure has no symptom until
one user sees another's queued writes.

**And the suite did not catch it.** *(Corrected 2026-08-29: case 49 landed the day this decision
was accepted, so the gap below is closed. The paragraph is kept because it is the argument that
produced the case, and paraphrasing it would make the case look like padding.)* Every scope key
in `src/testing/cases/` was clean:
`user:alice`, `user:bob`, `user:a`, `user:alice@tenant:acme@schema:1`. Not one of them collides under
naive sanitization, so a backend could ship the source's exact bug and pass all 44 cases. That is the
gap this decision closes, and it is why the case matters more than the recommendation.

## The Recommendation, Which Is Not The Decision

If a backend has no reason to prefer otherwise: **hash the key, and prefix it with a sanitized,
truncated, explicitly non-authoritative fragment for humans.**

**Reversibility is not needed, and this is the part that is easy to get wrong by over-engineering.**
A reversible encoding looks obviously better — you can read a scope back out of a file name. But
decision 009 already stamps `scope` on every record (`OutboxRecord::scope`), so the mapping from a
store back to its scope is recoverable by reading any row in it. Nothing needs to parse the name.
That removes reversible encoding's only real advantage.

**What it does not remove is length.** A caller-composed key is arbitrary, and a typical filesystem
caps a path component at 255 bytes. Percent-encoding a key that embeds an email address, a tenant
identifier, and a schema version can exceed it, and the failure arrives at `open()` on a real
device rather than in a test. A hash is fixed-width by construction.

The readable prefix costs nothing and buys operability: an engineer looking at a directory of SQLite
files can tell which tenant a store belongs to without opening it, while identity remains the hash.
The prefix must never be consulted for identity, or the collision comes straight back.

## Consequences

- **A conformance case is owed**, and it is the whole enforcement mechanism. Shape: open two stores
  on keys that differ only in a character a naive sanitizer would collapse — `user:a/b@tenant:acme`
  and `user:a_b@tenant:acme` — enqueue under one, and assert the other sees nothing, in both
  directions. The in-memory backend passes it trivially, which is fine: the case exists for the
  durable backends, exactly as the scope-isolation cases do.
- **The case list gains its first entry that no current backend can fail.** Worth saying, because a
  case that passes everywhere on the day it lands looks like padding and is not — decision 009's
  obligation has been unproven since D1.
- **D5's plan specifies the obligation and the case, not the encoding.** Each backend documents what
  it chose in its own compatibility note.
- **The recommendation is not gated.** A backend that hashes without a prefix, or uses a lookup table
  keyed by an integer, or stores every scope in one database behind a scope column, satisfies this
  decision. Only a non-injective derivation does not.

## Implementation Outcome

**Case 49 landed 2026-08-29**, in the shape this decision specified: `user:a/b@tenant:acme` and
`user:a_b@tenant:acme` — two keys that differ only in a character a sanitizer folds — enqueued
under each and asserted invisible to the other, in both directions.

It passes on the in-memory backend, whose keys are map keys and need no encoding. That was expected
and stated in advance. What matters is that it is now waiting for D5's backends rather than being
written after the first collision, and that decision 009's obligation is no longer unproven prose.



A backend appears where injectivity cannot be achieved locally — a storage engine with a short,
case-insensitive, alphabet-restricted name space where even a hash must be truncated far enough to
make collisions plausible. The remedy is a persisted scope-to-name mapping owned by that backend,
which is a legitimate implementation of this obligation rather than an exception to it, and is worth
naming here so the first backend to need it does not think it is cheating.

## Outcome: Neither Backend Needed An Encoder

Both took the option this page permitted and did not recommend: **a `scope` column, with every read
filtered on it.** SQLite stores `scope TEXT NOT NULL COLLATE BINARY`; IndexedDB stores a `scope`
property with an index on it.

That satisfies injectivity trivially — two different keys are two different strings, and nothing
is transformed — and it keeps the conformance suite's sharing contract, which needs two scopes to
live in *one* physical store or the isolation cases prove nothing. The hash-with-readable-prefix
scheme this page recommended would have satisfied injectivity too and bought nothing else, since
decision 009 already stamps the scope on every record and reversibility was never required.

Worth recording as a small vindication of the shape rather than the recommendation: the page stated
one obligation and left the mechanism open, and the mechanism both implementors chose was not the
one it had in mind.

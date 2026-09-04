# One Core Error Type, No Per-Trait Associated Errors

Document Class: Decision
Status: Accepted
Date: 2026-08-25
Category: API Shape
Scope: How errors are represented across store and transport traits, and how the sync runner classifies them.
Sources: `raw/initial/2026-08-25T083750Z/sources/persistence/types.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/mutations.rs`
Related: `wiki/decisions/001-single-threaded-core.decision.md`, `wiki/decisions/004-transport-auth-and-offline.decision.md`, `wiki/decisions/006-corrupt-record-policy.decision.md`

## Decision

Core owns one non-exhaustive error type. Traits return `Result<T, Error>` directly rather than
declaring a per-trait associated `type Error`.

The exact implementation can still move during D1, but the shape is:

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("offline")]
    Offline,

    #[error("transport failure")]
    Transport {
        #[source]
        source: Option<Box<dyn std::error::Error + 'static>>,
    },

    #[error("storage failure")]
    Storage {
        #[source]
        source: Option<Box<dyn std::error::Error + 'static>>,
    },

    #[error("serialization failure")]
    Serialization {
        #[source]
        source: serde_json::Error,
    },

    #[error("corrupt local record")]
    CorruptRecord {
        id: Option<MutationId>,
        reason: String,
    },

    #[error("protocol violation: {reason}")]
    Protocol {
        reason: String,
    },
}
```

Backend adapters may wrap platform-specific errors before boxing them. Core control flow must not
depend on string matching.

No variant carries `#[from]`. An earlier draft had `Serialization(#[from] serde_json::Error)`, which
makes `?` silently convert any serde failure into a core error at whatever call site it occurs. That
is the wrong default here: a serde failure while decoding a durable row is a corrupt local record
(`CorruptRecord`), while the same failure while decoding a server response is a protocol violation
(`Protocol`). Requiring an explicit conversion forces the caller to say which one it meant.

## Why

The sync runner must decide whether a failure means:

- stay queued without backoff (`Offline`),
- stay queued with retry/backoff (`Transport`),
- surface a storage failure,
- quarantine malformed local data,
- or reject an impossible server response.

With per-trait associated errors, the runner cannot make that decision from an opaque `S::Error`
without adding a separate classification trait to every boundary. That recreates a core error
model with more generic noise.

The source's storage error model is the anti-pattern to avoid:
`pub type StoreError = String` (`persistence/types.rs:12`). String-only errors make it hard to
distinguish operational failures from corrupt local data. The source works around part of this by
tracking offline status separately (`persistence/mutations.rs:61,334-345`) instead of carrying the
classification in the error value.

## Consequences

- Backends convert `rusqlite`, `rexie`, serialization, and transport errors at the boundary.
- Boxed sources are `dyn Error + 'static`, not `dyn Error + Send + Sync + 'static`, per
  `wiki/decisions/001-single-threaded-core.decision.md`.
- The enum starts `#[non_exhaustive]`, so future variants can be added without forcing an immediate
  major version.
- Corrupt local data is first-class and routes into the quarantine policy, rather than vanishing
  through `filter_map(...ok())`.
- Remote mutation rejections are not this error type. They are server verdict payloads attached to
  mutation results and dead-letter records.

## Revisit If

Implementation discovers a platform failure type that cannot be represented as a boxed source or
as a clearly typed core variant. That should produce a new variant, not a fallback to string
classification.

## What The Backends Added

**`Error::storage_message` exists because IndexedDB asked for it.** The original set carried an
opaque storage variant, on the argument that a caller cannot act on a backend's internal failure.
That held for SQLite and stopped holding in a browser: `QuotaExceededError` and
`TransactionInactiveError` are two DOM exceptions an operator genuinely has to tell apart, and
collapsing both into "storage failure" hides the only useful thing the browser said.

So the variant carries a message the backend supplies and core never parses — the same shape as
`last_error` (033) and `DeadLetterReason::Caller` (027), and for the same reason: core states a
bound, the implementor supplies the words, and nothing branches on the string.

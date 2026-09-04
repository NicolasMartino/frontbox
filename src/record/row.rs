//! The row a mutation is about, and the row a store holds.
//!
//! Split from `super` for length. Both types are decision 032's: frontbox stores read-model rows
//! as values it never reads, and mutations name the row they touch so the two can be reconciled
//! without anybody parsing a URL.

use serde::{Deserialize, Serialize};

/// Which read-model row a mutation is about.
///
/// # Why this exists
///
/// The application knows, at enqueue, exactly which row it is writing. Before this field it threw
/// that away and recovered it later by parsing the mutation's path — `row_id_of` in
/// `examples/todo-core`, which is guesswork dressed as a lookup and breaks the moment a route
/// changes. Recording the answer costs two strings and retires the parse.
///
/// It is also what makes decision 032's merge rule enforceable: a hydration write can skip rows
/// with queued work only if something knows which rows those are.
///
/// # Opaque, both halves
///
/// `entity` and `row_id` are compared for equality and stored. Core never parses either, for the
/// reason decision 007 gave for entity keys — a `String` keeps the SQLite and IndexedDB backends
/// from having to agree on anything but bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RowRef {
    /// Which kind of row. The application's own vocabulary.
    pub entity: String,
    /// Which row of that kind. Opaque, and unique only within `entity`.
    pub row_id: String,
}

impl RowRef {
    /// Name a row.
    pub fn new(entity: impl Into<String>, row_id: impl Into<String>) -> Self {
        Self {
            entity: entity.into(),
            row_id: row_id.into(),
        }
    }
}

/// A read-model row as a store holds it.
///
/// # What core does and does not know
///
/// It knows the key, that the row exists, and whether it is [`stale`](StoredRow::stale). It does
/// not know what `blob` means, and never deserializes it — which is what dissolves decision 023's
/// objection that owning rows means owning their canonical bytes. A store that never compares bytes
/// cannot be wrong about them.
///
/// `blob` is `serde_json::Value` rather than `Vec<u8>` because the crate already depends on
/// `serde_json` for mutation bodies and a durable backend has to serialize *something*. It is
/// still opaque: core moves it, and does not look inside.
///
/// # Three fields, where decision 032 specified five
///
/// That decision's shape was `(blob, version, stale, schema)` — a caller-supplied content version
/// and a caller-supplied blob-shape stamp beside the flag, on RxDB's precedent. **Both were removed
/// on 2026-09-01, having been written by the trial and read by nothing.**
///
/// The argument against them is the same one that makes this store defensible at all: *core never
/// deserializes the blob*. A stamp beside a blob core cannot interpret duplicates something the
/// application can already express — the blob's bytes are entirely its own, so a shape number or a
/// content version belongs *inside* them, where the code that understands both already is. Neither
/// field bought a query: nothing filters on them and `list_rows` returns whole rows, so a caller
/// reading the stamp always had the blob in hand anyway.
///
/// [`stale`](StoredRow::stale) stays because it is the opposite case: core writes it
/// ([`set_stale`](crate::RowStore::set_stale)), core reads it, and it means the same thing to core
/// as to the application. See `wiki/decisions/032-opaque-row-store.decision.md`, `## Amendment`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct StoredRow {
    /// Which row this is.
    pub row: RowRef,
    /// What the application stored. Never parsed by core.
    pub blob: serde_json::Value,
    /// Whether the row is known to be out of step with the server.
    ///
    /// Decision 023's marker, now living beside the row it describes rather than outliving it.
    pub stale: bool,
}

impl StoredRow {
    /// Build a row for storage.
    pub fn new(row: RowRef, blob: serde_json::Value) -> Self {
        Self {
            row,
            blob,
            stale: false,
        }
    }

    /// Mark it out of step with the server.
    #[must_use]
    pub fn with_stale(mut self, stale: bool) -> Self {
        self.stale = stale;
        self
    }
}

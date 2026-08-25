//! The single core error type.
//!
//! Per `wiki/decisions/002-error-model.decision.md` every trait in this crate returns
//! `Result<T, Error>` rather than declaring an associated error type. The sync runner has to
//! decide whether a failure means "stay queued without backoff", "stay queued with backoff",
//! "surface a storage failure", "quarantine malformed local data", or "reject an impossible
//! server response". It cannot make that decision from an opaque `S::Error`.

use crate::id::MutationId;

/// Every failure this crate can report.
///
/// `#[non_exhaustive]`, so new variants are additive.
///
/// No variant carries `#[from]`. That is deliberate: `?` must not silently classify a
/// [`serde_json::Error`], because the same failure means [`Error::CorruptRecord`] when decoding a
/// durable row and [`Error::Protocol`] when decoding a server response. Requiring an explicit
/// conversion forces the call site to say which one it meant.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// No request could be attempted because the network is unavailable.
    ///
    /// Distinct from [`Error::Transport`] on purpose: offline is an expected operating mode for an
    /// offline-first queue, and must not drive the error-backoff path
    /// (`wiki/decisions/004-transport-auth-and-offline.decision.md`).
    #[error("offline")]
    Offline,

    /// A request was attempted and failed.
    #[error("transport failure")]
    Transport {
        /// The underlying platform failure, if the implementor kept one.
        #[source]
        source: Option<Box<dyn std::error::Error + 'static>>,
    },

    /// A durable store operation failed.
    #[error("storage failure")]
    Storage {
        /// The underlying platform failure, if the implementor kept one.
        #[source]
        source: Option<Box<dyn std::error::Error + 'static>>,
    },

    /// A value could not be serialized for transmission or storage.
    #[error("serialization failure")]
    Serialization {
        /// The underlying serde failure.
        #[source]
        source: serde_json::Error,
    },

    /// A locally stored record cannot be decoded or converted into a sync intent.
    ///
    /// `id` is `None` when the row's own identifier is unreadable, which is exactly the case that
    /// cannot be addressed through an [`Outcome`](crate::store::Outcome) and must be found by
    /// [`OutboxStore::sweep_corrupt`](crate::store::OutboxStore::sweep_corrupt).
    #[error("corrupt local record: {reason}")]
    CorruptRecord {
        /// The record's identifier, when it could be read at all.
        id: Option<MutationId>,
        /// What made the record unusable.
        reason: String,
    },

    /// The server sent something the protocol does not allow.
    #[error("protocol violation: {reason}")]
    Protocol {
        /// What the server sent, and why it is not representable.
        reason: String,
    },

    /// A [`ScopeKey`](crate::scope::ScopeKey) was rejected at construction.
    ///
    /// Required by `wiki/decisions/009-local-scope-identity.decision.md`, which forbids an
    /// unscoped store constructor and therefore needs a typed failure for an unusable key.
    #[error("invalid scope key: {reason}")]
    InvalidScopeKey {
        /// Why the key was rejected.
        reason: String,
    },
}

impl Error {
    /// Build a [`Error::Transport`] from an underlying failure.
    pub fn transport(source: impl std::error::Error + 'static) -> Self {
        Self::Transport {
            source: Some(Box::new(source)),
        }
    }

    /// Build a [`Error::Transport`] with no underlying value.
    pub const fn transport_opaque() -> Self {
        Self::Transport { source: None }
    }

    /// Build a [`Error::Storage`] from an underlying failure.
    pub fn storage(source: impl std::error::Error + 'static) -> Self {
        Self::Storage {
            source: Some(Box::new(source)),
        }
    }

    /// Build a [`Error::Storage`] with no underlying value.
    pub const fn storage_opaque() -> Self {
        Self::Storage { source: None }
    }

    /// Build a [`Error::Serialization`].
    pub const fn serialization(source: serde_json::Error) -> Self {
        Self::Serialization { source }
    }

    /// Build a [`Error::CorruptRecord`] for a record whose identifier was readable.
    pub fn corrupt(id: MutationId, reason: impl Into<String>) -> Self {
        Self::CorruptRecord {
            id: Some(id),
            reason: reason.into(),
        }
    }

    /// Build a [`Error::CorruptRecord`] for a row whose identifier was not readable.
    pub fn corrupt_unidentified(reason: impl Into<String>) -> Self {
        Self::CorruptRecord {
            id: None,
            reason: reason.into(),
        }
    }

    /// Build a [`Error::Protocol`].
    pub fn protocol(reason: impl Into<String>) -> Self {
        Self::Protocol {
            reason: reason.into(),
        }
    }

    /// Build a [`Error::InvalidScopeKey`].
    pub fn invalid_scope_key(reason: impl Into<String>) -> Self {
        Self::InvalidScopeKey {
            reason: reason.into(),
        }
    }

    /// Whether this failure means no request reached the network.
    ///
    /// The runner uses this to keep work queued without counting a failed attempt.
    pub const fn is_offline(&self) -> bool {
        matches!(self, Self::Offline)
    }
}

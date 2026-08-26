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

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    fn underlying() -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::BrokenPipe, "socket closed")
    }

    /// Offline is the one failure the runner treats as "not an attempt", so nothing else may
    /// answer to it. This is the predicate the whole offline-vs-transport distinction rests on.
    #[test]
    fn only_offline_is_offline() {
        assert!(Error::Offline.is_offline());

        for other in [
            Error::transport(underlying()),
            Error::transport_opaque(),
            Error::storage(underlying()),
            Error::storage_opaque(),
            Error::corrupt(MutationId::from_uuid(uuid::Uuid::from_u128(1)), "bad"),
            Error::corrupt_unidentified("unreadable id"),
            Error::protocol("two verdicts"),
            Error::invalid_scope_key("empty"),
        ] {
            assert!(!other.is_offline(), "{other:?} must not read as offline");
        }
    }

    /// An implementor may keep the platform failure or drop it, and both have to survive `?`.
    ///
    /// The `_opaque` constructors exist for backends that have nothing worth boxing. Asserting the
    /// chain both ways is what keeps them from quietly becoming the only shape anyone uses.
    #[test]
    fn an_underlying_failure_is_reachable_when_one_was_kept() {
        let kept = Error::transport(underlying());
        assert_eq!(
            kept.source().expect("a source was supplied").to_string(),
            "socket closed"
        );

        assert!(Error::transport_opaque().source().is_none());
        assert!(Error::storage_opaque().source().is_none());
        assert_eq!(
            Error::storage(underlying())
                .source()
                .expect("a source was supplied")
                .to_string(),
            "socket closed"
        );
    }

    /// A serialization failure keeps the serde error, which is the only way to say *what* failed.
    #[test]
    fn a_serialization_failure_keeps_the_serde_error() {
        let serde_error = serde_json::from_str::<serde_json::Value>("{ not json")
            .expect_err("this is not valid JSON");
        let error = Error::serialization(serde_error);

        assert_eq!(error.to_string(), "serialization failure");
        assert!(error.source().is_some(), "the serde error is the source");
    }

    /// The reason travels in the message for the variants that carry one, and the opaque variants
    /// deliberately do not name a cause they were not given.
    #[test]
    fn messages_carry_the_reason_when_there_is_one() {
        assert_eq!(Error::Offline.to_string(), "offline");
        assert_eq!(Error::transport_opaque().to_string(), "transport failure");
        assert_eq!(Error::storage_opaque().to_string(), "storage failure");
        assert_eq!(
            Error::protocol("two verdicts for one id").to_string(),
            "protocol violation: two verdicts for one id"
        );
        assert_eq!(
            Error::invalid_scope_key("scope key must not be empty").to_string(),
            "invalid scope key: scope key must not be empty"
        );
        assert_eq!(
            Error::corrupt_unidentified("unparseable mutation_id").to_string(),
            "corrupt local record: unparseable mutation_id"
        );
    }

    /// `CorruptRecord` is the one variant whose identifier is optional, and the distinction is
    /// load-bearing: `None` is the row no `Outcome` can name, which only `sweep_corrupt` can reach.
    #[test]
    fn a_corrupt_record_reports_its_id_only_when_it_had_a_readable_one() {
        let id = MutationId::from_uuid(uuid::Uuid::from_u128(9));

        match Error::corrupt(id, "body is not valid JSON") {
            Error::CorruptRecord { id: Some(got), .. } => assert_eq!(got, id),
            other => panic!("expected an identified corrupt record, got {other:?}"),
        }

        match Error::corrupt_unidentified("unparseable mutation_id") {
            Error::CorruptRecord { id: None, reason } => {
                assert_eq!(reason, "unparseable mutation_id");
            }
            other => panic!("expected an unidentified corrupt record, got {other:?}"),
        }
    }
}

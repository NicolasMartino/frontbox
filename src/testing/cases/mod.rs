//! The conformance cases.
//!
//! Each function is a single behavioural assertion that every backend must satisfy. They are
//! public and individually callable so a backend can run each as its own test — a wasm backend, for
//! example, needs one `#[wasm_bindgen_test]` per case rather than one giant one.
//!
//! Several cases exist to prove a *decision* rather than to guard a line of code. Those are called
//! out in their own doc comments, because a failure there means the design is wrong, not that a
//! refactor slipped.
//!
//! # Layout
//!
//! The cases are grouped into submodules by the behaviour they pin down, and re-exported here, so
//! `crate::testing::cases::case_01_applied_is_deleted` is the path regardless of which file holds
//! it. The numbering is stable and global: it is what the macros, the wiki, and every review
//! comment refer to, so a case keeps its number even when it moves file.

mod anomalies;
mod corruption;
mod dead_letters;
mod drain;
mod envelope;
mod invalidation;
mod liveness;
mod ordering;
mod pass_control;
mod pull_conflict;
mod rows;
mod scope_isolation;
mod single_flight;
mod status;
mod transport;

pub use anomalies::*;
pub use corruption::*;
pub use dead_letters::*;
pub use drain::*;
pub use envelope::*;
pub use invalidation::*;
pub use liveness::*;
pub use ordering::*;
pub use pass_control::*;
pub use pull_conflict::*;
pub use rows::*;
pub use scope_isolation::*;
pub use single_flight::*;
pub use status::*;
pub use transport::*;

/// What every case module needs.
///
/// A glob import rather than a per-file list. The cases are a suite of near-identical scaffolding
/// around one assertion each, so hand-maintaining ten import blocks that differ by one name would
/// be churn without a reader to serve.
mod prelude {
    pub(super) use crate::cache::{
        CacheVersion, CacheVersionStore, EntityState, InvalidationEvent, InvalidationRunner,
        PendingConflict,
    };
    pub(super) use crate::entity::SliceRegistry;
    pub(super) use crate::error::Error;
    pub(super) use crate::protocol::{MutationResult, MutationStatus};
    pub(super) use crate::record::OutboxRecord;
    pub(super) use crate::record::{DeadLetterReason, MutationIntent, OperationMeta};
    pub(super) use crate::runner::{Anomaly, AnomalyKind, SyncPass, SyncRunner};
    pub(super) use crate::scope::ScopeKey;
    pub(super) use crate::store::{
        DeadLetterStore, Disposition, OutboxStore, Outcome, QuarantineStore,
    };
    pub(super) use crate::testing::{
        full_rejection, id, intent, registry, scope, CorruptKind, FaultInjection, Reply,
        ScriptedTransport, StoreFactory, VersionStoreFactory,
    };

    pub(super) use super::{cache, open, open_both, runner, seed};
}

use prelude::*;

/// Open a store on a fresh default scope.
async fn open<F: StoreFactory>(factory: &F) -> Result<(F::Store, ScopeKey), Error> {
    let key = scope("user:alice@tenant:acme@schema:1");
    let store = factory.open(key.clone()).await?;
    Ok((store, key))
}

/// Enqueue `n` intents with `created_at` running 1..=n.
async fn seed<S: OutboxStore>(store: &S, n: u128) -> Result<(), Error> {
    for i in 1..=n {
        store.enqueue(intent(i, i as i64)).await?;
    }
    Ok(())
}

fn runner<S: OutboxStore>(store: S, reply: Reply) -> SyncRunner<S, ScriptedTransport> {
    SyncRunner::new(store, ScriptedTransport::new(reply))
}

/// Open an outbox and a version store on one scope.
///
/// Both halves of decision 014's question live here: whether an entity is stale is a version-store
/// fact, and whether refetching it would discard work is an outbox fact. A case that could only
/// open one of them could not ask the question.
async fn open_both<F: VersionStoreFactory>(
    factory: &F,
) -> Result<(F::Store, F::Versions, ScopeKey), Error> {
    let key = scope("user:alice@tenant:acme@schema:1");
    let outbox = factory.open(key.clone()).await?;
    let versions = factory.open_versions(key.clone()).await?;
    Ok((outbox, versions, key))
}

/// Wrap a version store in an invalidation runner over the suite's registry.
fn cache<V: CacheVersionStore>(store: V) -> InvalidationRunner<V, SliceRegistry<&'static str>> {
    InvalidationRunner::new(store, registry())
}

//! Atomic migrations preserve causal order, audit facts and account boundaries.

use super::prelude::*;
use crate::testing::MigrationFaultInjection;
use crate::{
    CoalescingEnqueue, CoalescingPolicy, CoalescingRefusal, DeadLetterRecord, DrainLease,
    MigrationReport, MigrationStore, MutationPayload, PendingMigration, RowRef, RowStore,
    StorageMigration, StoredRow,
};
use serde_json::{json, Value};

struct Upgrade {
    fail_row: bool,
    replacement: Option<crate::MutationId>,
}

impl Upgrade {
    fn new() -> Self {
        Self {
            fail_row: false,
            replacement: None,
        }
    }

    fn payload(
        &self,
        body: &Value,
        op: &Option<OperationMeta>,
        guard: &Option<String>,
    ) -> Result<Option<MutationPayload>, Error> {
        match op.as_ref().and_then(|op| op.version.as_deref()) {
            Some("2") | None => Ok(None),
            Some("1") => Ok(Some(MutationPayload::new(
                json!({"row": body, "base": null}),
                Some(OperationMeta::new("save").with_version("2")),
                guard.clone(),
            ))),
            _ => Err(Error::protocol("unknown format")),
        }
    }
}

impl StorageMigration for Upgrade {
    fn pending(&self, record: &OutboxRecord) -> Result<Option<PendingMigration>, Error> {
        Ok(self
            .payload(&record.body, &record.op, &record.precondition)?
            .map(|payload| match self.replacement {
                Some(id) => PendingMigration::new_request(payload, id),
                None => PendingMigration::local(payload),
            }))
    }

    fn dead_letter(&self, record: &DeadLetterRecord) -> Result<Option<MutationPayload>, Error> {
        self.payload(&record.body, &record.op, &record.precondition)
    }

    fn row(&self, row: &StoredRow) -> Result<Option<Value>, Error> {
        if self.fail_row {
            return Err(Error::protocol("invalid cached blob"));
        }
        Ok((row.blob.get("version") != Some(&json!(2)))
            .then(|| json!({"version":2,"row":row.blob})))
    }
}

fn edit(n: u128) -> MutationIntent {
    intent(n, 100 - n as i64)
        .with_row(RowRef::new("things", n.to_string()))
        .with_op(OperationMeta::new("save").with_version("1"))
        .with_precondition("server-base")
        .with_traceparent("original-trace")
}

async fn seed_migration<S: OutboxStore + RowStore>(store: &S) -> Result<(), Error> {
    store.enqueue(intent(10, 1000)).await?; // unrelated parent
    store.enqueue(edit(1)).await?;
    store.enqueue(intent(11, 1)).await?; // unrelated child
    store.enqueue(edit(2)).await?;
    store.read_for_send(4).await?;
    store
        .apply_outcomes(&[
            Outcome::new(
                id(1),
                Disposition::Retain {
                    reason: Some("retry later".into()),
                },
            ),
            Outcome::new(
                id(2),
                Disposition::DeadLetter {
                    reason: DeadLetterReason::Rejected {
                        error: Some(full_rejection()),
                    },
                },
            ),
        ])
        .await?;
    store
        .put_rows(&[
            StoredRow::new(RowRef::new("things", "1"), json!({"draft":"kept"})).with_stale(true),
        ])
        .await
}

/// In-place rewrites retain the entire pending and rejected audit history, even after reopen.
pub async fn case_83_migration_preserves_order_and_audit<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error>
where
    F::Store: MigrationStore + RowStore,
{
    let key = scope("migration-order");
    let store = factory.open(key.clone()).await?;
    seed_migration(&store).await?;
    let mut expected = store.pending_batch(10).await?;
    let mut rejected = DeadLetterStore::list(&store, 10).await?;
    expected[1].body = json!({"row": expected[1].body, "base":null});
    expected[1].op = Some(OperationMeta::new("save").with_version("2"));
    rejected[0].body = json!({"row":rejected[0].body,"base":null});
    rejected[0].op = Some(OperationMeta::new("save").with_version("2"));
    let report = store.migrate(&Upgrade::new()).await?.expect("available");
    assert_eq!(
        (report.pending, report.dead_letters, report.rows),
        (1, 1, 1)
    );
    drop(store);
    let store = factory.open(key).await?;
    assert_eq!(store.pending_batch(10).await?, expected);
    assert_eq!(DeadLetterStore::list(&store, 10).await?, rejected);
    let row = store.get_row(&RowRef::new("things", "1")).await?.unwrap();
    assert!(row.stale);
    assert_eq!(row.blob, json!({"version":2,"row":{"draft":"kept"}}));
    assert_eq!(
        store.migrate(&Upgrade::new()).await?,
        Some(MigrationReport::default())
    );
    assert!(matches!(
        store
            .enqueue_coalescing(edit(1), CoalescingPolicy::RequireExisting)
            .await?,
        CoalescingEnqueue::NotQueued {
            reason: CoalescingRefusal::TransportStarted,
            ..
        }
    ));
    // The sequence generator also survives: later work must remain after the migrated child.
    store.enqueue(intent(12, 0)).await?;
    assert!(store.pending_batch(10).await?.last().unwrap().seq > expected[2].seq);
    Ok(())
}

/// A callback failing after other records were validated leaves all three stores unchanged.
pub async fn case_84_migration_failure_rolls_back_every_store<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error>
where
    F::Store: MigrationStore + RowStore,
{
    let key = scope("migration-rollback");
    let store = factory.open(key.clone()).await?;
    seed_migration(&store).await?;
    let pending = store.pending_batch(10).await?;
    let letters = DeadLetterStore::list(&store, 10).await?;
    let rows = store.list_rows("things", 10).await?;
    assert!(store
        .migrate(&Upgrade {
            fail_row: true,
            ..Upgrade::new()
        })
        .await
        .is_err());
    drop(store);
    let store = factory.open(key).await?;
    assert_eq!(store.pending_batch(10).await?, pending);
    assert_eq!(DeadLetterStore::list(&store, 10).await?, letters);
    assert_eq!(store.list_rows("things", 10).await?, rows);
    assert!(
        store.migrate(&Upgrade::new()).await?.is_some(),
        "failed migration releases its lease"
    );
    Ok(())
}

/// Account isolation includes pending writes, rejected edits, cached blobs and quarantine.
pub async fn case_85_migration_is_scoped_and_keeps_quarantine<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error>
where
    F::Store: MigrationStore + RowStore,
{
    let key = scope("migration/owner");
    let store = factory.open(key.clone()).await?;
    let other = factory.open(scope("migration_owner")).await?;
    seed_migration(&store).await?;
    seed_migration(&other).await?;
    factory
        .insert_corrupt_row(&key, CorruptKind::UnparseableId)
        .await?;
    store.sweep_corrupt().await?;
    let quarantine = QuarantineStore::list(&store, 10).await?;
    let pending = other.pending_batch(10).await?;
    let letters = DeadLetterStore::list(&other, 10).await?;
    let rows = other.list_rows("things", 10).await?;
    store.migrate(&Upgrade::new()).await?;
    assert_eq!(other.pending_batch(10).await?, pending);
    assert_eq!(DeadLetterStore::list(&other, 10).await?, letters);
    assert_eq!(other.list_rows("things", 10).await?, rows);
    assert_eq!(QuarantineStore::list(&store, 10).await?, quarantine);
    Ok(())
}

/// A changed outgoing request gets a new identifier in the same slot; collisions abort.
pub async fn case_86_migration_rekeys_without_reordering<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error>
where
    F::Store: MigrationStore + RowStore,
{
    let store = factory.open(scope("migration-identities")).await?;
    seed_migration(&store).await?;
    let before = store.pending_batch(10).await?;
    for collision in [id(1), id(2), id(11)] {
        assert!(store
            .migrate(&Upgrade {
                replacement: Some(collision),
                ..Upgrade::new()
            })
            .await
            .is_err());
        assert_eq!(store.pending_batch(10).await?, before);
    }
    let report = store
        .migrate(&Upgrade {
            replacement: Some(id(99)),
            ..Upgrade::new()
        })
        .await?
        .unwrap();
    assert_eq!(report.pending, 1);
    let pending = store.pending_batch(10).await?;
    assert_eq!(
        pending
            .iter()
            .map(|row| row.mutation_id)
            .collect::<Vec<_>>(),
        [id(10), id(99), id(11)]
    );
    assert_eq!(pending[1].seq, before[1].seq);
    assert_eq!(pending[1].attempts, before[1].attempts);
    assert_eq!(
        DeadLetterStore::list(&store, 10).await?[0].mutation_id,
        id(2)
    );
    assert_eq!(
        store.migrate(&Upgrade::new()).await?,
        Some(MigrationReport::default())
    );
    Ok(())
}

/// Migration and SyncRunner share one scope claim; a busy scope runs no transform.
pub async fn case_87_migration_and_drain_share_exclusion<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error>
where
    F::Store: MigrationStore + RowStore,
{
    let store = factory.open(scope("migration-exclusion")).await?;
    seed_migration(&store).await?;
    let lease = DrainLease::claim(&store).await?.expect("available");
    let same = factory.open(store.scope().clone()).await?;
    assert_eq!(same.migrate(&Upgrade::new()).await?, None);
    let runner = runner(same, Reply::All(MutationStatus::Applied));
    assert_eq!(runner.sync_once().await?.pass, SyncPass::AlreadyRunning);
    drop(lease);
    assert!(store.migrate(&Upgrade::new()).await?.is_some());
    assert_eq!(runner.sync_once().await?.pass, SyncPass::Completed);
    Ok(())
}

/// Unknown application versions and unswept corruption fail without losing any write.
pub async fn case_88_migration_rejects_unknown_and_corrupt_records<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error>
where
    F::Store: MigrationStore + RowStore,
{
    let key = scope("migration-invalid");
    let store = factory.open(key.clone()).await?;
    store.enqueue(edit(1)).await?;
    store
        .enqueue(edit(2).with_op(OperationMeta::new("save").with_version("future")))
        .await?;
    let before = store.pending_batch(10).await?;
    assert!(store.migrate(&Upgrade::new()).await.is_err());
    assert_eq!(store.pending_batch(10).await?, before);
    store
        .apply_outcomes(&[Outcome::new(id(2), Disposition::Delete)])
        .await?;
    factory
        .insert_corrupt_row(&key, CorruptKind::InvalidBody { id: id(3) })
        .await?;
    assert!(store.migrate(&Upgrade::new()).await.is_err());
    assert_eq!(store.pending_batch(10).await?, before[..1]);
    assert_eq!(store.sweep_corrupt().await?, 1);
    assert!(store.migrate(&Upgrade::new()).await?.is_some());
    Ok(())
}

/// An aborted write transaction leaves the complete pre-migration state, including on reopen.
pub async fn case_89_migration_write_failure_rolls_back<F: MigrationFaultInjection>(
    factory: &F,
) -> Result<(), Error>
where
    F::Store: MigrationStore + RowStore,
{
    let key = scope("migration-storage-failure");
    let store = factory.open(key.clone()).await?;
    seed_migration(&store).await?;
    let pending = store.pending_batch(10).await?;
    let letters = DeadLetterStore::list(&store, 10).await?;
    let rows = store.list_rows("things", 10).await?;
    factory.fail_next_migration().await;
    assert!(store.migrate(&Upgrade::new()).await.is_err());
    drop(store);
    let store = factory.open(key).await?;
    assert_eq!(store.pending_batch(10).await?, pending);
    assert_eq!(DeadLetterStore::list(&store, 10).await?, letters);
    assert_eq!(store.list_rows("things", 10).await?, rows);
    assert!(store.migrate(&Upgrade::new()).await?.is_some());
    Ok(())
}

/// A request handed to transport without a verdict stays non-coalescible after migration.
pub async fn case_90_migration_keeps_unacknowledged_transport_start<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error>
where
    F::Store: MigrationStore + RowStore,
{
    let key = scope("migration-started");
    let store = factory.open(key.clone()).await?;
    store.enqueue(edit(1)).await?;
    assert_eq!(store.read_for_send(1).await?[0].attempts, 0);
    store.migrate(&Upgrade::new()).await?;
    drop(store);
    let store = factory.open(key).await?;
    assert_eq!(store.pending_batch(1).await?[0].attempts, 0);
    assert!(matches!(
        store
            .enqueue_coalescing(edit(1), CoalescingPolicy::RequireExisting)
            .await?,
        CoalescingEnqueue::NotQueued {
            reason: CoalescingRefusal::TransportStarted,
            ..
        }
    ));
    Ok(())
}

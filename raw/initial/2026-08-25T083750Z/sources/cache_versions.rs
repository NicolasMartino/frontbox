//! Cache version repository for entity invalidation tracking.
//!
//! Tracks version numbers per entity type per user in Postgres.
//! Version is incremented on each change, enabling precise cache invalidation via SSE.

use std::collections::HashMap;
use std::fmt;

use db::DbPool;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Entity types that can be versioned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Exercises,
    Sessions,
    Templates,
    UserPreferences,
}

impl EntityType {
    /// Get the database table name for this entity type.
    pub fn table_name(&self) -> &'static str {
        match self {
            EntityType::Exercises => "cache_exercises",
            EntityType::Sessions => "cache_sessions",
            EntityType::Templates => "cache_templates",
            EntityType::UserPreferences => "cache_user_preferences",
        }
    }

    /// Get the string representation for SSE events.
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityType::Exercises => "exercises",
            EntityType::Sessions => "sessions",
            EntityType::Templates => "templates",
            EntityType::UserPreferences => "user_preferences",
        }
    }

    /// Parse from string (SSE event entity name).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "exercises" => Some(EntityType::Exercises),
            "sessions" => Some(EntityType::Sessions),
            "templates" => Some(EntityType::Templates),
            "user_preferences" => Some(EntityType::UserPreferences),
            _ => None,
        }
    }

    /// All entity types.
    pub fn all() -> &'static [EntityType] {
        &[
            EntityType::Exercises,
            EntityType::Sessions,
            EntityType::Templates,
            EntityType::UserPreferences,
        ]
    }
}

impl fmt::Display for EntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Row from cache version tables.
#[derive(Debug, FromRow)]
struct CacheVersionRow {
    version: i64,
}

/// Row from invalidation ledger table.
#[derive(Debug, FromRow)]
struct InvalidationLedgerRow {
    version: i64,
}

/// Repository for cache version operations.
#[derive(Clone)]
pub struct CacheVersionRepo {
    pool: DbPool,
}

impl CacheVersionRepo {
    /// Create a new cache version repository.
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Increment version for an entity type, returns new version.
    /// Uses upsert to handle first-time access.
    pub async fn bump(&self, user_id: Uuid, entity: EntityType) -> Result<u64, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let bumped = Self::bump_in_tx(&mut tx, user_id, entity).await?;
        tx.commit().await?;
        Ok(bumped)
    }

    /// Get the version for this outcome+entity if already assigned, otherwise bump once and
    /// record it atomically in the invalidation ledger.
    pub async fn get_or_bump_for_outcome(
        &self,
        correlation_id: Uuid,
        batch_id: &str,
        user_id: Uuid,
        entity: EntityType,
    ) -> Result<u64, sqlx::Error> {
        if let Some(existing) = self
            .fetch_ledger_version(correlation_id, batch_id, entity)
            .await?
        {
            return Ok(existing);
        }

        let mut tx = self.pool.begin().await?;

        if let Some(existing) =
            Self::fetch_ledger_version_in_tx(&mut tx, correlation_id, batch_id, entity).await?
        {
            tx.rollback().await?;
            return Ok(existing);
        }

        let version = Self::bump_in_tx(&mut tx, user_id, entity).await?;
        let inserted = sqlx::query(
            r#"
            INSERT INTO cache_invalidation_ledger (
                correlation_id,
                batch_id,
                entity,
                version,
                created_at
            )
            VALUES ($1, $2, $3, $4, NOW())
            ON CONFLICT (correlation_id, batch_id, entity) DO NOTHING
            "#,
        )
        .bind(correlation_id)
        .bind(batch_id)
        .bind(entity.as_str())
        .bind(version as i64)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if inserted == 0 {
            tx.rollback().await?;
            if let Some(existing) = self
                .fetch_ledger_version(correlation_id, batch_id, entity)
                .await?
            {
                return Ok(existing);
            }
            return Err(sqlx::Error::RowNotFound);
        }

        tx.commit().await?;
        Ok(version)
    }

    /// Delete expired invalidation ledger rows in bounded batches.
    pub async fn cleanup_invalidation_ledger_batch(
        &self,
        limit: i64,
        retention_secs: i64,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            WITH doomed AS (
                SELECT correlation_id, batch_id, entity
                FROM cache_invalidation_ledger
                WHERE created_at < NOW() - ($1::bigint * INTERVAL '1 second')
                ORDER BY created_at ASC
                LIMIT $2
            )
            DELETE FROM cache_invalidation_ledger cil
            USING doomed d
            WHERE cil.correlation_id = d.correlation_id
              AND cil.batch_id = d.batch_id
              AND cil.entity = d.entity
            "#,
        )
        .bind(retention_secs)
        .bind(limit)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    async fn bump_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
        entity: EntityType,
    ) -> Result<u64, sqlx::Error> {
        let table = entity.table_name();

        // We need to use raw SQL since table name can't be parameterized
        let query = format!(
            r#"
            INSERT INTO {} (user_id, version, updated_at)
            VALUES ($1, 1, NOW())
            ON CONFLICT (user_id) DO UPDATE
            SET version = {}.version + 1, updated_at = NOW()
            RETURNING version
            "#,
            table, table
        );

        let row: CacheVersionRow = sqlx::query_as(&query)
            .bind(user_id)
            .fetch_one(&mut **tx)
            .await?;

        Ok(row.version as u64)
    }

    async fn fetch_ledger_version(
        &self,
        correlation_id: Uuid,
        batch_id: &str,
        entity: EntityType,
    ) -> Result<Option<u64>, sqlx::Error> {
        let row: Option<InvalidationLedgerRow> = sqlx::query_as(
            r#"
            SELECT version
            FROM cache_invalidation_ledger
            WHERE correlation_id = $1
              AND batch_id = $2
              AND entity = $3
            "#,
        )
        .bind(correlation_id)
        .bind(batch_id)
        .bind(entity.as_str())
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.version as u64))
    }

    async fn fetch_ledger_version_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        correlation_id: Uuid,
        batch_id: &str,
        entity: EntityType,
    ) -> Result<Option<u64>, sqlx::Error> {
        let row: Option<InvalidationLedgerRow> = sqlx::query_as(
            r#"
            SELECT version
            FROM cache_invalidation_ledger
            WHERE correlation_id = $1
              AND batch_id = $2
              AND entity = $3
            FOR UPDATE
            "#,
        )
        .bind(correlation_id)
        .bind(batch_id)
        .bind(entity.as_str())
        .fetch_optional(&mut **tx)
        .await?;

        Ok(row.map(|r| r.version as u64))
    }

    /// Get current version (0 if never set).
    pub async fn get(&self, user_id: Uuid, entity: EntityType) -> Result<u64, sqlx::Error> {
        let table = entity.table_name();
        let query = format!("SELECT version FROM {} WHERE user_id = $1", table);

        let result: Option<CacheVersionRow> = sqlx::query_as(&query)
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(result.map(|r| r.version as u64).unwrap_or(0))
    }

    /// Get all versions for a user.
    pub async fn get_all(&self, user_id: Uuid) -> Result<HashMap<EntityType, u64>, sqlx::Error> {
        let mut versions = HashMap::new();

        for entity in EntityType::all() {
            let version = self.get(user_id, *entity).await?;
            versions.insert(*entity, version);
        }

        Ok(versions)
    }

    /// Reset version to 0 (for self-correction on anomalies).
    pub async fn reset(&self, user_id: Uuid, entity: EntityType) -> Result<(), sqlx::Error> {
        let table = entity.table_name();
        let query = format!(
            r#"
            INSERT INTO {} (user_id, version, updated_at)
            VALUES ($1, 0, NOW())
            ON CONFLICT (user_id) DO UPDATE
            SET version = 0, updated_at = NOW()
            "#,
            table
        );

        sqlx::query(&query)
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Connection;

    async fn maybe_test_pool() -> Option<db::DbPool> {
        let base_url = std::env::var("APP_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:7060/app_db".to_string());
        let Some((prefix, _)) = base_url.rsplit_once('/') else {
            eprintln!(
                "skipping cache version tests: invalid database url {}",
                base_url
            );
            return None;
        };
        let database_name = format!("app_cache_version_test_{}", Uuid::new_v4().simple());
        let admin_url = format!("{}/postgres", prefix);
        let url = format!("{}/{}", prefix, database_name);

        let Ok(mut admin) = sqlx::PgConnection::connect(&admin_url).await else {
            eprintln!(
                "skipping cache version tests: cannot connect to admin db {}",
                admin_url
            );
            return None;
        };
        if let Err(err) = sqlx::query(&format!("CREATE DATABASE \"{}\"", database_name))
            .execute(&mut admin)
            .await
        {
            eprintln!(
                "skipping cache version tests: failed to create {} ({})",
                database_name, err
            );
            return None;
        }
        drop(admin);

        let Ok(pool) = db::DbPool::connect(&url).await else {
            eprintln!("skipping cache version tests: cannot connect to {}", url);
            return None;
        };
        if let Err(err) = sqlx::migrate!("./migrations").run(&pool).await {
            eprintln!("skipping cache version tests: migration failed ({})", err);
            return None;
        }

        Some(pool)
    }

    #[test]
    fn entity_type_helpers_cover_catalog_and_roundtrip() {
        assert_eq!(EntityType::all().len(), 4);
        assert_eq!(EntityType::Exercises.table_name(), "cache_exercises");
        assert_eq!(EntityType::Sessions.table_name(), "cache_sessions");
        assert_eq!(EntityType::Templates.table_name(), "cache_templates");
        assert_eq!(
            EntityType::UserPreferences.table_name(),
            "cache_user_preferences"
        );
        assert_eq!(EntityType::Exercises.as_str(), "exercises");
        assert_eq!(
            EntityType::parse("user_preferences"),
            Some(EntityType::UserPreferences)
        );
        assert_eq!(EntityType::parse("unknown"), None);
        assert_eq!(EntityType::Templates.to_string(), "templates");
    }

    #[tokio::test]
    async fn bump_get_get_all_and_reset_roundtrip() {
        let Some(pool) = maybe_test_pool().await else {
            return;
        };

        let repo = CacheVersionRepo::new(pool);
        let user_id = Uuid::new_v4();

        for entity in EntityType::all() {
            assert_eq!(
                repo.get(user_id, *entity).await.expect("initial get"),
                0,
                "fresh users should start at version 0 for {entity}"
            );
        }

        assert_eq!(
            repo.bump(user_id, EntityType::Exercises)
                .await
                .expect("first bump"),
            1
        );
        assert_eq!(
            repo.bump(user_id, EntityType::Exercises)
                .await
                .expect("second bump"),
            2
        );
        assert_eq!(
            repo.bump(user_id, EntityType::Templates)
                .await
                .expect("template bump"),
            1
        );

        let versions = repo.get_all(user_id).await.expect("get all versions");
        assert_eq!(versions[&EntityType::Exercises], 2);
        assert_eq!(versions[&EntityType::Templates], 1);
        assert_eq!(versions[&EntityType::Sessions], 0);
        assert_eq!(versions[&EntityType::UserPreferences], 0);

        repo.reset(user_id, EntityType::Exercises)
            .await
            .expect("reset exercise version");
        assert_eq!(
            repo.get(user_id, EntityType::Exercises)
                .await
                .expect("get reset version"),
            0
        );
        assert_eq!(
            repo.get(user_id, EntityType::Templates)
                .await
                .expect("template version should remain"),
            1
        );
    }

    #[tokio::test]
    async fn get_or_bump_for_outcome_is_idempotent_per_entity() {
        let Some(pool) = maybe_test_pool().await else {
            return;
        };

        let repo = CacheVersionRepo::new(pool.clone());
        let user_id = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();
        let batch_id = "batch-cache-idempotent";

        let first = repo
            .get_or_bump_for_outcome(correlation_id, batch_id, user_id, EntityType::Exercises)
            .await
            .expect("first outcome version");
        let replay = repo
            .get_or_bump_for_outcome(correlation_id, batch_id, user_id, EntityType::Exercises)
            .await
            .expect("replayed outcome version");
        let template = repo
            .get_or_bump_for_outcome(correlation_id, batch_id, user_id, EntityType::Templates)
            .await
            .expect("template outcome version");

        assert_eq!(first, 1);
        assert_eq!(replay, 1);
        assert_eq!(template, 1);
        assert_eq!(
            repo.get(user_id, EntityType::Exercises)
                .await
                .expect("exercise version"),
            1
        );
        assert_eq!(
            repo.get(user_id, EntityType::Templates)
                .await
                .expect("template version"),
            1
        );

        let exercise_ledger_rows: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM cache_invalidation_ledger
            WHERE correlation_id = $1
              AND batch_id = $2
              AND entity = $3
            "#,
        )
        .bind(correlation_id)
        .bind(batch_id)
        .bind(EntityType::Exercises.as_str())
        .fetch_one(&pool)
        .await
        .expect("count exercise ledger rows");
        let template_ledger_rows: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM cache_invalidation_ledger
            WHERE correlation_id = $1
              AND batch_id = $2
              AND entity = $3
            "#,
        )
        .bind(correlation_id)
        .bind(batch_id)
        .bind(EntityType::Templates.as_str())
        .fetch_one(&pool)
        .await
        .expect("count template ledger rows");

        assert_eq!(exercise_ledger_rows, 1);
        assert_eq!(template_ledger_rows, 1);
    }

    #[tokio::test]
    async fn cleanup_invalidation_ledger_batch_removes_only_expired_rows() {
        let Some(pool) = maybe_test_pool().await else {
            return;
        };

        let repo = CacheVersionRepo::new(pool.clone());
        let user_id = Uuid::new_v4();

        repo.get_or_bump_for_outcome(
            Uuid::new_v4(),
            "batch-old-a",
            user_id,
            EntityType::Exercises,
        )
        .await
        .expect("seed old ledger row a");
        repo.get_or_bump_for_outcome(Uuid::new_v4(), "batch-old-b", user_id, EntityType::Sessions)
            .await
            .expect("seed old ledger row b");
        repo.get_or_bump_for_outcome(
            Uuid::new_v4(),
            "batch-recent",
            user_id,
            EntityType::Templates,
        )
        .await
        .expect("seed recent ledger row");

        sqlx::query(
            r#"
            UPDATE cache_invalidation_ledger
            SET created_at = NOW() - INTERVAL '2 hours'
            WHERE batch_id IN ('batch-old-a', 'batch-old-b')
            "#,
        )
        .execute(&pool)
        .await
        .expect("age old ledger rows");

        assert_eq!(
            repo.cleanup_invalidation_ledger_batch(1, 60)
                .await
                .expect("cleanup first batch"),
            1
        );
        assert_eq!(
            repo.cleanup_invalidation_ledger_batch(1, 60)
                .await
                .expect("cleanup second batch"),
            1
        );
        assert_eq!(
            repo.cleanup_invalidation_ledger_batch(1, 60)
                .await
                .expect("cleanup should finish"),
            0
        );

        let old_rows_remaining: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM cache_invalidation_ledger
            WHERE batch_id IN ('batch-old-a', 'batch-old-b')
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("count remaining old rows");
        let recent_rows_remaining: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM cache_invalidation_ledger
            WHERE batch_id = 'batch-recent'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("count remaining recent rows");

        assert_eq!(old_rows_remaining, 0);
        assert_eq!(recent_rows_remaining, 1);
    }
}

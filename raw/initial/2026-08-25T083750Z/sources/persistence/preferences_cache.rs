//! Client-side preference cache for offline support.
//!
//! Stores user preferences locally in the app database to enable:
//! - Fast loading on app startup
//! - Offline access to preferences
//! - Optimistic updates before server sync
//!
//! Preferences are also synced to the server via queued mutations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shared::{MAX_REST_SECONDS, MIN_REST_SECONDS};
use uuid::Uuid;

use super::{Database, StoreResult};

/// User preferences stored in local cache (internal storage format)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedPreferencesRecord {
    pub user_id: String,
    pub units: String,
    pub theme: String,
    pub language: String,
    pub default_rest_seconds: u32,
    pub updated_at: i64,        // Unix timestamp in milliseconds
    pub synced_at: Option<i64>, // Unix timestamp in milliseconds
}

/// User preferences with parsed dates (public API)
#[derive(Debug, Clone)]
pub struct CachedPreferences {
    pub user_id: String,
    pub units: String,
    pub theme: String,
    pub language: String,
    pub default_rest_seconds: u32,
    pub updated_at: DateTime<Utc>,
    pub synced_at: Option<DateTime<Utc>>,
}

impl CachedPreferences {
    #[cfg(any(target_arch = "wasm32", test))]
    fn to_record(&self) -> CachedPreferencesRecord {
        CachedPreferencesRecord {
            user_id: self.user_id.clone(),
            units: self.units.clone(),
            theme: self.theme.clone(),
            language: self.language.clone(),
            default_rest_seconds: self.default_rest_seconds,
            updated_at: self.updated_at.timestamp_millis(),
            synced_at: self.synced_at.map(|dt| dt.timestamp_millis()),
        }
    }

    fn from_record(record: CachedPreferencesRecord) -> StoreResult<Self> {
        if !(MIN_REST_SECONDS..=MAX_REST_SECONDS).contains(&record.default_rest_seconds) {
            return Err(format!(
                "Invalid default_rest_seconds in cache: {} (expected {}..={})",
                record.default_rest_seconds, MIN_REST_SECONDS, MAX_REST_SECONDS
            ));
        }
        Ok(CachedPreferences {
            user_id: record.user_id,
            units: record.units,
            theme: record.theme,
            language: record.language,
            default_rest_seconds: record.default_rest_seconds,
            updated_at: DateTime::from_timestamp_millis(record.updated_at)
                .ok_or_else(|| "Invalid updated_at timestamp".to_string())?,
            synced_at: record.synced_at.and_then(DateTime::from_timestamp_millis),
        })
    }
}

/// Preference cache operations
pub struct PreferenceCache;

impl PreferenceCache {
    /// Get cached preferences for a user
    pub async fn get(db: &Database, user_id: Uuid) -> StoreResult<Option<CachedPreferences>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rusqlite::OptionalExtension;

            let conn = db.conn.lock().await;
            let user_id_str = user_id.to_string();

            let result = conn
                .query_row(
                    "SELECT user_id, units, theme, language, default_rest_seconds, updated_at, synced_at
                     FROM user_preferences_cache
                     WHERE user_id = ?1",
                    [&user_id_str],
                    |row| {
                        let default_rest_seconds_i32: i32 = row.get(4)?;
                        let default_rest_seconds = u32::try_from(default_rest_seconds_i32).map_err(|_| {
                            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!(
                                    "Invalid default_rest_seconds in cache: {}",
                                    default_rest_seconds_i32
                                ),
                            )))
                        })?;
                        let record = CachedPreferencesRecord {
                            user_id: row.get(0)?,
                            units: row.get(1)?,
                            theme: row.get(2)?,
                            language: row.get(3)?,
                            default_rest_seconds,
                            updated_at: {
                                let timestamp: i64 = row.get(5)?;
                                timestamp * 1000 // Convert seconds to milliseconds
                            },
                            synced_at: {
                                let timestamp_opt: Option<i64> = row.get(6)?;
                                timestamp_opt.map(|ts| ts * 1000) // Convert seconds to milliseconds
                            },
                        };
                        CachedPreferences::from_record(record)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))))
                    },
                )
                .optional()
                .map_err(|e| format!("Failed to query user_preferences_cache: {}", e))?;

            Ok(result)
        }

        #[cfg(target_arch = "wasm32")]
        {
            use rexie::TransactionMode;
            use wasm_bindgen::JsValue;

            #[cfg(debug_assertions)]
            crate::log!(
                "[PreferenceCache::get] Starting get for user_id={}",
                user_id
            );

            let tx = db
                .transaction(&["user_preferences_cache"], TransactionMode::ReadOnly)
                .map_err(|e| format!("Failed to create transaction: {:?}", e))?;
            #[cfg(debug_assertions)]
            crate::log!("[PreferenceCache::get] Transaction created");

            let store = tx
                .store("user_preferences_cache")
                .map_err(|e| format!("Failed to get store: {:?}", e))?;
            #[cfg(debug_assertions)]
            crate::log!("[PreferenceCache::get] Store opened");

            let key = JsValue::from_str(&user_id.to_string());
            let result = store
                .get(key)
                .await
                .map_err(|e| format!("Failed to get preferences: {:?}", e))?;

            match result {
                Some(v) => {
                    #[cfg(debug_assertions)]
                    crate::log!("[PreferenceCache::get] Record found, deserializing...");
                    let record: CachedPreferencesRecord = serde_wasm_bindgen::from_value(v)
                        .map_err(|e| format!("Failed to deserialize preferences: {}", e))?;
                    let cached = CachedPreferences::from_record(record)?;
                    #[cfg(debug_assertions)]
                    crate::log!(
                        "[PreferenceCache::get] ✓ Deserialized: theme={}, units={}, language={}",
                        cached.theme,
                        cached.units,
                        cached.language
                    );
                    Ok(Some(cached))
                }
                None => {
                    #[cfg(debug_assertions)]
                    crate::log!(
                        "[PreferenceCache::get] No record found for user_id={}",
                        user_id
                    );
                    Ok(None)
                }
            }
        }
    }

    /// Upsert preferences to cache
    pub async fn upsert(
        db: &Database,
        user_id: Uuid,
        units: &str,
        theme: &str,
        language: &str,
        default_rest_seconds: u32,
    ) -> StoreResult<()> {
        if !(MIN_REST_SECONDS..=MAX_REST_SECONDS).contains(&default_rest_seconds) {
            return Err(format!(
                "default_rest_seconds {} is outside allowed range {}..={}",
                default_rest_seconds, MIN_REST_SECONDS, MAX_REST_SECONDS
            ));
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            use rusqlite::params;

            let conn = db.conn.lock().await;
            let user_id_str = user_id.to_string();
            let now = Utc::now().timestamp();
            let rest_seconds_i32 = i32::try_from(default_rest_seconds).map_err(|_| {
                format!(
                    "default_rest_seconds {} exceeds i32::MAX",
                    default_rest_seconds
                )
            })?;

            conn.execute(
                "INSERT INTO user_preferences_cache
                    (user_id, units, theme, language, default_rest_seconds, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                ON CONFLICT (user_id)
                DO UPDATE SET
                    units = excluded.units,
                    theme = excluded.theme,
                    language = excluded.language,
                    default_rest_seconds = excluded.default_rest_seconds,
                    updated_at = excluded.updated_at,
                    synced_at = NULL",
                params![user_id_str, units, theme, language, rest_seconds_i32, now],
            )
            .map_err(|e| format!("Failed to upsert user_preferences_cache: {}", e))?;

            Ok(())
        }

        #[cfg(target_arch = "wasm32")]
        {
            use rexie::TransactionMode;

            #[cfg(debug_assertions)]
            crate::log!("[PreferenceCache::upsert] Starting upsert for user_id={}, units={}, theme={}, language={}, rest={}",
                user_id, units, theme, language, default_rest_seconds);

            let tx = db
                .transaction(&["user_preferences_cache"], TransactionMode::ReadWrite)
                .map_err(|e| format!("Failed to create transaction: {:?}", e))?;
            #[cfg(debug_assertions)]
            crate::log!("[PreferenceCache::upsert] Transaction created");

            let store = tx
                .store("user_preferences_cache")
                .map_err(|e| format!("Failed to get store: {:?}", e))?;
            #[cfg(debug_assertions)]
            crate::log!("[PreferenceCache::upsert] Store opened");

            let prefs = CachedPreferences {
                user_id: user_id.to_string(),
                units: units.to_string(),
                theme: theme.to_string(),
                language: language.to_string(),
                default_rest_seconds,
                updated_at: Utc::now(),
                synced_at: None,
            };
            let record = prefs.to_record();

            let value = serde_wasm_bindgen::to_value(&record)
                .map_err(|e| format!("Failed to serialize preferences: {}", e))?;
            #[cfg(debug_assertions)]
            crate::log!("[PreferenceCache::upsert] Serialized record");

            store
                .put(&value, None)
                .await
                .map_err(|e| format!("Failed to put preferences: {:?}", e))?;
            #[cfg(debug_assertions)]
            crate::log!("[PreferenceCache::upsert] Put successful, committing...");

            tx.commit()
                .await
                .map_err(|e| format!("Failed to commit transaction: {:?}", e))?;
            #[cfg(debug_assertions)]
            crate::log!("[PreferenceCache::upsert] ✓ Transaction committed successfully");

            Ok(())
        }
    }

    /// Mark preferences as synced to server
    pub async fn mark_synced(db: &Database, user_id: Uuid) -> StoreResult<()> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let conn = db.conn.lock().await;
            let user_id_str = user_id.to_string();
            let now = Utc::now().timestamp();

            conn.execute(
                "UPDATE user_preferences_cache SET synced_at = ?1 WHERE user_id = ?2",
                rusqlite::params![now, user_id_str],
            )
            .map_err(|e| format!("Failed to mark preferences synced: {}", e))?;

            Ok(())
        }

        #[cfg(target_arch = "wasm32")]
        {
            use rexie::TransactionMode;
            use wasm_bindgen::JsValue;

            let tx = db
                .transaction(&["user_preferences_cache"], TransactionMode::ReadWrite)
                .map_err(|e| format!("Failed to create transaction: {:?}", e))?;
            let store = tx
                .store("user_preferences_cache")
                .map_err(|e| format!("Failed to get store: {:?}", e))?;

            let key = JsValue::from_str(&user_id.to_string());
            let result = store
                .get(key.clone())
                .await
                .map_err(|e| format!("Failed to get preferences: {:?}", e))?;

            if let Some(value) = result {
                let record: CachedPreferencesRecord = serde_wasm_bindgen::from_value(value)
                    .map_err(|e| format!("Failed to deserialize preferences: {}", e))?;

                let mut cached = CachedPreferences::from_record(record)?;
                cached.synced_at = Some(Utc::now());

                let updated_record = cached.to_record();
                let updated_value = serde_wasm_bindgen::to_value(&updated_record)
                    .map_err(|e| format!("Failed to serialize preferences: {}", e))?;

                store
                    .put(&updated_value, None)
                    .await
                    .map_err(|e| format!("Failed to put preferences: {:?}", e))?;
            }

            tx.commit()
                .await
                .map_err(|e| format!("Failed to commit transaction: {:?}", e))?;

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::persistence::Database;

    #[test]
    fn cached_preferences_roundtrip_record_preserves_fields() {
        let prefs = CachedPreferences {
            user_id: Uuid::new_v4().to_string(),
            units: "kg".to_string(),
            theme: "dark".to_string(),
            language: "en".to_string(),
            default_rest_seconds: shared::DEFAULT_REST_SECONDS,
            updated_at: Utc::now(),
            synced_at: Some(Utc::now()),
        };

        let record = prefs.to_record();
        let restored = CachedPreferences::from_record(record).expect("record should parse");
        assert_eq!(restored.units, "kg");
        assert_eq!(restored.theme, "dark");
        assert_eq!(restored.language, "en");
        assert_eq!(restored.default_rest_seconds, shared::DEFAULT_REST_SECONDS);
        assert!(restored.synced_at.is_some());
    }

    #[tokio::test]
    async fn preference_cache_upsert_roundtrip_and_isolates_users() {
        let db = Database::new_in_memory().await.expect("in-memory db");
        let first_user = Uuid::new_v4();
        let second_user = Uuid::new_v4();

        assert!(PreferenceCache::get(&db, first_user)
            .await
            .expect("query missing first user")
            .is_none());

        PreferenceCache::upsert(
            &db,
            first_user,
            "kg",
            "dark",
            "en",
            shared::DEFAULT_REST_SECONDS,
        )
        .await
        .expect("insert first user preferences");
        PreferenceCache::upsert(&db, second_user, "lb", "athletic", "es", 120)
            .await
            .expect("insert second user preferences");
        PreferenceCache::upsert(&db, first_user, "lb", "athletic", "fr", 150)
            .await
            .expect("update first user preferences");

        let first = PreferenceCache::get(&db, first_user)
            .await
            .expect("load first user")
            .expect("first user preferences should exist");
        let second = PreferenceCache::get(&db, second_user)
            .await
            .expect("load second user")
            .expect("second user preferences should exist");

        assert_eq!(first.units, "lb");
        assert_eq!(first.theme, "athletic");
        assert_eq!(first.language, "fr");
        assert_eq!(first.default_rest_seconds, 150);

        assert_eq!(second.units, "lb");
        assert_eq!(second.theme, "athletic");
        assert_eq!(second.language, "es");
        assert_eq!(second.default_rest_seconds, 120);
        assert_ne!(first.user_id, second.user_id);
    }

    #[tokio::test]
    async fn preference_cache_upsert_rejects_out_of_range_rest_seconds_before_write() {
        let db = Database::new_in_memory().await.expect("in-memory db");
        let user_id = Uuid::new_v4();

        let err = PreferenceCache::upsert(
            &db,
            user_id,
            "kg",
            "athletic",
            "en",
            shared::MAX_REST_SECONDS + 1,
        )
        .await
        .expect_err("out-of-range rest seconds should be rejected");

        assert!(err.contains("outside allowed range"));
        assert!(PreferenceCache::get(&db, user_id)
            .await
            .expect("query after rejected write")
            .is_none());
    }
}

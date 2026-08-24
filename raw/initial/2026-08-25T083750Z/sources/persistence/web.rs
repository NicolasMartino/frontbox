//! Web IndexedDB storage backend (wasm32 only).

use rexie::{ObjectStore, Rexie, TransactionMode};
use shared::frontend::dto::{MutationId, MutationIntentDto};
use shared::{Exercise, ExerciseId, SessionId, TemplateId, WorkoutSession, WorkoutTemplate};
use wasm_bindgen::JsValue;

use super::types::{
    DeadLetterRecord, ExerciseRecord, OutboxRecord, SessionRecord, StoreResult, TemplateRecord,
};

const DB_NAME: &str = "repforge_app";
const DB_VERSION: u32 = 5; // Bumped for mutation intent outbox stores

/// Database wrapper for IndexedDB.
pub type Database = Rexie;

/// Helper to create a read-only transaction for a single store.
fn read_tx(db: &Rexie, store_name: &str) -> StoreResult<rexie::Transaction> {
    db.transaction(&[store_name], TransactionMode::ReadOnly)
        .map_err(|e| format!("Failed to create read transaction: {:?}", e))
}

/// Helper to create a read-write transaction for a single store.
fn write_tx(db: &Rexie, store_name: &str) -> StoreResult<rexie::Transaction> {
    db.transaction(&[store_name], TransactionMode::ReadWrite)
        .map_err(|e| format!("Failed to create write transaction: {:?}", e))
}

/// Generate a user-specific database name for data isolation.
fn db_name_for_user(user_id: &str) -> String {
    // Sanitize user_id for IndexedDB naming (most chars are allowed, but be safe)
    let safe_id = user_id.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    format!("repforge_{}", safe_id)
}

/// Helper to build an IndexedDB database with the standard schema.
async fn build_database(name: &str) -> StoreResult<Database> {
    Rexie::builder(name)
        .version(DB_VERSION)
        // Mutation sync stores
        .add_object_store(
            ObjectStore::new("mutation_outbox")
                .key_path("mutation_id")
                .add_index(rexie::Index::new("created_at", "created_at"))
                .add_index(rexie::Index::new("path", "path")),
        )
        .add_object_store(ObjectStore::new("mutation_dead_letter").key_path("mutation_id"))
        // Workout stores
        .add_object_store(
            ObjectStore::new("exercises")
                .key_path("id")
                .add_index(rexie::Index::new("owner_id", "owner_id"))
                .add_index(rexie::Index::new("name", "name")),
        )
        .add_object_store(
            ObjectStore::new("templates")
                .key_path("id")
                .add_index(rexie::Index::new("owner_id", "owner_id"))
                .add_index(rexie::Index::new("updated_at", "updated_at")),
        )
        .add_object_store(
            ObjectStore::new("workout_sessions")
                .key_path("id")
                .add_index(rexie::Index::new("owner_id", "owner_id"))
                .add_index(rexie::Index::new("status", "status"))
                .add_index(rexie::Index::new("created_at", "created_at")),
        )
        // Preference cache for offline support
        .add_object_store(
            ObjectStore::new("user_preferences_cache")
                .key_path("user_id")
                .add_index(rexie::Index::new("updated_at", "updated_at")),
        )
        .build()
        .await
        .map_err(|e| format!("Failed to initialize IndexedDB: {:?}", e))
}

/// Initialize the IndexedDB database (legacy, shared for all users).
pub async fn init_db() -> StoreResult<Database> {
    build_database(DB_NAME).await
}

/// Initialize the IndexedDB database for a specific user.
///
/// Creates or opens a user-specific database to ensure data isolation
/// between different users in the same browser.
pub async fn init_db_for_user(user_id: &str) -> StoreResult<Database> {
    let name = db_name_for_user(user_id);
    build_database(&name).await
}

/// Store operations for the mutation outbox.
pub struct OutboxStore;

impl OutboxStore {
    /// Get all pending mutation intents, ordered by created_at ascending (FIFO).
    pub async fn get_all(db: &Database) -> StoreResult<Vec<OutboxRecord>> {
        let tx = read_tx(db, "mutation_outbox")?;
        let store = tx
            .store("mutation_outbox")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let records = store
            .get_all(None, None)
            .await
            .map_err(|e| format!("Failed to get all outbox records: {:?}", e))?;

        let mut outbox: Vec<OutboxRecord> = records
            .into_iter()
            .filter_map(|value| serde_wasm_bindgen::from_value::<OutboxRecord>(value).ok())
            .collect();

        outbox.sort_by_key(|r| r.created_at);

        Ok(outbox)
    }

    /// Add a mutation intent to the outbox.
    pub async fn add(db: &Database, intent: &MutationIntentDto) -> StoreResult<bool> {
        let record = OutboxRecord::from_intent(intent)?;

        let tx = write_tx(db, "mutation_outbox")?;
        let store = tx
            .store("mutation_outbox")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let key = JsValue::from_str(&record.mutation_id);
        let existing = store
            .get(key)
            .await
            .map_err(|e| format!("Failed to check existing: {:?}", e))?;

        if existing.is_some() {
            return Ok(false);
        }

        let value = serde_wasm_bindgen::to_value(&record)
            .map_err(|e| format!("Failed to serialize outbox record: {}", e))?;

        store
            .put(&value, None)
            .await
            .map_err(|e| format!("Failed to put outbox record: {:?}", e))?;

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit: {:?}", e))?;

        Ok(true)
    }

    /// Delete a mutation from the outbox by mutation_id.
    pub async fn delete(db: &Database, mutation_id: &MutationId) -> StoreResult<()> {
        Self::delete_by_id(db, &mutation_id.as_str()).await
    }

    /// Delete a mutation from the outbox by mutation_id string.
    pub async fn delete_by_id(db: &Database, mutation_id: &str) -> StoreResult<()> {
        let tx = write_tx(db, "mutation_outbox")?;
        let store = tx
            .store("mutation_outbox")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let key = JsValue::from_str(mutation_id);
        store
            .delete(key)
            .await
            .map_err(|e| format!("Failed to delete outbox record: {:?}", e))?;

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit: {:?}", e))?;

        Ok(())
    }

    /// Count pending mutations in the outbox.
    pub async fn count(db: &Database) -> StoreResult<usize> {
        let tx = read_tx(db, "mutation_outbox")?;
        let store = tx
            .store("mutation_outbox")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let count = store
            .count(None)
            .await
            .map_err(|e| format!("Failed to count outbox: {:?}", e))?;

        Ok(count as usize)
    }

    /// Check if a mutation_id exists in the outbox.
    pub async fn exists(db: &Database, mutation_id: &str) -> StoreResult<bool> {
        let tx = read_tx(db, "mutation_outbox")?;
        let store = tx
            .store("mutation_outbox")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let key = JsValue::from_str(mutation_id);
        let existing = store
            .get(key)
            .await
            .map_err(|e| format!("Failed to check existing: {:?}", e))?;

        Ok(existing.is_some())
    }
}

/// Store operations for dead letters.
pub struct DeadLetterStore;

impl DeadLetterStore {
    /// Get all dead letter records.
    pub async fn get_all(db: &Database) -> StoreResult<Vec<DeadLetterRecord>> {
        let tx = read_tx(db, "mutation_dead_letter")?;
        let store = tx
            .store("mutation_dead_letter")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let records = store
            .get_all(None, None)
            .await
            .map_err(|e| format!("Failed to get all dead letters: {:?}", e))?;

        let dead_letters: Vec<DeadLetterRecord> = records
            .into_iter()
            .filter_map(|value| serde_wasm_bindgen::from_value::<DeadLetterRecord>(value).ok())
            .collect();

        Ok(dead_letters)
    }

    /// Add a rejected mutation to the dead letter store.
    pub async fn add(db: &Database, record: &DeadLetterRecord) -> StoreResult<()> {
        let tx = write_tx(db, "mutation_dead_letter")?;
        let store = tx
            .store("mutation_dead_letter")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let value = serde_wasm_bindgen::to_value(record)
            .map_err(|e| format!("Failed to serialize dead letter record: {}", e))?;

        store
            .put(&value, None)
            .await
            .map_err(|e| format!("Failed to put dead letter: {:?}", e))?;

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit: {:?}", e))?;

        Ok(())
    }

    /// Delete a dead letter by mutation_id.
    pub async fn delete(db: &Database, mutation_id: &str) -> StoreResult<()> {
        let tx = write_tx(db, "mutation_dead_letter")?;
        let store = tx
            .store("mutation_dead_letter")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let key = JsValue::from_str(mutation_id);
        store
            .delete(key)
            .await
            .map_err(|e| format!("Failed to delete dead letter: {:?}", e))?;

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit: {:?}", e))?;

        Ok(())
    }

    /// Clear all dead letters.
    pub async fn clear(db: &Database) -> StoreResult<()> {
        let tx = write_tx(db, "mutation_dead_letter")?;
        let store = tx
            .store("mutation_dead_letter")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        store
            .clear()
            .await
            .map_err(|e| format!("Failed to clear dead letters: {:?}", e))?;

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit: {:?}", e))?;

        Ok(())
    }

    /// Count dead letters.
    pub async fn count(db: &Database) -> StoreResult<usize> {
        let tx = read_tx(db, "mutation_dead_letter")?;
        let store = tx
            .store("mutation_dead_letter")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let count = store
            .count(None)
            .await
            .map_err(|e| format!("Failed to count dead letters: {:?}", e))?;

        Ok(count as usize)
    }

    /// Check if a dead letter record exists.
    pub async fn exists(db: &Database, mutation_id: &str) -> StoreResult<bool> {
        let tx = read_tx(db, "mutation_dead_letter")?;
        let store = tx
            .store("mutation_dead_letter")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let key = JsValue::from_str(mutation_id);
        let existing = store
            .get(key)
            .await
            .map_err(|e| format!("Failed to read dead letter: {:?}", e))?;

        Ok(existing.is_some())
    }
}

// ============================================================================
// Workout Stores
// ============================================================================

/// Store operations for exercises.
pub struct ExerciseStore;

impl ExerciseStore {
    /// Get all exercises.
    pub async fn get_all(db: &Database) -> StoreResult<Vec<Exercise>> {
        let tx = read_tx(db, "exercises")?;
        let store = tx
            .store("exercises")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let records = store
            .get_all(None, None)
            .await
            .map_err(|e| format!("Failed to get all exercises: {:?}", e))?;

        let mut exercises = Vec::new();
        for value in records {
            if let Ok(record) = serde_wasm_bindgen::from_value::<ExerciseRecord>(value) {
                match record.to_exercise() {
                    Ok(exercise) => exercises.push(exercise),
                    Err(e) => web_sys::console::warn_1(
                        &format!("Skipping corrupted exercise record: {}", e).into(),
                    ),
                }
            }
        }

        exercises.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(exercises)
    }

    /// Get a single exercise by ID.
    pub async fn get(db: &Database, id: &ExerciseId) -> StoreResult<Option<Exercise>> {
        let tx = read_tx(db, "exercises")?;
        let store = tx
            .store("exercises")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let key = JsValue::from_str(&id.to_string());
        let value = store
            .get(key)
            .await
            .map_err(|e| format!("Failed to get exercise: {:?}", e))?;

        match value {
            Some(v) => {
                let record: ExerciseRecord = serde_wasm_bindgen::from_value(v)
                    .map_err(|e| format!("Failed to deserialize exercise: {}", e))?;
                Ok(Some(record.to_exercise()?))
            }
            None => Ok(None),
        }
    }

    /// Insert or update an exercise.
    pub async fn put(db: &Database, exercise: &Exercise) -> StoreResult<()> {
        let tx = write_tx(db, "exercises")?;
        let store = tx
            .store("exercises")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let record = ExerciseRecord::from(exercise);
        let value = serde_wasm_bindgen::to_value(&record)
            .map_err(|e| format!("Failed to serialize exercise: {}", e))?;

        store
            .put(&value, None)
            .await
            .map_err(|e| format!("Failed to put exercise: {:?}", e))?;

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit: {:?}", e))?;

        Ok(())
    }

    /// Delete an exercise by ID.
    pub async fn delete(db: &Database, id: &ExerciseId) -> StoreResult<()> {
        let tx = write_tx(db, "exercises")?;
        let store = tx
            .store("exercises")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let key = JsValue::from_str(&id.to_string());
        store
            .delete(key)
            .await
            .map_err(|e| format!("Failed to delete exercise: {:?}", e))?;

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit: {:?}", e))?;

        Ok(())
    }

    /// Replace all exercises (used by eager refetch).
    ///
    /// SAFETY: If `exercises` is empty, this is a no-op to prevent accidental data deletion.
    /// The fetch functions may return empty if the API call fails or isn't implemented yet.
    pub async fn replace_all(db: &Database, exercises: &[Exercise]) -> StoreResult<()> {
        // Guard: Don't delete data if we have nothing to replace it with
        if exercises.is_empty() {
            crate::log!("[ExerciseStore] replace_all called with empty data, skipping to preserve local cache");
            return Ok(());
        }

        let tx = write_tx(db, "exercises")?;
        let store = tx
            .store("exercises")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        // Clear all existing exercises
        store
            .clear()
            .await
            .map_err(|e| format!("Failed to clear exercises: {:?}", e))?;

        // Insert all new exercises
        for exercise in exercises {
            let record = ExerciseRecord::from(exercise);
            let value = serde_wasm_bindgen::to_value(&record)
                .map_err(|e| format!("Failed to serialize exercise: {}", e))?;

            store
                .put(&value, None)
                .await
                .map_err(|e| format!("Failed to put exercise: {:?}", e))?;
        }

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit: {:?}", e))?;

        Ok(())
    }
}

/// Store operations for workout templates.
pub struct TemplateStore;

impl TemplateStore {
    /// Get all templates.
    pub async fn get_all(db: &Database) -> StoreResult<Vec<WorkoutTemplate>> {
        let tx = read_tx(db, "templates")?;
        let store = tx
            .store("templates")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let records = store
            .get_all(None, None)
            .await
            .map_err(|e| format!("Failed to get all templates: {:?}", e))?;

        let mut templates = Vec::new();
        for value in records {
            if let Ok(record) = serde_wasm_bindgen::from_value::<TemplateRecord>(value) {
                match record.to_template() {
                    Ok(template) => templates.push(template),
                    Err(e) => web_sys::console::warn_1(
                        &format!("Skipping corrupted template record: {}", e).into(),
                    ),
                }
            }
        }

        templates.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(templates)
    }

    /// Get a single template by ID.
    pub async fn get(db: &Database, id: &TemplateId) -> StoreResult<Option<WorkoutTemplate>> {
        let tx = read_tx(db, "templates")?;
        let store = tx
            .store("templates")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let key = JsValue::from_str(&id.to_string());
        let value = store
            .get(key)
            .await
            .map_err(|e| format!("Failed to get template: {:?}", e))?;

        match value {
            Some(v) => {
                let record: TemplateRecord = serde_wasm_bindgen::from_value(v)
                    .map_err(|e| format!("Failed to deserialize template: {}", e))?;
                Ok(Some(record.to_template()?))
            }
            None => Ok(None),
        }
    }

    /// Insert or update a template.
    pub async fn put(db: &Database, template: &WorkoutTemplate) -> StoreResult<()> {
        let tx = write_tx(db, "templates")?;
        let store = tx
            .store("templates")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let record = TemplateRecord::from(template);
        let value = serde_wasm_bindgen::to_value(&record)
            .map_err(|e| format!("Failed to serialize template: {}", e))?;

        store
            .put(&value, None)
            .await
            .map_err(|e| format!("Failed to put template: {:?}", e))?;

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit: {:?}", e))?;

        Ok(())
    }

    /// Delete a template by ID.
    pub async fn delete(db: &Database, id: &TemplateId) -> StoreResult<()> {
        let tx = write_tx(db, "templates")?;
        let store = tx
            .store("templates")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let key = JsValue::from_str(&id.to_string());
        store
            .delete(key)
            .await
            .map_err(|e| format!("Failed to delete template: {:?}", e))?;

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit: {:?}", e))?;

        Ok(())
    }

    /// Replace all templates (used by eager refetch).
    ///
    /// SAFETY: If `templates` is empty, this is a no-op to prevent accidental data deletion.
    /// The fetch functions may return empty if the API call fails or isn't implemented yet.
    pub async fn replace_all(db: &Database, templates: &[WorkoutTemplate]) -> StoreResult<()> {
        // Guard: Don't delete data if we have nothing to replace it with
        if templates.is_empty() {
            crate::log!("[TemplateStore] replace_all called with empty data, skipping to preserve local cache");
            return Ok(());
        }

        let tx = write_tx(db, "templates")?;
        let store = tx
            .store("templates")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        // Clear all existing templates
        store
            .clear()
            .await
            .map_err(|e| format!("Failed to clear templates: {:?}", e))?;

        // Insert all new templates
        for template in templates {
            let record = TemplateRecord::from(template);
            let value = serde_wasm_bindgen::to_value(&record)
                .map_err(|e| format!("Failed to serialize template: {}", e))?;

            store
                .put(&value, None)
                .await
                .map_err(|e| format!("Failed to put template: {:?}", e))?;
        }

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit: {:?}", e))?;

        Ok(())
    }
}

/// Store operations for workout sessions.
pub struct SessionStore;

impl SessionStore {
    /// Get all sessions, ordered by created_at descending.
    pub async fn get_all(db: &Database) -> StoreResult<Vec<WorkoutSession>> {
        let tx = read_tx(db, "workout_sessions")?;
        let store = tx
            .store("workout_sessions")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let records = store
            .get_all(None, None)
            .await
            .map_err(|e| format!("Failed to get all sessions: {:?}", e))?;

        let mut sessions = Vec::new();
        for value in records {
            if let Ok(record) = serde_wasm_bindgen::from_value::<SessionRecord>(value) {
                match record.to_session() {
                    Ok(session) => sessions.push(session),
                    Err(e) => web_sys::console::warn_1(
                        &format!("Skipping corrupted session record: {}", e).into(),
                    ),
                }
            }
        }

        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(sessions)
    }

    /// Get a single session by ID.
    pub async fn get(db: &Database, id: &SessionId) -> StoreResult<Option<WorkoutSession>> {
        let tx = read_tx(db, "workout_sessions")?;
        let store = tx
            .store("workout_sessions")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let key = JsValue::from_str(&id.to_string());
        let value = store
            .get(key)
            .await
            .map_err(|e| format!("Failed to get session: {:?}", e))?;

        match value {
            Some(v) => {
                let record: SessionRecord = serde_wasm_bindgen::from_value(v)
                    .map_err(|e| format!("Failed to deserialize session: {}", e))?;
                Ok(Some(record.to_session()?))
            }
            None => Ok(None),
        }
    }

    /// Insert or update a session.
    pub async fn put(db: &Database, session: &WorkoutSession) -> StoreResult<()> {
        let tx = write_tx(db, "workout_sessions")?;
        let store = tx
            .store("workout_sessions")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let record = SessionRecord::from(session);
        let value = serde_wasm_bindgen::to_value(&record)
            .map_err(|e| format!("Failed to serialize session: {}", e))?;

        store
            .put(&value, None)
            .await
            .map_err(|e| format!("Failed to put session: {:?}", e))?;

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit: {:?}", e))?;

        Ok(())
    }

    /// Delete a session by ID.
    pub async fn delete(db: &Database, id: &SessionId) -> StoreResult<()> {
        let tx = write_tx(db, "workout_sessions")?;
        let store = tx
            .store("workout_sessions")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        let key = JsValue::from_str(&id.to_string());
        store
            .delete(key)
            .await
            .map_err(|e| format!("Failed to delete session: {:?}", e))?;

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit: {:?}", e))?;

        Ok(())
    }

    /// Get the active (in-progress or paused) session, if any.
    pub async fn get_active(db: &Database) -> StoreResult<Option<WorkoutSession>> {
        let sessions = Self::get_all(db).await?;

        // Find the most recent active session
        Ok(sessions.into_iter().find(|s| {
            matches!(
                s.status,
                shared::SessionStatus::Active | shared::SessionStatus::Paused
            )
        }))
    }

    /// Replace all sessions (used by eager refetch).
    ///
    /// SAFETY: If `sessions` is empty, this is a no-op to prevent accidental data deletion.
    /// The fetch functions may return empty if the API call fails or isn't implemented yet.
    pub async fn replace_all(db: &Database, sessions: &[WorkoutSession]) -> StoreResult<()> {
        // Guard: Don't delete data if we have nothing to replace it with
        if sessions.is_empty() {
            crate::log!("[SessionStore] replace_all called with empty data, skipping to preserve local cache");
            return Ok(());
        }

        let tx = write_tx(db, "workout_sessions")?;
        let store = tx
            .store("workout_sessions")
            .map_err(|e| format!("Failed to get store: {:?}", e))?;

        // Clear all existing sessions
        store
            .clear()
            .await
            .map_err(|e| format!("Failed to clear sessions: {:?}", e))?;

        // Insert all new sessions
        for session in sessions {
            let record = SessionRecord::from(session);
            let value = serde_wasm_bindgen::to_value(&record)
                .map_err(|e| format!("Failed to serialize session: {}", e))?;

            store
                .put(&value, None)
                .await
                .map_err(|e| format!("Failed to put session: {:?}", e))?;
        }

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit: {:?}", e))?;

        Ok(())
    }
}

//! Native SQLite storage backend for desktop and mobile platforms.

use std::path::PathBuf;
use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension};
use shared::frontend::dto::{MutationId, MutationIntentDto};
use shared::{
    Exercise, ExerciseId, SessionId, TemplateId, WorkoutSession, WorkoutTemplate,
    DEFAULT_REST_SECONDS, MAX_REST_SECONDS, MIN_REST_SECONDS,
};
use tokio::sync::Mutex;

use super::types::{
    DeadLetterRecord, ExerciseRecord, OutboxRecord, SessionRecord, StoreResult, TemplateRecord,
};

/// SQLite database wrapper.
#[derive(Clone)]
pub struct Database {
    pub(super) conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish()
    }
}

impl Database {
    /// Get the app data directory for storing databases.
    fn app_dir() -> StoreResult<PathBuf> {
        #[cfg(target_os = "android")]
        {
            // On Android, use the app's internal data directory
            // This is typically /data/data/com.repforge.app/files/
            let path = std::env::var("HOME")
                .or_else(|_| std::env::var("TMPDIR"))
                .unwrap_or_else(|_| "/data/local/tmp".to_string());
            Ok(PathBuf::from(path))
        }

        #[cfg(not(target_os = "android"))]
        {
            let data_dir = dirs::data_local_dir()
                .ok_or_else(|| "Could not find local data directory".to_string())?;
            let app_dir = data_dir.join("com.repforge.app");
            std::fs::create_dir_all(&app_dir)
                .map_err(|e| format!("Failed to create app data directory: {}", e))?;
            Ok(app_dir)
        }
    }

    /// Get the database file path for the default/legacy database.
    fn db_path() -> StoreResult<PathBuf> {
        Ok(Self::app_dir()?.join("app.db"))
    }

    /// Get the database file path for a specific user.
    ///
    /// Uses the user_id to create an isolated database file per user.
    /// This ensures data isolation after logout/login cycles.
    fn db_path_for_user(user_id: &str) -> StoreResult<PathBuf> {
        // Sanitize user_id to be filesystem-safe (UUIDs are already safe)
        let safe_id = user_id.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        Ok(Self::app_dir()?.join(format!("{}.db", safe_id)))
    }

    /// Open or create the database (legacy, uses shared app.db).
    pub fn open() -> StoreResult<Self> {
        let path = Self::db_path()?;
        let conn = Connection::open(&path)
            .map_err(|e| format!("Failed to open database at {:?}: {}", path, e))?;

        // Initialize schema synchronously (rusqlite is sync)
        Self::init_schema(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open or create the database for a specific user.
    ///
    /// Each user gets their own database file to ensure data isolation.
    pub fn open_for_user(user_id: &str) -> StoreResult<Self> {
        let path = Self::db_path_for_user(user_id)?;
        let conn = Connection::open(&path).map_err(|e| {
            format!(
                "Failed to open database for user {} at {:?}: {}",
                user_id, path, e
            )
        })?;

        Self::init_schema(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Create an in-memory database for testing.
    ///
    /// This database will be destroyed when dropped.
    pub async fn new_in_memory() -> StoreResult<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to create in-memory database: {}", e))?;

        Self::init_schema(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Initialize database schema (synchronous).
    fn init_schema(conn: &Connection) -> StoreResult<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS mutation_outbox (
                mutation_id TEXT PRIMARY KEY,
                method TEXT NOT NULL,
                path TEXT NOT NULL,
                body TEXT NOT NULL,
                created_at INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| format!("Failed to create mutation_outbox table: {}", e))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS mutation_dead_letter (
                mutation_id TEXT PRIMARY KEY,
                method TEXT NOT NULL,
                path TEXT NOT NULL,
                body TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                rejected_at INTEGER NOT NULL,
                error TEXT
            )",
            [],
        )
        .map_err(|e| format!("Failed to create mutation_dead_letter table: {}", e))?;

        // Workout tables
        conn.execute(
            "CREATE TABLE IF NOT EXISTS exercises (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                muscle_groups TEXT NOT NULL DEFAULT '[]',
                equipment TEXT NOT NULL,
                is_custom INTEGER NOT NULL DEFAULT 0,
                is_favorite INTEGER NOT NULL DEFAULT 0,
                owner_id TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| format!("Failed to create exercises table: {}", e))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS templates (
                id TEXT PRIMARY KEY,
                owner_id TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT,
                exercises TEXT NOT NULL DEFAULT '[]',
                estimated_minutes INTEGER,
                is_favorite INTEGER NOT NULL DEFAULT 0,
                tags TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| format!("Failed to create templates table: {}", e))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS workout_sessions (
                id TEXT PRIMARY KEY,
                owner_id TEXT NOT NULL,
                template_id TEXT,
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                exercises TEXT NOT NULL DEFAULT '[]',
                started_at INTEGER NOT NULL,
                paused_at INTEGER,
                total_paused_seconds INTEGER NOT NULL DEFAULT 0,
                notes TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| format!("Failed to create workout_sessions table: {}", e))?;

        // Create indexes for common queries
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_exercises_owner ON exercises(owner_id)",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_templates_owner ON templates(owner_id)",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sessions_owner ON workout_sessions(owner_id)",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sessions_status ON workout_sessions(status)",
            [],
        );

        // User preferences cache table.
        // Keep runtime schema bounds aligned with shared contracts constants.
        let user_preferences_schema = format!(
            "CREATE TABLE IF NOT EXISTS user_preferences_cache (
                user_id TEXT PRIMARY KEY,
                units TEXT NOT NULL DEFAULT 'kg',
                theme TEXT NOT NULL DEFAULT 'athletic',
                language TEXT NOT NULL DEFAULT 'en',
                default_rest_seconds INTEGER NOT NULL DEFAULT {} CHECK (default_rest_seconds BETWEEN {} AND {}),
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                synced_at INTEGER
            )",
            DEFAULT_REST_SECONDS, MIN_REST_SECONDS, MAX_REST_SECONDS
        );
        conn.execute(&user_preferences_schema, [])
            .map_err(|e| format!("Failed to create user_preferences_cache table: {}", e))?;

        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_user_prefs_cache_updated ON user_preferences_cache(updated_at)",
            [],
        );

        Ok(())
    }
}

/// Store operations for the mutation outbox.
pub struct OutboxStore;

impl OutboxStore {
    /// Get all pending mutation intents, ordered by created_at ascending (FIFO).
    pub async fn get_all(db: &Database) -> StoreResult<Vec<OutboxRecord>> {
        let conn = db.conn.lock().await;

        let mut stmt = conn
            .prepare(
                "SELECT mutation_id, method, path, body, created_at FROM mutation_outbox ORDER BY created_at ASC",
            )
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let records = stmt
            .query_map([], |row| {
                Ok(OutboxRecord {
                    mutation_id: row.get(0)?,
                    method: row.get(1)?,
                    path: row.get(2)?,
                    body: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| format!("Failed to query outbox: {}", e))?;

        Ok(records.filter_map(|r| r.ok()).collect())
    }

    /// Add a mutation intent to the outbox.
    pub async fn add(db: &Database, intent: &MutationIntentDto) -> StoreResult<bool> {
        let record = OutboxRecord::from_intent(intent)?;
        let conn = db.conn.lock().await;

        // Check if already exists
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM mutation_outbox WHERE mutation_id = ?",
                [&record.mutation_id],
                |_| Ok(true),
            )
            .optional()
            .map_err(|e| format!("Failed to check existing: {}", e))?
            .unwrap_or(false);

        if exists {
            return Ok(false);
        }

        conn.execute(
            "INSERT INTO mutation_outbox (mutation_id, method, path, body, created_at) VALUES (?, ?, ?, ?, ?)",
            params![
                record.mutation_id,
                record.method,
                record.path,
                record.body,
                record.created_at
            ],
        )
        .map_err(|e| format!("Failed to add to outbox: {}", e))?;

        Ok(true)
    }

    /// Delete a mutation from the outbox by mutation_id.
    pub async fn delete(db: &Database, mutation_id: &MutationId) -> StoreResult<()> {
        Self::delete_by_id(db, &mutation_id.as_str()).await
    }

    /// Delete a mutation from the outbox by mutation_id string.
    pub async fn delete_by_id(db: &Database, mutation_id: &str) -> StoreResult<()> {
        let conn = db.conn.lock().await;

        conn.execute(
            "DELETE FROM mutation_outbox WHERE mutation_id = ?",
            [mutation_id],
        )
        .map_err(|e| format!("Failed to delete from outbox: {}", e))?;

        Ok(())
    }

    /// Count pending mutations in the outbox.
    pub async fn count(db: &Database) -> StoreResult<usize> {
        let conn = db.conn.lock().await;

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM mutation_outbox", [], |row| row.get(0))
            .map_err(|e| format!("Failed to count outbox: {}", e))?;

        Ok(count as usize)
    }

    /// Check if a mutation_id exists in the outbox.
    pub async fn exists(db: &Database, mutation_id: &str) -> StoreResult<bool> {
        let conn = db.conn.lock().await;

        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM mutation_outbox WHERE mutation_id = ?",
                [mutation_id],
                |_| Ok(true),
            )
            .optional()
            .map_err(|e| format!("Failed to check existing: {}", e))?
            .unwrap_or(false);

        Ok(exists)
    }
}

/// Store operations for dead-lettered mutations.
pub struct DeadLetterStore;

impl DeadLetterStore {
    /// Get all dead letter records.
    pub async fn get_all(db: &Database) -> StoreResult<Vec<DeadLetterRecord>> {
        let conn = db.conn.lock().await;

        let mut stmt = conn
            .prepare(
                "SELECT mutation_id, method, path, body, created_at, rejected_at, error FROM mutation_dead_letter",
            )
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let records = stmt
            .query_map([], |row| {
                Ok(DeadLetterRecord {
                    mutation_id: row.get(0)?,
                    method: row.get(1)?,
                    path: row.get(2)?,
                    body: row.get(3)?,
                    created_at: row.get(4)?,
                    rejected_at: row.get(5)?,
                    error: row.get(6)?,
                })
            })
            .map_err(|e| format!("Failed to query dead letters: {}", e))?;

        Ok(records.filter_map(|r| r.ok()).collect())
    }

    /// Add a rejected mutation to the dead letter store.
    pub async fn add(db: &Database, record: &DeadLetterRecord) -> StoreResult<()> {
        let conn = db.conn.lock().await;

        conn.execute(
            "INSERT OR REPLACE INTO mutation_dead_letter (mutation_id, method, path, body, created_at, rejected_at, error) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                record.mutation_id,
                record.method,
                record.path,
                record.body,
                record.created_at,
                record.rejected_at,
                record.error
            ],
        )
        .map_err(|e| format!("Failed to add dead letter: {}", e))?;

        Ok(())
    }

    /// Delete a dead letter by mutation_id.
    pub async fn delete(db: &Database, mutation_id: &str) -> StoreResult<()> {
        let conn = db.conn.lock().await;

        conn.execute(
            "DELETE FROM mutation_dead_letter WHERE mutation_id = ?",
            [mutation_id],
        )
        .map_err(|e| format!("Failed to delete dead letter: {}", e))?;

        Ok(())
    }

    /// Clear all dead letters.
    pub async fn clear(db: &Database) -> StoreResult<()> {
        let conn = db.conn.lock().await;

        conn.execute("DELETE FROM mutation_dead_letter", [])
            .map_err(|e| format!("Failed to clear dead letters: {}", e))?;

        Ok(())
    }

    /// Count dead letters.
    pub async fn count(db: &Database) -> StoreResult<usize> {
        let conn = db.conn.lock().await;

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM mutation_dead_letter", [], |row| {
                row.get(0)
            })
            .map_err(|e| format!("Failed to count dead letters: {}", e))?;

        Ok(count as usize)
    }

    /// Check if a dead letter record exists.
    pub async fn exists(db: &Database, mutation_id: &str) -> StoreResult<bool> {
        let conn = db.conn.lock().await;

        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM mutation_dead_letter WHERE mutation_id = ?",
                [mutation_id],
                |_| Ok(true),
            )
            .optional()
            .map_err(|e| format!("Failed to check dead letter existence: {}", e))?
            .unwrap_or(false);

        Ok(exists)
    }
}

// ============================================================================
// Workout Stores
// ============================================================================

/// Store operations for exercises.
pub struct ExerciseStore;

impl ExerciseStore {
    /// Get all exercises (built-in + custom for user).
    pub async fn get_all(db: &Database) -> StoreResult<Vec<Exercise>> {
        let conn = db.conn.lock().await;

        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, muscle_groups, equipment, \
                 is_custom, is_favorite, owner_id, created_at, updated_at \
                 FROM exercises ORDER BY name ASC",
            )
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let records = stmt
            .query_map([], |row| {
                Ok(ExerciseRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    muscle_groups: row.get(3)?,
                    equipment: row.get(4)?,
                    is_custom: row.get::<_, i32>(5)? != 0,
                    is_favorite: row.get::<_, i32>(6)? != 0,
                    owner_id: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })
            .map_err(|e| format!("Failed to query exercises: {}", e))?;

        let mut exercises = Vec::new();
        for record in records.filter_map(|r| r.ok()) {
            match record.to_exercise() {
                Ok(exercise) => exercises.push(exercise),
                Err(e) => eprintln!("[warn] Skipping corrupted exercise record: {}", e),
            }
        }

        Ok(exercises)
    }

    /// Get a single exercise by ID.
    pub async fn get(db: &Database, id: &ExerciseId) -> StoreResult<Option<Exercise>> {
        let conn = db.conn.lock().await;

        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, muscle_groups, equipment, \
                 is_custom, is_favorite, owner_id, created_at, updated_at \
                 FROM exercises WHERE id = ?",
            )
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let result: Option<ExerciseRecord> = stmt
            .query_row([id.to_string()], |row| {
                Ok(ExerciseRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    muscle_groups: row.get(3)?,
                    equipment: row.get(4)?,
                    is_custom: row.get::<_, i32>(5)? != 0,
                    is_favorite: row.get::<_, i32>(6)? != 0,
                    owner_id: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })
            .optional()
            .map_err(|e| format!("Failed to get exercise: {}", e))?;

        match result {
            Some(r) => Ok(Some(r.to_exercise()?)),
            None => Ok(None),
        }
    }

    /// Insert or update an exercise.
    pub async fn put(db: &Database, exercise: &Exercise) -> StoreResult<()> {
        let conn = db.conn.lock().await;
        let record = ExerciseRecord::from(exercise);

        conn.execute(
            "INSERT OR REPLACE INTO exercises \
             (id, name, description, muscle_groups, equipment, is_custom, is_favorite, owner_id, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                record.id,
                record.name,
                record.description,
                record.muscle_groups,
                record.equipment,
                record.is_custom as i32,
                record.is_favorite as i32,
                record.owner_id,
                record.created_at,
                record.updated_at
            ],
        )
        .map_err(|e| format!("Failed to put exercise: {}", e))?;

        Ok(())
    }

    /// Delete an exercise by ID.
    pub async fn delete(db: &Database, id: &ExerciseId) -> StoreResult<()> {
        let conn = db.conn.lock().await;

        conn.execute("DELETE FROM exercises WHERE id = ?", [id.to_string()])
            .map_err(|e| format!("Failed to delete exercise: {}", e))?;

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

        let conn = db.conn.lock().await;

        // Delete all existing exercises
        conn.execute("DELETE FROM exercises", [])
            .map_err(|e| format!("Failed to clear exercises: {}", e))?;

        // Insert all new exercises
        for exercise in exercises {
            let record = ExerciseRecord::from(exercise);
            conn.execute(
                "INSERT INTO exercises \
                 (id, name, description, muscle_groups, equipment, is_custom, is_favorite, owner_id, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    record.id,
                    record.name,
                    record.description,
                    record.muscle_groups,
                    record.equipment,
                    record.is_custom as i32,
                    record.is_favorite as i32,
                    record.owner_id,
                    record.created_at,
                    record.updated_at
                ],
            )
            .map_err(|e| format!("Failed to insert exercise: {}", e))?;
        }

        Ok(())
    }
}

/// Store operations for workout templates.
pub struct TemplateStore;

impl TemplateStore {
    /// Get all templates for the user.
    pub async fn get_all(db: &Database) -> StoreResult<Vec<WorkoutTemplate>> {
        let conn = db.conn.lock().await;

        let mut stmt = conn
            .prepare(
                "SELECT id, owner_id, name, description, exercises, estimated_minutes, \
                 is_favorite, tags, created_at, updated_at \
                 FROM templates ORDER BY updated_at DESC",
            )
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let records = stmt
            .query_map([], |row| {
                Ok(TemplateRecord {
                    id: row.get(0)?,
                    owner_id: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    exercises: row.get(4)?,
                    estimated_minutes: row.get(5)?,
                    is_favorite: row.get::<_, i32>(6)? != 0,
                    tags: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })
            .map_err(|e| format!("Failed to query templates: {}", e))?;

        let mut templates = Vec::new();
        for record in records.filter_map(|r| r.ok()) {
            match record.to_template() {
                Ok(template) => templates.push(template),
                Err(e) => eprintln!("[warn] Skipping corrupted template record: {}", e),
            }
        }

        Ok(templates)
    }

    /// Get a single template by ID.
    pub async fn get(db: &Database, id: &TemplateId) -> StoreResult<Option<WorkoutTemplate>> {
        let conn = db.conn.lock().await;

        let mut stmt = conn
            .prepare(
                "SELECT id, owner_id, name, description, exercises, estimated_minutes, \
                 is_favorite, tags, created_at, updated_at \
                 FROM templates WHERE id = ?",
            )
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let result: Option<TemplateRecord> = stmt
            .query_row([id.to_string()], |row| {
                Ok(TemplateRecord {
                    id: row.get(0)?,
                    owner_id: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    exercises: row.get(4)?,
                    estimated_minutes: row.get(5)?,
                    is_favorite: row.get::<_, i32>(6)? != 0,
                    tags: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })
            .optional()
            .map_err(|e| format!("Failed to get template: {}", e))?;

        match result {
            Some(r) => Ok(Some(r.to_template()?)),
            None => Ok(None),
        }
    }

    /// Insert or update a template.
    pub async fn put(db: &Database, template: &WorkoutTemplate) -> StoreResult<()> {
        let conn = db.conn.lock().await;
        let record = TemplateRecord::from(template);

        conn.execute(
            "INSERT OR REPLACE INTO templates \
             (id, owner_id, name, description, exercises, estimated_minutes, is_favorite, tags, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                record.id,
                record.owner_id,
                record.name,
                record.description,
                record.exercises,
                record.estimated_minutes,
                record.is_favorite as i32,
                record.tags,
                record.created_at,
                record.updated_at
            ],
        )
        .map_err(|e| format!("Failed to put template: {}", e))?;

        Ok(())
    }

    /// Delete a template by ID.
    pub async fn delete(db: &Database, id: &TemplateId) -> StoreResult<()> {
        let conn = db.conn.lock().await;

        conn.execute("DELETE FROM templates WHERE id = ?", [id.to_string()])
            .map_err(|e| format!("Failed to delete template: {}", e))?;

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

        let conn = db.conn.lock().await;

        // Delete all existing templates
        conn.execute("DELETE FROM templates", [])
            .map_err(|e| format!("Failed to clear templates: {}", e))?;

        // Insert all new templates
        for template in templates {
            let record = TemplateRecord::from(template);
            conn.execute(
                "INSERT INTO templates \
                 (id, owner_id, name, description, exercises, estimated_minutes, is_favorite, tags, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    record.id,
                    record.owner_id,
                    record.name,
                    record.description,
                    record.exercises,
                    record.estimated_minutes,
                    record.is_favorite as i32,
                    record.tags,
                    record.created_at,
                    record.updated_at
                ],
            )
            .map_err(|e| format!("Failed to insert template: {}", e))?;
        }

        Ok(())
    }
}

/// Store operations for workout sessions.
pub struct SessionStore;

impl SessionStore {
    /// Get all sessions, ordered by created_at descending.
    pub async fn get_all(db: &Database) -> StoreResult<Vec<WorkoutSession>> {
        let conn = db.conn.lock().await;

        let mut stmt = conn
            .prepare(
                "SELECT id, owner_id, template_id, name, status, exercises, \
                 started_at, paused_at, total_paused_seconds, notes, created_at, updated_at \
                 FROM workout_sessions ORDER BY created_at DESC",
            )
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let records = stmt
            .query_map([], |row| {
                Ok(SessionRecord {
                    id: row.get(0)?,
                    owner_id: row.get(1)?,
                    template_id: row.get(2)?,
                    name: row.get(3)?,
                    status: row.get(4)?,
                    exercises: row.get(5)?,
                    started_at: row.get(6)?,
                    paused_at: row.get(7)?,
                    total_paused_seconds: row.get(8)?,
                    notes: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            })
            .map_err(|e| format!("Failed to query sessions: {}", e))?;

        let mut sessions = Vec::new();
        for record in records.filter_map(|r| r.ok()) {
            match record.to_session() {
                Ok(session) => sessions.push(session),
                Err(e) => eprintln!("[warn] Skipping corrupted session record: {}", e),
            }
        }

        Ok(sessions)
    }

    /// Get a single session by ID.
    pub async fn get(db: &Database, id: &SessionId) -> StoreResult<Option<WorkoutSession>> {
        let conn = db.conn.lock().await;

        let mut stmt = conn
            .prepare(
                "SELECT id, owner_id, template_id, name, status, exercises, \
                 started_at, paused_at, total_paused_seconds, notes, created_at, updated_at \
                 FROM workout_sessions WHERE id = ?",
            )
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let result: Option<SessionRecord> = stmt
            .query_row([id.to_string()], |row| {
                Ok(SessionRecord {
                    id: row.get(0)?,
                    owner_id: row.get(1)?,
                    template_id: row.get(2)?,
                    name: row.get(3)?,
                    status: row.get(4)?,
                    exercises: row.get(5)?,
                    started_at: row.get(6)?,
                    paused_at: row.get(7)?,
                    total_paused_seconds: row.get(8)?,
                    notes: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            })
            .optional()
            .map_err(|e| format!("Failed to get session: {}", e))?;

        match result {
            Some(r) => Ok(Some(r.to_session()?)),
            None => Ok(None),
        }
    }

    /// Insert or update a session.
    pub async fn put(db: &Database, session: &WorkoutSession) -> StoreResult<()> {
        let conn = db.conn.lock().await;
        let record = SessionRecord::from(session);

        conn.execute(
            "INSERT OR REPLACE INTO workout_sessions \
             (id, owner_id, template_id, name, status, exercises, started_at, paused_at, total_paused_seconds, notes, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                record.id,
                record.owner_id,
                record.template_id,
                record.name,
                record.status,
                record.exercises,
                record.started_at,
                record.paused_at,
                record.total_paused_seconds,
                record.notes,
                record.created_at,
                record.updated_at
            ],
        )
        .map_err(|e| format!("Failed to put session: {}", e))?;

        Ok(())
    }

    /// Delete a session by ID.
    pub async fn delete(db: &Database, id: &SessionId) -> StoreResult<()> {
        let conn = db.conn.lock().await;

        conn.execute(
            "DELETE FROM workout_sessions WHERE id = ?",
            [id.to_string()],
        )
        .map_err(|e| format!("Failed to delete session: {}", e))?;

        Ok(())
    }

    /// Get the active (in-progress or paused) session, if any.
    pub async fn get_active(db: &Database) -> StoreResult<Option<WorkoutSession>> {
        let conn = db.conn.lock().await;

        let mut stmt = conn
            .prepare(
                "SELECT id, owner_id, template_id, name, status, exercises, \
                 started_at, paused_at, total_paused_seconds, notes, created_at, updated_at \
                 FROM workout_sessions \
                 WHERE status IN ('\"active\"', '\"paused\"') \
                 ORDER BY updated_at DESC LIMIT 1",
            )
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let result: Option<SessionRecord> = stmt
            .query_row([], |row| {
                Ok(SessionRecord {
                    id: row.get(0)?,
                    owner_id: row.get(1)?,
                    template_id: row.get(2)?,
                    name: row.get(3)?,
                    status: row.get(4)?,
                    exercises: row.get(5)?,
                    started_at: row.get(6)?,
                    paused_at: row.get(7)?,
                    total_paused_seconds: row.get(8)?,
                    notes: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            })
            .optional()
            .map_err(|e| format!("Failed to get active session: {}", e))?;

        match result {
            Some(r) => Ok(Some(r.to_session()?)),
            None => Ok(None),
        }
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

        let conn = db.conn.lock().await;

        // Delete all existing sessions
        conn.execute("DELETE FROM workout_sessions", [])
            .map_err(|e| format!("Failed to clear sessions: {}", e))?;

        // Insert all new sessions
        for session in sessions {
            let record = SessionRecord::from(session);
            conn.execute(
                "INSERT INTO workout_sessions \
                 (id, owner_id, template_id, name, status, exercises, started_at, paused_at, total_paused_seconds, notes, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    record.id,
                    record.owner_id,
                    record.template_id,
                    record.name,
                    record.status,
                    record.exercises,
                    record.started_at,
                    record.paused_at,
                    record.total_paused_seconds,
                    record.notes,
                    record.created_at,
                    record.updated_at
                ],
            )
            .map_err(|e| format!("Failed to insert session: {}", e))?;
        }

        Ok(())
    }
}

/// Initialize the database (legacy, shared for all users).
pub fn init_db() -> StoreResult<Database> {
    Database::open()
}

/// Initialize the database for a specific user.
///
/// Creates or opens a user-specific database file to ensure data isolation
/// between different users on the same device.
pub fn init_db_for_user(user_id: &str) -> StoreResult<Database> {
    Database::open_for_user(user_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::frontend::dto::{ApiError, MutationId, MutationIntentDto};
    use shared::{
        Equipment, Exercise, ExerciseId, LoggedSet, MuscleGroup, SessionExercise, SessionId, SetId,
        TemplateExercise, TemplateId, UserId, Visibility, WorkoutSession, WorkoutTemplate,
    };
    use uuid::Uuid;

    fn sample_user_id() -> UserId {
        UserId::from_uuid(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440099").unwrap())
    }

    fn sample_exercise(name: &str) -> Exercise {
        let mut exercise = Exercise::new_custom(
            ExerciseId::from_uuid(Uuid::new_v4()),
            sample_user_id(),
            name.to_string(),
            vec![MuscleGroup::Quadriceps, MuscleGroup::Glutes],
            Equipment::Barbell,
        );
        exercise.description = Some(format!("{name} description"));
        exercise.is_favorite = true;
        exercise.visibility = Visibility::Private;
        exercise
    }

    fn sample_template(name: &str, exercise_id: ExerciseId) -> WorkoutTemplate {
        let mut template = WorkoutTemplate::new(
            TemplateId::from_uuid(Uuid::new_v4()),
            sample_user_id(),
            name.to_string(),
        );
        template.description = Some(format!("{name} description"));
        template.estimated_minutes = Some(42);
        template.is_favorite = true;
        template.tags = vec!["legs".to_string(), "strength".to_string()];
        template.add_exercise(
            TemplateExercise::new(exercise_id, 4, "6-8".to_string(), 0)
                .with_rest(120)
                .with_notes("pause on each rep".to_string()),
        );
        template
    }

    fn sample_session(
        name: &str,
        exercise: &Exercise,
        template_id: Option<TemplateId>,
    ) -> WorkoutSession {
        let mut session = WorkoutSession::new(
            SessionId::from_uuid(Uuid::new_v4()),
            sample_user_id(),
            name.to_string(),
            template_id,
        );
        let mut session_exercise =
            SessionExercise::new(exercise.id.clone(), exercise.name.clone(), 0)
                .with_targets(3, "8-10".to_string(), Some(100.0))
                .with_rest(90)
                .with_notes("controlled tempo".to_string());
        session_exercise.log_set(LoggedSet::new_weight_reps(
            SetId::from_uuid(Uuid::new_v4()),
            1,
            100.0,
            8,
        ));
        session.add_exercise(session_exercise);
        session.notes = Some("session note".to_string());
        session
    }

    #[test]
    fn preferences_cache_schema_uses_shared_rest_bounds_constants() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        Database::init_schema(&conn).expect("schema should initialize");

        let schema: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'user_preferences_cache'",
                [],
                |row| row.get(0),
            )
            .expect("user_preferences_cache schema should exist");

        assert!(schema.contains(&format!("DEFAULT {}", DEFAULT_REST_SECONDS)));
        assert!(schema.contains(&format!(
            "BETWEEN {} AND {}",
            MIN_REST_SECONDS, MAX_REST_SECONDS
        )));
    }

    #[test]
    fn preferences_cache_migration_bounds_match_shared_constants() {
        let migration_sql = include_str!(
            "../../../migrations/006_enforce_user_preferences_cache_rest_seconds_bounds.sql"
        );

        assert!(migration_sql.contains(&format!("DEFAULT {}", DEFAULT_REST_SECONDS)));
        assert!(migration_sql.contains(&format!(
            "BETWEEN {} AND {}",
            MIN_REST_SECONDS, MAX_REST_SECONDS
        )));
    }

    #[tokio::test]
    async fn preferences_cache_schema_applies_defaults_on_direct_insert() {
        let db = Database::new_in_memory().await.expect("in-memory db");
        let user_id = Uuid::new_v4().to_string();

        let conn = db.conn.lock().await;
        conn.execute(
            "INSERT INTO user_preferences_cache (user_id) VALUES (?1)",
            params![user_id.clone()],
        )
        .expect("insert with defaults should succeed");

        let row: (String, String, String, i32) = conn
            .query_row(
                "SELECT units, theme, language, default_rest_seconds
                 FROM user_preferences_cache
                 WHERE user_id = ?1",
                params![user_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("preferences row should exist");

        assert_eq!(row.0, "kg");
        assert_eq!(row.1, "athletic");
        assert_eq!(row.2, "en");
        assert_eq!(row.3, DEFAULT_REST_SECONDS as i32);
    }

    #[tokio::test]
    async fn preferences_cache_schema_enforces_rest_seconds_bounds() {
        let db = Database::new_in_memory().await.expect("in-memory db");

        let conn = db.conn.lock().await;
        let insert_rest_seconds = |user_id: String, value: i32| {
            conn.execute(
                "INSERT INTO user_preferences_cache (user_id, default_rest_seconds)
                 VALUES (?1, ?2)",
                params![user_id, value],
            )
        };

        assert!(
            insert_rest_seconds(Uuid::new_v4().to_string(), MIN_REST_SECONDS as i32).is_ok(),
            "minimum rest_seconds should be accepted by schema"
        );
        assert!(
            insert_rest_seconds(Uuid::new_v4().to_string(), MAX_REST_SECONDS as i32).is_ok(),
            "maximum rest_seconds should be accepted by schema"
        );
        assert!(
            insert_rest_seconds(Uuid::new_v4().to_string(), MIN_REST_SECONDS as i32 - 1).is_err(),
            "below-minimum rest_seconds should be rejected by schema"
        );
        assert!(
            insert_rest_seconds(Uuid::new_v4().to_string(), MAX_REST_SECONDS as i32 + 1).is_err(),
            "above-maximum rest_seconds should be rejected by schema"
        );
    }

    #[test]
    fn db_path_for_user_sanitizes_reserved_path_characters() {
        let path =
            Database::db_path_for_user("user:/with*bad?chars|\\\\<>\"").expect("sanitized db path");
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .expect("db path should have a file name");
        assert_eq!(file_name, "user__with_bad_chars______.db");
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("db"),
            "sanitized database path should preserve the sqlite extension"
        );
        assert!(
            path.parent().is_some(),
            "database path should remain rooted under the app data directory"
        );
        for forbidden in [':', '/', '*', '?', '|', '\\', '<', '>', '"'] {
            assert!(
                !file_name.contains(forbidden),
                "sanitized file name still contains forbidden character '{}': {}",
                forbidden,
                file_name
            );
        }

        let empty_path = Database::db_path_for_user("").expect("empty user id db path");
        let empty_file_name = empty_path
            .file_name()
            .and_then(|value| value.to_str())
            .expect("empty-user db path should still have a file name");
        assert_eq!(
            empty_file_name, ".db",
            "empty user ids should not escape the app data directory or lose sqlite extension"
        );
        assert_eq!(
            empty_path.parent(),
            path.parent(),
            "empty and non-empty user db paths should share the app data directory"
        );
    }

    #[tokio::test]
    async fn outbox_store_roundtrip_covers_fifo_and_duplicate_guard() {
        let db = Database::new_in_memory().await.expect("in-memory db");
        let mut first = MutationIntentDto::new(
            MutationId::from_uuid(Uuid::new_v4()),
            "POST",
            "/api/v1/preferences",
            serde_json::json!({"theme": "athletic"}),
        );
        let mut second = MutationIntentDto::new(
            MutationId::from_uuid(Uuid::new_v4()),
            "PATCH",
            "/api/v1/exercises/123",
            serde_json::json!({"name": "Updated"}),
        );
        first.client_datetime = chrono::Utc::now() - chrono::TimeDelta::seconds(10);
        second.client_datetime = chrono::Utc::now();

        assert!(OutboxStore::add(&db, &first)
            .await
            .expect("add first intent"));
        assert!(OutboxStore::exists(&db, &first.mutation_id.as_str())
            .await
            .expect("exists first"));
        assert_eq!(OutboxStore::count(&db).await.expect("count after first"), 1);
        assert!(!OutboxStore::add(&db, &first)
            .await
            .expect("duplicate insert should be ignored"));
        assert!(OutboxStore::add(&db, &second)
            .await
            .expect("add second intent"));

        let records = OutboxStore::get_all(&db).await.expect("load outbox");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].mutation_id, first.mutation_id.as_str());
        assert_eq!(records[1].mutation_id, second.mutation_id.as_str());
        let restored_first = records[0].to_intent().expect("rehydrate first");
        let restored_second = records[1].to_intent().expect("rehydrate second");
        assert_eq!(restored_first.method, "POST");
        assert_eq!(restored_first.path, "/api/v1/preferences");
        assert_eq!(
            restored_first.body,
            serde_json::json!({"theme": "athletic"})
        );
        assert_eq!(restored_second.method, "PATCH");
        assert_eq!(restored_second.path, "/api/v1/exercises/123");
        assert_eq!(restored_second.body, serde_json::json!({"name": "Updated"}));

        OutboxStore::delete(&db, &first.mutation_id)
            .await
            .expect("delete by mutation id");
        assert!(!OutboxStore::exists(&db, &first.mutation_id.as_str())
            .await
            .expect("first intent should be deleted"));
        OutboxStore::delete_by_id(&db, &second.mutation_id.as_str())
            .await
            .expect("delete by raw id");
        assert_eq!(
            OutboxStore::count(&db).await.expect("count after deletes"),
            0
        );
    }

    #[tokio::test]
    async fn dead_letter_store_roundtrip_covers_delete_and_clear() {
        let db = Database::new_in_memory().await.expect("in-memory db");
        let api_error = ApiError {
            code: "conflict".to_string(),
            message: "mutation rejected".to_string(),
        };
        let first = DeadLetterRecord::new(
            MutationId::from_uuid(Uuid::new_v4()).as_str(),
            "POST".to_string(),
            "/api/v1/preferences".to_string(),
            "{}".to_string(),
            100,
            Some(&api_error),
        );
        let second = DeadLetterRecord::new(
            MutationId::from_uuid(Uuid::new_v4()).as_str(),
            "DELETE".to_string(),
            "/api/v1/exercises/123".to_string(),
            "{}".to_string(),
            101,
            None,
        );

        DeadLetterStore::add(&db, &first)
            .await
            .expect("insert first dead letter");
        DeadLetterStore::add(&db, &second)
            .await
            .expect("insert second dead letter");

        let records = DeadLetterStore::get_all(&db)
            .await
            .expect("load dead letters");
        assert_eq!(records.len(), 2);
        assert!(records.iter().any(|record| record.error.is_some()));
        assert!(DeadLetterStore::exists(&db, &first.mutation_id)
            .await
            .expect("first exists"));
        assert_eq!(
            DeadLetterStore::count(&db)
                .await
                .expect("dead letter count"),
            2
        );

        DeadLetterStore::delete(&db, &first.mutation_id)
            .await
            .expect("delete dead letter");
        assert!(!DeadLetterStore::exists(&db, &first.mutation_id)
            .await
            .expect("first dead letter removed"));

        DeadLetterStore::clear(&db)
            .await
            .expect("clear dead letters");
        assert_eq!(
            DeadLetterStore::count(&db)
                .await
                .expect("count after clear"),
            0
        );
        assert!(
            DeadLetterStore::get_all(&db)
                .await
                .expect("dead letters after clear")
                .is_empty(),
            "clear should leave no loadable dead-letter rows"
        );
    }

    #[tokio::test]
    async fn exercise_store_roundtrip_replace_all_and_skip_corrupt_rows() {
        let db = Database::new_in_memory().await.expect("in-memory db");
        let squat = sample_exercise("Back Squat");
        let lunge = sample_exercise("Walking Lunge");

        ExerciseStore::put(&db, &squat).await.expect("put squat");
        assert_eq!(
            ExerciseStore::get(&db, &squat.id)
                .await
                .expect("get squat")
                .expect("squat exists")
                .name,
            "Back Squat"
        );
        assert_eq!(
            ExerciseStore::get_all(&db)
                .await
                .expect("all exercises")
                .len(),
            1
        );

        ExerciseStore::replace_all(&db, &[])
            .await
            .expect("empty replace should be a no-op");
        assert_eq!(
            ExerciseStore::get_all(&db)
                .await
                .expect("all exercises after empty replace")
                .len(),
            1
        );

        ExerciseStore::replace_all(&db, &[squat.clone(), lunge.clone()])
            .await
            .expect("replace all exercises");

        {
            let conn = db.conn.lock().await;
            conn.execute(
                "INSERT INTO exercises (id, name, description, muscle_groups, equipment, is_custom, is_favorite, owner_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    Uuid::new_v4().to_string(),
                    "Broken Exercise",
                    Option::<String>::None,
                    "[]",
                    "\"not-real-equipment\"",
                    0,
                    0,
                    Option::<String>::None,
                    0_i64,
                    0_i64,
                ],
            )
            .expect("insert corrupted exercise row");
        }

        let all = ExerciseStore::get_all(&db)
            .await
            .expect("load exercises with corrupt row skipped");
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|exercise| exercise.name == "Back Squat"));
        assert!(all.iter().any(|exercise| exercise.name == "Walking Lunge"));

        ExerciseStore::delete(&db, &squat.id)
            .await
            .expect("delete squat");
        assert!(ExerciseStore::get(&db, &squat.id)
            .await
            .expect("load deleted squat")
            .is_none());
    }

    #[tokio::test]
    async fn template_store_roundtrip_replace_all_and_skip_corrupt_rows() {
        let db = Database::new_in_memory().await.expect("in-memory db");
        let exercise = sample_exercise("Romanian Deadlift");
        let first = sample_template("Posterior Chain", exercise.id.clone());
        let second = sample_template("Accessory Day", exercise.id.clone());

        TemplateStore::put(&db, &first)
            .await
            .expect("put first template");
        assert_eq!(
            TemplateStore::get(&db, &first.id)
                .await
                .expect("get first template")
                .expect("first template exists")
                .name,
            "Posterior Chain"
        );

        TemplateStore::replace_all(&db, &[])
            .await
            .expect("empty replace should preserve templates");
        assert_eq!(
            TemplateStore::get_all(&db)
                .await
                .expect("all templates after empty replace")
                .len(),
            1
        );

        TemplateStore::replace_all(&db, &[first.clone(), second.clone()])
            .await
            .expect("replace all templates");

        {
            let conn = db.conn.lock().await;
            conn.execute(
                "INSERT INTO templates (id, owner_id, name, description, exercises, estimated_minutes, is_favorite, tags, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    Uuid::new_v4().to_string(),
                    "not-a-uuid",
                    "Broken Template",
                    Option::<String>::None,
                    "{not-json",
                    30_i64,
                    0,
                    "[\"tag\"]",
                    0_i64,
                    0_i64,
                ],
            )
            .expect("insert corrupted template row");
        }

        let all = TemplateStore::get_all(&db)
            .await
            .expect("load templates with corrupt row skipped");
        assert_eq!(all.len(), 2);

        TemplateStore::delete(&db, &first.id)
            .await
            .expect("delete first template");
        assert!(TemplateStore::get(&db, &first.id)
            .await
            .expect("get deleted template")
            .is_none());
    }

    #[tokio::test]
    async fn session_store_roundtrip_get_active_replace_all_and_skip_corrupt_rows() {
        let db = Database::new_in_memory().await.expect("in-memory db");
        let exercise = sample_exercise("Bench Press");
        let template = sample_template("Upper Day", exercise.id.clone());
        let active = sample_session("Morning Session", &exercise, Some(template.id.clone()));
        let mut completed =
            sample_session("Finished Session", &exercise, Some(template.id.clone()));
        completed.complete();

        SessionStore::put(&db, &active)
            .await
            .expect("put active session");
        SessionStore::put(&db, &completed)
            .await
            .expect("put completed session");

        assert_eq!(
            SessionStore::get_active(&db)
                .await
                .expect("get active session")
                .expect("active session exists")
                .name,
            "Morning Session"
        );

        SessionStore::replace_all(&db, &[])
            .await
            .expect("empty replace should preserve sessions");
        assert_eq!(
            SessionStore::get_all(&db)
                .await
                .expect("sessions after empty replace")
                .len(),
            2
        );

        SessionStore::replace_all(&db, &[completed.clone()])
            .await
            .expect("replace all sessions");
        assert!(SessionStore::get_active(&db)
            .await
            .expect("no active session after replace")
            .is_none());

        {
            let conn = db.conn.lock().await;
            conn.execute(
                "INSERT INTO workout_sessions (id, owner_id, template_id, name, status, exercises, started_at, paused_at, total_paused_seconds, notes, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    Uuid::new_v4().to_string(),
                    "not-a-uuid",
                    Option::<String>::None,
                    "Broken Session",
                    "\"active\"",
                    "{not-json",
                    0_i64,
                    Option::<i64>::None,
                    0_i64,
                    Option::<String>::None,
                    0_i64,
                    0_i64,
                ],
            )
            .expect("insert corrupted session row");
        }

        let all = SessionStore::get_all(&db)
            .await
            .expect("load sessions with corrupt row skipped");
        assert_eq!(all.len(), 1);

        SessionStore::delete(&db, &completed.id)
            .await
            .expect("delete completed session");
        assert!(SessionStore::get(&db, &completed.id)
            .await
            .expect("get deleted session")
            .is_none());
        assert!(
            SessionStore::get_all(&db)
                .await
                .expect("sessions after delete")
                .is_empty(),
            "after deleting the only valid session, only corrupt rows should remain and be skipped"
        );
    }
}

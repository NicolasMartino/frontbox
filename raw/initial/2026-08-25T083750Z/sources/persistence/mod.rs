//! Platform-specific storage module.
//!
//! - Native (desktop + mobile): SQLite via rusqlite
//! - Web (wasm32): IndexedDB via rexie
//!
//! Both backends expose the same API through re-exports.

pub mod cache;
pub mod exercise_drafts;
pub mod mutations;
pub mod preferences;
pub mod preferences_cache;
pub mod types;

#[cfg(not(target_arch = "wasm32"))]
mod native;

#[cfg(target_arch = "wasm32")]
mod web;

// Re-export types
pub use types::{
    DeadLetterRecord, ExerciseRecord, OutboxRecord, SessionRecord, StoreError, StoreResult,
    TemplateRecord,
};

// Re-export cache types
pub use cache::{EntityCache, EntityType, VersionUpdateResult};

// Re-export mutation outbox
pub use mutations::{start_sync_loop, MutationStore, SyncStatus};

// Re-export exercise draft persistence
pub use exercise_drafts::{clear_exercise_draft, load_exercise_draft, save_exercise_draft};

// Re-export preference cache
pub use preferences_cache::{CachedPreferences, PreferenceCache};

// Re-export platform-specific implementations
#[cfg(not(target_arch = "wasm32"))]
pub use native::{
    init_db, init_db_for_user, Database, DeadLetterStore, ExerciseStore, OutboxStore, SessionStore,
    TemplateStore,
};

#[cfg(target_arch = "wasm32")]
pub use web::{
    init_db, init_db_for_user, Database, DeadLetterStore, ExerciseStore, OutboxStore, SessionStore,
    TemplateStore,
};

#[cfg(all(test, not(target_arch = "wasm32")))]
pub async fn init_test_db() -> StoreResult<Database> {
    Database::new_in_memory().await
}

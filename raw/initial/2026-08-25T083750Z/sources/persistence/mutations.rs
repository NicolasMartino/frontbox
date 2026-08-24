//! Mutation outbox for offline-first write dispatch.
//!
//! This module provides a unified interface for queuing workout, user, and
//! exercise mutations with offline support, optimistic updates, and
//! background synchronization.
//!
//! ## Architecture
//!
//! ```text
//! UI → enqueue mutation → local outbox → optimistic projection
//!                         ↓
//!                   background sync → server → outcome
//!                         ↓
//!                   remove from outbox
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! let mutation_store = use_context::<MutationStore>();
//!
//! // Enqueue a mutation (returns immediately, syncs in background)
//! mutation_store.enqueue_post_session(PostSessionRequest { ... });
//!
//! // Check sync status
//! let pending = mutation_store.pending_count();
//! let is_syncing = mutation_store.is_syncing();
//! ```

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use chrono::Utc;
use shared::frontend::dto::{
    MutationBatchRequest, MutationBatchResponse, MutationId, MutationIntentDto, MutationStatus,
};
use shared::{
    PatchExerciseRequest, PatchSessionRequest, PatchTranslationProposalRequest,
    PostExerciseRequest, PostSessionRequest, PostSessionSetRequest, PostTranslationProposalRequest,
    PutUserPreferencesRequest,
};
use uuid::Uuid;

use super::{Database, DeadLetterRecord, DeadLetterStore, OutboxRecord, OutboxStore, StoreResult};

const DEFAULT_DEAD_LETTER_RETENTION_DAYS: u32 = 30;
const DEAD_LETTER_CLEANUP_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;

/// Sync status for the mutation outbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncStatus {
    /// Not syncing, ready for new mutations.
    #[default]
    Idle,
    /// Currently syncing mutations to the server.
    Syncing,
    /// Sync failed, will retry.
    Error,
    /// Offline, mutations queued locally.
    Offline,
}

/// Internal state for `MutationStore` (shared via `Rc`).
struct MutationStoreInner {
    db: Rc<Database>,
    pending_count: AtomicUsize,
    is_syncing: AtomicBool,
    is_error: AtomicBool,
    is_offline: AtomicBool,
}

struct SyncApplySummary {
    accepted_count: usize,
    duplicate_count: usize,
    rejected_count: usize,
    mutation_ids_to_delete: HashSet<MutationId>,
}

impl SyncApplySummary {
    fn new(capacity: usize) -> Self {
        Self {
            accepted_count: 0,
            duplicate_count: 0,
            rejected_count: 0,
            mutation_ids_to_delete: HashSet::with_capacity(capacity),
        }
    }
}

fn mutation_intent(
    mutation_id: MutationId,
    method: &str,
    path: String,
    body: impl serde::Serialize,
) -> StoreResult<MutationIntentDto> {
    let body = serde_json::to_value(body)
        .map_err(|error| format!("failed to serialize mutation body: {error}"))?;
    Ok(MutationIntentDto::new(mutation_id, method, path, body))
}

fn post_session_intent(
    mutation_id: MutationId,
    request: PostSessionRequest,
) -> StoreResult<MutationIntentDto> {
    mutation_intent(mutation_id, "POST", "/api/v1/sessions".to_string(), request)
}

fn patch_session_intent(
    mutation_id: MutationId,
    session_id: Uuid,
    request: PatchSessionRequest,
) -> StoreResult<MutationIntentDto> {
    mutation_intent(
        mutation_id,
        "PATCH",
        format!("/api/v1/sessions/{session_id}"),
        request,
    )
}

fn post_session_set_intent(
    mutation_id: MutationId,
    session_id: Uuid,
    request: PostSessionSetRequest,
) -> StoreResult<MutationIntentDto> {
    mutation_intent(
        mutation_id,
        "POST",
        format!("/api/v1/sessions/{session_id}/sets"),
        request,
    )
}

fn put_user_preferences_intent(
    mutation_id: MutationId,
    request: PutUserPreferencesRequest,
) -> StoreResult<MutationIntentDto> {
    mutation_intent(
        mutation_id,
        "PUT",
        "/api/v1/me/preferences".to_string(),
        request,
    )
}

fn post_exercise_intent(
    mutation_id: MutationId,
    request: PostExerciseRequest,
) -> StoreResult<MutationIntentDto> {
    mutation_intent(
        mutation_id,
        "POST",
        "/api/v1/exercises".to_string(),
        request,
    )
}

fn patch_exercise_intent(
    mutation_id: MutationId,
    exercise_id: Uuid,
    request: PatchExerciseRequest,
) -> StoreResult<MutationIntentDto> {
    mutation_intent(
        mutation_id,
        "PATCH",
        format!("/api/v1/exercises/{exercise_id}"),
        request,
    )
}

fn delete_exercise_intent(
    mutation_id: MutationId,
    exercise_id: Uuid,
) -> StoreResult<MutationIntentDto> {
    mutation_intent(
        mutation_id,
        "DELETE",
        format!("/api/v1/exercises/{exercise_id}"),
        serde_json::json!({}),
    )
}

fn put_favorite_exercise_intent(
    mutation_id: MutationId,
    exercise_id: Uuid,
) -> StoreResult<MutationIntentDto> {
    mutation_intent(
        mutation_id,
        "PUT",
        format!("/api/v1/me/favorite-exercises/{exercise_id}"),
        serde_json::json!({}),
    )
}

fn delete_favorite_exercise_intent(
    mutation_id: MutationId,
    exercise_id: Uuid,
) -> StoreResult<MutationIntentDto> {
    mutation_intent(
        mutation_id,
        "DELETE",
        format!("/api/v1/me/favorite-exercises/{exercise_id}"),
        serde_json::json!({}),
    )
}

fn post_translation_proposal_intent(
    mutation_id: MutationId,
    exercise_id: Uuid,
    request: PostTranslationProposalRequest,
) -> StoreResult<MutationIntentDto> {
    mutation_intent(
        mutation_id,
        "POST",
        format!("/api/v1/exercises/{exercise_id}/translation-proposals"),
        request,
    )
}

fn patch_translation_proposal_intent(
    mutation_id: MutationId,
    proposal_id: Uuid,
    request: PatchTranslationProposalRequest,
) -> StoreResult<MutationIntentDto> {
    mutation_intent(
        mutation_id,
        "PATCH",
        format!("/api/v1/translation-proposals/{proposal_id}"),
        request,
    )
}

/// Mutation outbox for offline-first write dispatch.
///
/// Manages a local outbox of mutation intents waiting to be synced to the server.
/// Mutations are persisted locally and synced in the background when online.
///
/// This store is designed to be shared via `use_context` in Dioxus components.
#[derive(Clone)]
pub struct MutationStore {
    inner: Rc<MutationStoreInner>,
}

impl MutationStore {
    async fn apply_sync_results(
        &self,
        records: &[OutboxRecord],
        response: MutationBatchResponse,
    ) -> SyncApplySummary {
        let mut records_by_id = HashMap::with_capacity(records.len());
        for record in records {
            if let Ok(mutation_id) = record.mutation_id() {
                records_by_id.insert(mutation_id, record);
            }
        }

        let mut summary = SyncApplySummary::new(records.len());

        for result in response.results {
            match result.status {
                MutationStatus::Applied => {
                    summary.accepted_count += 1;
                    summary.mutation_ids_to_delete.insert(result.mutation_id);
                }
                MutationStatus::Duplicate => {
                    summary.duplicate_count += 1;
                    summary.mutation_ids_to_delete.insert(result.mutation_id);
                }
                MutationStatus::Rejected | MutationStatus::Blocked => {
                    if let Some(record) = records_by_id.get(&result.mutation_id) {
                        let dead_letter = DeadLetterRecord::new(
                            record.mutation_id.clone(),
                            record.method.clone(),
                            record.path.clone(),
                            record.body.clone(),
                            record.created_at,
                            result.error.as_ref(),
                        );

                        if DeadLetterStore::add(&self.inner.db, &dead_letter)
                            .await
                            .is_ok()
                        {
                            summary.rejected_count += 1;
                            summary.mutation_ids_to_delete.insert(result.mutation_id);
                        } else {
                            crate::log!(
                                "[MutationStore] Failed to move rejected mutation {} to dead letter",
                                result.mutation_id.as_str()
                            );
                        }
                    } else {
                        crate::log!(
                            "[MutationStore] Rejected result references unknown mutation {}",
                            result.mutation_id.as_str()
                        );
                    }
                }
                MutationStatus::Pending => {}
            }
        }

        summary
    }

    /// Create a new mutation outbox from an `Rc<Database>`.
    ///
    /// The database is shared via Rc since some platforms (web/IndexedDB)
    /// don't support cloning the database handle.
    pub fn from_rc(db: Rc<Database>) -> Self {
        Self {
            inner: Rc::new(MutationStoreInner {
                db,
                pending_count: AtomicUsize::new(0),
                is_syncing: AtomicBool::new(false),
                is_error: AtomicBool::new(false),
                is_offline: AtomicBool::new(false),
            }),
        }
    }

    /// Get the database reference.
    pub fn database(&self) -> &Database {
        &self.inner.db
    }

    /// Get the pending mutation count.
    pub fn pending_count(&self) -> usize {
        self.inner.pending_count.load(Ordering::SeqCst)
    }

    /// Get the current sync status.
    pub fn sync_status(&self) -> SyncStatus {
        if self.inner.is_syncing.load(Ordering::SeqCst) {
            SyncStatus::Syncing
        } else if self.inner.is_error.load(Ordering::SeqCst) {
            SyncStatus::Error
        } else if self.inner.is_offline.load(Ordering::SeqCst) {
            SyncStatus::Offline
        } else {
            SyncStatus::Idle
        }
    }

    /// Check if there are pending mutations.
    pub fn has_pending(&self) -> bool {
        self.pending_count() > 0
    }

    /// Check if currently syncing.
    pub fn is_syncing(&self) -> bool {
        self.inner.is_syncing.load(Ordering::SeqCst)
    }

    /// Refresh the pending count from the database.
    pub async fn refresh_pending_count(&self) -> StoreResult<()> {
        let count = OutboxStore::count(&self.inner.db).await?;
        self.inner.pending_count.store(count, Ordering::SeqCst);
        Ok(())
    }

    /// Enqueue a create-session mutation.
    pub async fn enqueue_post_session(
        &self,
        request: PostSessionRequest,
    ) -> StoreResult<MutationId> {
        let mutation_id = MutationId::new();
        let intent = post_session_intent(mutation_id, request)?;
        self.dispatch_mutation_intent(intent).await
    }

    /// Enqueue a patch-session mutation.
    pub async fn enqueue_patch_session(
        &self,
        session_id: Uuid,
        request: PatchSessionRequest,
    ) -> StoreResult<MutationId> {
        let mutation_id = MutationId::new();
        let intent = patch_session_intent(mutation_id, session_id, request)?;
        self.dispatch_mutation_intent(intent).await
    }

    /// Enqueue a log-set mutation.
    pub async fn enqueue_post_session_set(
        &self,
        session_id: Uuid,
        request: PostSessionSetRequest,
    ) -> StoreResult<MutationId> {
        let mutation_id = MutationId::new();
        let intent = post_session_set_intent(mutation_id, session_id, request)?;
        self.dispatch_mutation_intent(intent).await
    }

    /// Enqueue a preferences mutation.
    pub async fn enqueue_put_user_preferences(
        &self,
        request: PutUserPreferencesRequest,
    ) -> StoreResult<MutationId> {
        let mutation_id = MutationId::new();
        self.enqueue_put_user_preferences_with_id(mutation_id, request)
            .await
    }

    /// Enqueue a preferences mutation with a stable caller-provided idempotency key.
    pub async fn enqueue_put_user_preferences_with_id(
        &self,
        mutation_id: MutationId,
        request: PutUserPreferencesRequest,
    ) -> StoreResult<MutationId> {
        let intent = put_user_preferences_intent(mutation_id, request)?;
        self.dispatch_mutation_intent(intent).await
    }

    /// Enqueue a create-exercise mutation.
    pub async fn enqueue_post_exercise(
        &self,
        request: PostExerciseRequest,
    ) -> StoreResult<MutationId> {
        let mutation_id = MutationId::new();
        let intent = post_exercise_intent(mutation_id, request)?;
        self.dispatch_mutation_intent(intent).await
    }

    /// Enqueue an update-exercise mutation.
    pub async fn enqueue_patch_exercise(
        &self,
        exercise_id: Uuid,
        request: PatchExerciseRequest,
    ) -> StoreResult<MutationId> {
        let mutation_id = MutationId::new();
        let intent = patch_exercise_intent(mutation_id, exercise_id, request)?;
        self.dispatch_mutation_intent(intent).await
    }

    /// Enqueue a delete-exercise mutation.
    pub async fn enqueue_delete_exercise(&self, exercise_id: Uuid) -> StoreResult<MutationId> {
        let mutation_id = MutationId::new();
        let intent = delete_exercise_intent(mutation_id, exercise_id)?;
        self.dispatch_mutation_intent(intent).await
    }

    /// Enqueue a favorite-exercise mutation.
    pub async fn enqueue_put_favorite_exercise(
        &self,
        exercise_id: Uuid,
    ) -> StoreResult<MutationId> {
        let mutation_id = MutationId::new();
        let intent = put_favorite_exercise_intent(mutation_id, exercise_id)?;
        self.dispatch_mutation_intent(intent).await
    }

    /// Enqueue an unfavorite-exercise mutation.
    pub async fn enqueue_delete_favorite_exercise(
        &self,
        exercise_id: Uuid,
    ) -> StoreResult<MutationId> {
        let mutation_id = MutationId::new();
        let intent = delete_favorite_exercise_intent(mutation_id, exercise_id)?;
        self.dispatch_mutation_intent(intent).await
    }

    /// Enqueue a translation-proposal mutation.
    pub async fn enqueue_post_translation_proposal(
        &self,
        exercise_id: Uuid,
        request: PostTranslationProposalRequest,
    ) -> StoreResult<MutationId> {
        let mutation_id = MutationId::new();
        let intent = post_translation_proposal_intent(mutation_id, exercise_id, request)?;
        self.dispatch_mutation_intent(intent).await
    }

    /// Enqueue a translation moderation mutation.
    pub async fn enqueue_patch_translation_proposal(
        &self,
        proposal_id: Uuid,
        request: PatchTranslationProposalRequest,
    ) -> StoreResult<MutationId> {
        let mutation_id = MutationId::new();
        let intent = patch_translation_proposal_intent(mutation_id, proposal_id, request)?;
        self.dispatch_mutation_intent(intent).await
    }

    async fn dispatch_mutation_intent(&self, intent: MutationIntentDto) -> StoreResult<MutationId> {
        let mutation_id = intent.mutation_id;
        let method = intent.method.clone();
        let path = intent.path.clone();
        let added = OutboxStore::add(&self.inner.db, &intent).await?;

        if added {
            let count = OutboxStore::count(&self.inner.db).await?;
            self.inner.pending_count.store(count, Ordering::SeqCst);

            crate::log!(
                "[MutationStore] Dispatched mutation {} {} {} (pending: {})",
                mutation_id.as_str(),
                method,
                path,
                count
            );
        } else {
            crate::log!(
                "[MutationStore] mutation {} already exists in outbox",
                mutation_id.as_str()
            );
        }

        Ok(mutation_id)
    }

    /// Get all pending mutations from the outbox.
    pub async fn get_pending(&self) -> StoreResult<Vec<OutboxRecord>> {
        OutboxStore::get_all(&self.inner.db).await
    }

    /// Remove pending create/update mutations for a draft exercise id.
    pub async fn remove_pending_exercise_draft_mutations(
        &self,
        exercise_id: Uuid,
    ) -> StoreResult<usize> {
        let pending = OutboxStore::get_all(&self.inner.db).await?;
        let mut mutation_ids = Vec::<String>::new();

        for record in pending {
            if !record.path.starts_with("/api/v1/exercises") {
                continue;
            }

            let should_remove = if record.method == "POST" && record.path == "/api/v1/exercises" {
                serde_json::from_str::<serde_json::Value>(&record.body)
                    .ok()
                    .and_then(|body| body.get("exercise_id").cloned())
                    .and_then(|value| serde_json::from_value::<Uuid>(value).ok())
                    == Some(exercise_id)
            } else if record.method == "PATCH" {
                record.path == format!("/api/v1/exercises/{exercise_id}")
            } else {
                false
            };

            if should_remove {
                mutation_ids.push(record.mutation_id);
            }
        }

        for mutation_id in &mutation_ids {
            OutboxStore::delete_by_id(&self.inner.db, mutation_id).await?;
        }

        let count = OutboxStore::count(&self.inner.db).await?;
        self.inner.pending_count.store(count, Ordering::SeqCst);

        Ok(mutation_ids.len())
    }

    /// Check if a mutation is still pending in the outbox.
    pub async fn mutation_is_pending(&self, mutation_id: &MutationId) -> StoreResult<bool> {
        OutboxStore::exists(&self.inner.db, &mutation_id.as_str()).await
    }

    /// Check if a mutation has been moved to dead letter storage.
    pub async fn mutation_was_rejected(&self, mutation_id: &MutationId) -> StoreResult<bool> {
        DeadLetterStore::exists(&self.inner.db, &mutation_id.as_str()).await
    }

    /// Purge dead-letter records older than `retention_days`.
    ///
    /// Returns the number of records removed.
    pub async fn purge_dead_letters(&self, retention_days: u32) -> StoreResult<usize> {
        let retention_ms = i64::from(retention_days) * 24 * 60 * 60 * 1000;
        let cutoff_ms = Utc::now().timestamp_millis() - retention_ms;
        let dead_letters = DeadLetterStore::get_all(&self.inner.db).await?;
        let mut removed = 0usize;

        for record in dead_letters {
            if record.rejected_at < cutoff_ms {
                DeadLetterStore::delete(&self.inner.db, &record.mutation_id).await?;
                removed += 1;
            }
        }

        Ok(removed)
    }

    /// Remove a mutation from the outbox after successful sync.
    pub async fn mark_synced(&self, mutation_id: &MutationId) -> StoreResult<()> {
        OutboxStore::delete(&self.inner.db, mutation_id).await?;

        // Update pending count
        let count = OutboxStore::count(&self.inner.db).await?;
        self.inner.pending_count.store(count, Ordering::SeqCst);

        crate::log!(
            "[MutationStore] Marked {} as synced (pending: {})",
            mutation_id.as_str(),
            count
        );

        Ok(())
    }

    /// Sync pending mutations to the server.
    ///
    /// This method is called by the background sync loop. It:
    /// 1. Gets all pending mutations from the outbox
    /// 2. Sends them to the server via `push_mutations`
    /// 3. Removes successfully synced mutations
    /// 4. Updates sync status
    ///
    /// Returns the number of mutations synced.
    pub async fn sync(&self, auth_header: &str) -> StoreResult<usize> {
        // Check if already syncing
        if self
            .inner
            .is_syncing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(0);
        }

        self.inner.is_error.store(false, Ordering::SeqCst);
        self.inner.is_offline.store(false, Ordering::SeqCst);

        // Get pending mutations
        let records = match self.get_pending().await {
            Ok(r) => r,
            Err(e) => {
                self.inner.is_syncing.store(false, Ordering::SeqCst);
                return Err(e);
            }
        };

        if records.is_empty() {
            self.inner.is_syncing.store(false, Ordering::SeqCst);
            return Ok(0);
        }

        crate::log!("[MutationStore] Syncing {} mutations...", records.len());

        let mutations: Vec<MutationIntentDto> =
            records.iter().filter_map(|r| r.to_intent().ok()).collect();

        if mutations.is_empty() {
            self.inner.is_syncing.store(false, Ordering::SeqCst);
            return Ok(0);
        }

        let request = MutationBatchRequest { mutations };

        // Call the server function
        match crate::api::sync::push_mutations(auth_header.to_string(), request).await {
            Ok(response) => {
                let summary = self.apply_sync_results(&records, response).await;

                for mutation_id in &summary.mutation_ids_to_delete {
                    let _ = OutboxStore::delete(&self.inner.db, mutation_id).await;
                }

                // Update pending count
                let count = OutboxStore::count(&self.inner.db).await.unwrap_or(0);
                self.inner.pending_count.store(count, Ordering::SeqCst);
                self.inner.is_syncing.store(false, Ordering::SeqCst);

                crate::log!(
                    "[MutationStore] Sync complete (applied: {}, duplicate: {}, rejected->dead-letter: {}, pending: {})",
                    summary.accepted_count,
                    summary.duplicate_count,
                    summary.rejected_count,
                    count
                );

                Ok(summary.mutation_ids_to_delete.len())
            }
            Err(e) => {
                let error_msg = format!("Sync failed: {}", e);
                crate::log!("[MutationStore] {}", error_msg);

                self.inner.is_syncing.store(false, Ordering::SeqCst);
                self.inner.is_error.store(true, Ordering::SeqCst);

                Err(error_msg)
            }
        }
    }

    /// Mark the store as offline.
    pub fn set_offline(&self, offline: bool) {
        self.inner.is_offline.store(offline, Ordering::SeqCst);
    }

    /// Clear all pending mutations (use with caution).
    pub async fn clear_pending(&self) -> StoreResult<()> {
        let records = self.get_pending().await?;
        for record in records {
            if let Ok(mutation_id) = record.mutation_id() {
                OutboxStore::delete(&self.inner.db, &mutation_id).await?;
            }
        }
        self.inner.pending_count.store(0, Ordering::SeqCst);
        Ok(())
    }
}

/// Background sync task that periodically syncs pending mutations.
///
/// This function should be spawned as a background task when the app starts.
/// It will run indefinitely, syncing mutations every few seconds when online.
///
/// # Example
///
/// ```ignore
/// let mutation_store = use_context::<MutationStore>();
/// let auth = use_context::<AuthContext>();
///
/// use_future(move || {
///     let store = mutation_store.clone();
///     let auth = auth.clone();
///     async move {
///         start_sync_loop(store, move || get_auth_header(&auth)).await;
///     }
/// });
/// ```
pub async fn start_sync_loop(
    store: MutationStore,
    get_auth_header: impl Fn() -> Option<String> + 'static,
) {
    use crate::infrastructure::platform::sleep_ms;

    const SYNC_INTERVAL_MS: u32 = 5000; // 5 seconds
    const RETRY_INTERVAL_MS: u32 = 30000; // 30 seconds on error

    crate::log!(
        "[SyncLoop] Loop started - will run every {}ms",
        SYNC_INTERVAL_MS
    );

    let mut last_dead_letter_cleanup_ms = 0_i64;

    loop {
        // Wait before syncing
        let wait_time = if store.sync_status() == SyncStatus::Error {
            RETRY_INTERVAL_MS
        } else {
            SYNC_INTERVAL_MS
        };
        sleep_ms(wait_time).await;

        // Refresh pending count from DB
        let _ = store.refresh_pending_count().await;
        let pending = store.pending_count();

        let now_ms = Utc::now().timestamp_millis();
        if now_ms - last_dead_letter_cleanup_ms >= DEAD_LETTER_CLEANUP_INTERVAL_MS {
            match store
                .purge_dead_letters(DEFAULT_DEAD_LETTER_RETENTION_DAYS)
                .await
            {
                Ok(removed) if removed > 0 => {
                    crate::log!(
                        "[SyncLoop] Purged {} dead-letter records older than {} days",
                        removed,
                        DEFAULT_DEAD_LETTER_RETENTION_DAYS
                    );
                }
                Ok(_) => {}
                Err(err) => {
                    crate::log!("[SyncLoop] Dead-letter purge failed: {}", err);
                }
            }
            last_dead_letter_cleanup_ms = now_ms;
        }

        crate::log!("[SyncLoop] Woke up - pending mutations: {}", pending);

        // Get auth header
        let auth_header = match get_auth_header() {
            Some(h) => h,
            None => {
                crate::log!("[SyncLoop] No auth header, skipping sync");
                store.set_offline(true);
                continue;
            }
        };

        // Attempt sync
        crate::log!(
            "[SyncLoop] Calling store.sync() with {} pending mutations",
            pending
        );
        let _ = store.sync(&auth_header).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::contracts::common::{Equipment, Language, MuscleGroup, Theme, Units, Visibility};
    use shared::frontend::dto::{ApiError, MutationResult};
    use shared::{
        PatchExerciseRequest, PatchSessionRequest, PatchTranslationProposalRequest,
        PostExerciseRequest, PostSessionRequest, PostTranslationProposalRequest,
        PutUserPreferencesRequest, SessionMutationStatus, TranslationProposalStatus,
    };
    use std::rc::Rc;

    #[test]
    fn test_sync_status_default() {
        assert_eq!(SyncStatus::default(), SyncStatus::Idle);
    }

    #[tokio::test]
    async fn mutation_is_pending_tracks_outbox_membership() {
        let db = crate::infrastructure::persistence::init_test_db()
            .await
            .expect("in-memory db should initialize");
        let store = MutationStore::from_rc(Rc::new(db));

        let mutation_id = store
            .enqueue_put_favorite_exercise(Uuid::new_v4())
            .await
            .expect("favorite mutation should dispatch");

        assert!(
            store
                .mutation_is_pending(&mutation_id)
                .await
                .expect("pending lookup should succeed"),
            "dispatched mutation should be pending in outbox"
        );

        store
            .mark_synced(&mutation_id)
            .await
            .expect("marking mutation synced should succeed");

        assert!(
            !store
                .mutation_is_pending(&mutation_id)
                .await
                .expect("pending lookup should succeed after sync"),
            "synced mutation should no longer be pending"
        );
    }

    #[tokio::test]
    async fn enqueue_put_user_preferences_with_id_preserves_caller_supplied_mutation_id() {
        let db = crate::infrastructure::persistence::init_test_db()
            .await
            .expect("in-memory db should initialize");
        let store = MutationStore::from_rc(Rc::new(db));
        let mutation_id = MutationId::new();

        let returned = store
            .enqueue_put_user_preferences_with_id(
                mutation_id,
                PutUserPreferencesRequest {
                    units: shared::Units::Kg,
                    theme: shared::Theme::Dark,
                    language: shared::ContractLanguage::En,
                    default_rest_seconds: 90,
                },
            )
            .await
            .expect("preferences mutation should dispatch");

        assert_eq!(returned, mutation_id);
        assert!(
            store
                .mutation_is_pending(&mutation_id)
                .await
                .expect("pending lookup should succeed"),
            "caller-provided mutation id should be stored in the outbox"
        );
    }

    #[tokio::test]
    async fn purge_dead_letters_removes_only_expired_records() {
        let db = Rc::new(
            crate::infrastructure::persistence::init_test_db()
                .await
                .expect("in-memory db should initialize"),
        );
        let store = MutationStore::from_rc(Rc::clone(&db));
        let now_ms = Utc::now().timestamp_millis();
        let day_ms = 24 * 60 * 60 * 1000;

        let old_id = format!("old-{}", Uuid::new_v4());
        let recent_id = format!("recent-{}", Uuid::new_v4());

        let old_record = DeadLetterRecord {
            mutation_id: old_id.clone(),
            method: "PATCH".to_string(),
            path: "/api/v1/exercises/old".to_string(),
            body: "{\"name\":\"Old\"}".to_string(),
            created_at: now_ms - (45 * day_ms),
            rejected_at: now_ms - (40 * day_ms),
            error: Some("{\"code\":\"ValidationError\"}".to_string()),
        };
        let recent_record = DeadLetterRecord {
            mutation_id: recent_id.clone(),
            method: "PATCH".to_string(),
            path: "/api/v1/exercises/recent".to_string(),
            body: "{\"name\":\"Recent\"}".to_string(),
            created_at: now_ms - (5 * day_ms),
            rejected_at: now_ms - (2 * day_ms),
            error: Some("{\"code\":\"ValidationError\"}".to_string()),
        };

        DeadLetterStore::add(&db, &old_record)
            .await
            .expect("old dead letter inserted");
        DeadLetterStore::add(&db, &recent_record)
            .await
            .expect("recent dead letter inserted");

        let removed = store
            .purge_dead_letters(30)
            .await
            .expect("purge should succeed");
        assert_eq!(removed, 1, "only one expired record should be removed");
        assert!(
            !DeadLetterStore::exists(&db, &old_id)
                .await
                .expect("old record lookup should succeed"),
            "expired record should be removed"
        );
        assert!(
            DeadLetterStore::exists(&db, &recent_id)
                .await
                .expect("recent record lookup should succeed"),
            "recent record should remain"
        );
    }

    #[tokio::test]
    async fn remove_pending_exercise_draft_mutations_only_removes_matching_create_update() {
        let db = crate::infrastructure::persistence::init_test_db()
            .await
            .expect("in-memory db should initialize");
        let store = MutationStore::from_rc(Rc::new(db));

        let target_exercise_id = Uuid::new_v4();
        let other_exercise_id = Uuid::new_v4();

        let target_create = store
            .enqueue_post_exercise(PostExerciseRequest {
                exercise_id: target_exercise_id,
                name: "Target draft".to_string(),
                muscle_groups: vec![MuscleGroup::Core],
                equipment: Some(Equipment::Bodyweight),
                description: None,
                instructions: None,
                video_url: None,
                visibility: Visibility::Private,
            })
            .await
            .expect("target create mutation should dispatch");
        let target_update = store
            .enqueue_patch_exercise(
                target_exercise_id,
                PatchExerciseRequest {
                    name: Some("Target updated".to_string()),
                    muscle_groups: Some(vec![MuscleGroup::Core]),
                    equipment: Some(Equipment::Bodyweight),
                    description: None,
                    instructions: None,
                    video_url: None,
                    visibility: Some(Visibility::Private),
                },
            )
            .await
            .expect("target update mutation should dispatch");
        let target_toggle = store
            .enqueue_put_favorite_exercise(target_exercise_id)
            .await
            .expect("target favorite mutation should dispatch");
        let other_create = store
            .enqueue_post_exercise(PostExerciseRequest {
                exercise_id: other_exercise_id,
                name: "Other draft".to_string(),
                muscle_groups: vec![MuscleGroup::Back],
                equipment: Some(Equipment::Barbell),
                description: None,
                instructions: None,
                video_url: None,
                visibility: Visibility::Private,
            })
            .await
            .expect("other create mutation should dispatch");

        let removed = store
            .remove_pending_exercise_draft_mutations(target_exercise_id)
            .await
            .expect("removing pending draft mutations should succeed");
        assert_eq!(
            removed, 2,
            "should remove exactly target create/update mutations"
        );

        assert!(
            !store
                .mutation_is_pending(&target_create)
                .await
                .expect("target create lookup should succeed"),
            "target create should be removed"
        );
        assert!(
            !store
                .mutation_is_pending(&target_update)
                .await
                .expect("target update lookup should succeed"),
            "target update should be removed"
        );
        assert!(
            store
                .mutation_is_pending(&target_toggle)
                .await
                .expect("target toggle lookup should succeed"),
            "non create/update mutation should be retained"
        );
        assert!(
            store
                .mutation_is_pending(&other_create)
                .await
                .expect("other create lookup should succeed"),
            "other exercise mutations should be retained"
        );
    }

    #[test]
    fn typed_request_intents_map_supported_workout_routes() {
        let create = post_session_intent(
            MutationId::new(),
            PostSessionRequest {
                session_id: Uuid::new_v4(),
                template_id: Some(Uuid::new_v4()),
                planned_workout_id: None,
                name: Some("Push Day".to_string()),
            },
        )
        .expect("create session should map");
        assert_eq!(create.method, "POST");
        assert_eq!(create.path, "/api/v1/sessions");
        assert_eq!(create.body["name"], serde_json::json!("Push Day"));

        let session_id = Uuid::new_v4();
        let complete = patch_session_intent(
            MutationId::new(),
            session_id,
            PatchSessionRequest {
                name: None,
                notes: Some("done".to_string()),
                status: Some(SessionMutationStatus::Completed),
                rating: Some(5),
            },
        )
        .expect("complete session should map");
        assert_eq!(complete.method, "PATCH");
        assert_eq!(complete.path, format!("/api/v1/sessions/{session_id}"));
        assert_eq!(complete.body["status"], serde_json::json!("completed"));
        assert_eq!(complete.body["rating"], serde_json::json!(5));

        let cancel = patch_session_intent(
            MutationId::new(),
            session_id,
            PatchSessionRequest {
                name: None,
                notes: None,
                status: Some(SessionMutationStatus::Cancelled),
                rating: None,
            },
        )
        .expect("cancel session should map");
        assert_eq!(cancel.method, "PATCH");
        assert_eq!(cancel.body["status"], serde_json::json!("cancelled"));
    }

    #[test]
    fn typed_request_intents_map_preferences_and_exercise_routes() {
        let preferences = put_user_preferences_intent(
            MutationId::new(),
            PutUserPreferencesRequest {
                units: Units::Kg,
                theme: Theme::Dark,
                language: Language::En,
                default_rest_seconds: 90,
            },
        )
        .expect("preferences mutation should map");
        assert_eq!(preferences.method, "PUT");
        assert_eq!(preferences.path, "/api/v1/me/preferences");

        let favorite = put_favorite_exercise_intent(MutationId::new(), Uuid::new_v4())
            .expect("favorite mutation should map");
        assert_eq!(favorite.method, "PUT");
    }

    #[test]
    fn typed_request_intents_map_translation_routes() {
        let exercise_id = Uuid::new_v4();
        let proposal_id = Uuid::new_v4();
        let proposal = post_translation_proposal_intent(
            MutationId::new(),
            exercise_id,
            PostTranslationProposalRequest {
                proposal_id,
                language: Language::Fr,
                name: Some("Développé couché".to_string()),
                description: None,
                instructions: None,
            },
        )
        .expect("translation proposal should map");
        assert_eq!(proposal.method, "POST");
        assert_eq!(
            proposal.path,
            format!("/api/v1/exercises/{exercise_id}/translation-proposals")
        );
        assert_eq!(proposal.body["proposal_id"], serde_json::json!(proposal_id));

        let reject = patch_translation_proposal_intent(
            MutationId::new(),
            proposal_id,
            PatchTranslationProposalRequest {
                status: TranslationProposalStatus::Rejected,
                feedback: Some("needs more detail".to_string()),
            },
        )
        .expect("translation rejection should map");
        assert_eq!(reject.method, "PATCH");
        assert_eq!(
            reject.path,
            format!("/api/v1/translation-proposals/{proposal_id}")
        );
        assert_eq!(reject.body["status"], serde_json::json!("rejected"));
        assert_eq!(
            reject.body["feedback"],
            serde_json::json!("needs more detail")
        );
    }

    #[tokio::test]
    async fn apply_sync_results_moves_rejected_and_blocked_mutations_to_dead_letter() {
        let db = Rc::new(
            crate::infrastructure::persistence::init_test_db()
                .await
                .expect("in-memory db should initialize"),
        );
        let store = MutationStore::from_rc(Rc::clone(&db));

        let rejected_id = store
            .enqueue_put_user_preferences(PutUserPreferencesRequest {
                units: Units::Kg,
                theme: Theme::Dark,
                language: Language::En,
                default_rest_seconds: 90,
            })
            .await
            .expect("preferences mutation should dispatch");
        let blocked_id = store
            .enqueue_put_favorite_exercise(Uuid::new_v4())
            .await
            .expect("favorite mutation should dispatch");
        let duplicate_id = store
            .enqueue_post_exercise(PostExerciseRequest {
                exercise_id: Uuid::new_v4(),
                name: "Bench Press".to_string(),
                muscle_groups: vec![MuscleGroup::Chest],
                equipment: Some(Equipment::Barbell),
                description: None,
                instructions: None,
                video_url: None,
                visibility: Visibility::Private,
            })
            .await
            .expect("create mutation should dispatch");

        let records = store
            .get_pending()
            .await
            .expect("pending outbox records should load");
        let response = MutationBatchResponse {
            results: vec![
                MutationResult {
                    mutation_id: rejected_id,
                    status: MutationStatus::Rejected,
                    error: Some(ApiError {
                        code: "mutation_rejected".to_string(),
                        message: "bad request".to_string(),
                    }),
                },
                MutationResult {
                    mutation_id: blocked_id,
                    status: MutationStatus::Blocked,
                    error: Some(ApiError {
                        code: "mutation_blocked".to_string(),
                        message: "blocked behind failure".to_string(),
                    }),
                },
                MutationResult {
                    mutation_id: duplicate_id,
                    status: MutationStatus::Duplicate,
                    error: None,
                },
            ],
        };

        let summary = store.apply_sync_results(&records, response).await;

        assert_eq!(summary.accepted_count, 0);
        assert_eq!(summary.duplicate_count, 1);
        assert_eq!(summary.rejected_count, 2);
        assert_eq!(summary.mutation_ids_to_delete.len(), 3);
        assert!(DeadLetterStore::exists(&db, &rejected_id.as_str())
            .await
            .expect("rejected dead letter lookup should succeed"));
        assert!(DeadLetterStore::exists(&db, &blocked_id.as_str())
            .await
            .expect("blocked dead letter lookup should succeed"));
        assert!(!DeadLetterStore::exists(&db, &duplicate_id.as_str())
            .await
            .expect("duplicate dead letter lookup should succeed"));
    }

    #[tokio::test]
    async fn sync_status_reports_priority_order() {
        let db = Rc::new(
            crate::infrastructure::persistence::init_test_db()
                .await
                .expect("in-memory db should initialize"),
        );
        let store = MutationStore::from_rc(db);

        assert_eq!(store.sync_status(), SyncStatus::Idle);

        store.inner.is_offline.store(true, Ordering::SeqCst);
        assert_eq!(store.sync_status(), SyncStatus::Offline);

        store.inner.is_error.store(true, Ordering::SeqCst);
        assert_eq!(store.sync_status(), SyncStatus::Error);

        store.inner.is_syncing.store(true, Ordering::SeqCst);
        assert_eq!(store.sync_status(), SyncStatus::Syncing);
    }

    #[tokio::test]
    async fn enqueue_additional_variants_and_clear_pending_cover_route_shapes() {
        let db = crate::infrastructure::persistence::init_test_db()
            .await
            .expect("in-memory db should initialize");
        let store = MutationStore::from_rc(Rc::new(db));

        let session_id = Uuid::new_v4();
        let exercise_id = Uuid::new_v4();
        let proposal_id = Uuid::new_v4();

        store
            .enqueue_post_session_set(
                session_id,
                PostSessionSetRequest {
                    activity_id: Uuid::new_v4(),
                    set_id: Uuid::new_v4(),
                    reps: Some(8),
                    weight: Some(shared::Weight::kg(100.0)),
                    rpe: Some(8),
                    notes: Some("top set".to_string()),
                    flags: Default::default(),
                },
            )
            .await
            .expect("session set mutation should dispatch");
        store
            .enqueue_delete_exercise(exercise_id)
            .await
            .expect("delete exercise mutation should dispatch");
        store
            .enqueue_delete_favorite_exercise(exercise_id)
            .await
            .expect("delete favorite mutation should dispatch");
        store
            .enqueue_patch_translation_proposal(
                proposal_id,
                PatchTranslationProposalRequest {
                    status: TranslationProposalStatus::Approved,
                    feedback: Some("looks good".to_string()),
                },
            )
            .await
            .expect("translation moderation mutation should dispatch");

        assert!(store.has_pending());
        assert_eq!(store.pending_count(), 4);

        let pending = store
            .get_pending()
            .await
            .expect("pending outbox lookup should succeed");
        let intents = pending
            .into_iter()
            .map(|record| {
                let intent = record.to_intent().expect("pending record should decode");
                ((intent.method, intent.path), intent.body)
            })
            .collect::<Vec<_>>();

        assert!(intents.iter().any(|((method, path), body)| {
            method == "POST"
                && path == &format!("/api/v1/sessions/{session_id}/sets")
                && body["reps"] == serde_json::json!(8)
                && body["weight"]["value"] == serde_json::json!(100.0)
        }));
        assert!(intents.iter().any(|((method, path), _)| {
            method == "DELETE" && path == &format!("/api/v1/exercises/{exercise_id}")
        }));
        assert!(intents.iter().any(|((method, path), _)| {
            method == "DELETE" && path == &format!("/api/v1/me/favorite-exercises/{exercise_id}")
        }));
        assert!(intents.iter().any(|((method, path), body)| {
            method == "PATCH"
                && path == &format!("/api/v1/translation-proposals/{proposal_id}")
                && body["status"] == serde_json::json!("approved")
        }));

        store
            .clear_pending()
            .await
            .expect("clearing outbox should succeed");
        assert_eq!(store.pending_count(), 0);
        assert!(!store.has_pending());
    }

    #[tokio::test]
    async fn refresh_pending_count_reloads_external_outbox_changes() {
        let db = crate::infrastructure::persistence::init_test_db()
            .await
            .expect("in-memory db should initialize");
        let store = MutationStore::from_rc(Rc::new(db));

        let mutation_id = store
            .enqueue_patch_session(
                Uuid::new_v4(),
                PatchSessionRequest {
                    name: Some("Updated".to_string()),
                    notes: None,
                    status: Some(SessionMutationStatus::InProgress),
                    rating: None,
                },
            )
            .await
            .expect("patch session mutation should dispatch");
        assert_eq!(store.pending_count(), 1);

        OutboxStore::delete(&store.inner.db, &mutation_id)
            .await
            .expect("outbox delete should succeed");
        assert_eq!(
            store.pending_count(),
            1,
            "in-memory count should remain stale until refreshed"
        );

        store
            .refresh_pending_count()
            .await
            .expect("pending count refresh should succeed");
        assert_eq!(store.pending_count(), 0);
        assert!(!store.has_pending());
    }
}

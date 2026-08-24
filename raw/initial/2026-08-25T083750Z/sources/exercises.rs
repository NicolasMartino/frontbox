use std::{collections::HashSet, future::Future, rc::Rc};

use dioxus::prelude::ServerFnError;
use shared::contracts::common::{Equipment as CommandEquipment, MuscleGroup as CommandMuscleGroup};
use shared::frontend::dto::MutationId;
use shared::{
    Exercise, ExerciseId, PatchExerciseRequest, PatchTranslationProposalRequest,
    PostExerciseRequest, PostTranslationProposalRequest,
};
use shared_ui::screens::{
    ExerciseDiscoverySummary, ExerciseDraft, ExerciseDraftVisibility, ExerciseDto,
    ExerciseFacetCount, ExerciseListMode, ExerciseModeCounts, ExerciseVisibility, ModerationStatus,
    ProposalStatus, TranslationProposalDto,
};
use uuid::Uuid;

use crate::api::exercise;
use crate::infrastructure::persistence::{Database, ExerciseStore, MutationStore};

/// Poll cadence for exercise-library refresh while the screen is active.
pub const EXERCISE_LIBRARY_POLL_INTERVAL_MS: u32 = 30_000;

/// Feature service for exercise-api intents plus read-side coordination.
#[derive(Clone)]
pub struct ExerciseService {
    db: Rc<Database>,
    mutation_store: MutationStore,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExerciseLibrarySnapshot {
    pub items: Vec<ExerciseDto>,
    pub summary: Option<ExerciseDiscoverySummary>,
    pub applied_language: Option<String>,
    pub next_cursor: Option<String>,
}

impl ExerciseLibrarySnapshot {
    fn from_cached(items: Vec<ExerciseDto>, language: &str) -> Self {
        Self {
            items,
            summary: None,
            applied_language: Some(normalize_language_code(language)),
            next_cursor: None,
        }
    }

    fn empty(language: &str) -> Self {
        Self {
            items: Vec::new(),
            summary: None,
            applied_language: Some(normalize_language_code(language)),
            next_cursor: None,
        }
    }
}

impl ExerciseService {
    pub fn new(db: Rc<Database>, mutation_store: MutationStore) -> Self {
        Self { db, mutation_store }
    }

    /// Dispatch a create mutation from an exercise draft.
    ///
    /// Returns `(exercise_id, mutation_id)`.
    pub async fn dispatch_create_from_draft(
        &self,
        draft: &ExerciseDraft,
    ) -> Result<(Uuid, MutationId), String> {
        let exercise_id = draft.exercise_id.unwrap_or_else(Uuid::now_v7);
        let name = trim_option(draft.name.clone())
            .ok_or_else(|| "exercise_draft_missing_name".to_string())?;
        let muscle_groups = map_draft_muscle_groups(&draft.muscle_groups);
        if muscle_groups.is_empty() {
            return Err("exercise_draft_missing_muscle_groups".to_string());
        }

        let request = PostExerciseRequest {
            exercise_id,
            name,
            muscle_groups,
            equipment: map_draft_equipment(draft.equipment.as_deref()),
            description: trim_option(draft.description.clone()),
            instructions: trim_option(draft.instructions.clone()),
            video_url: trim_option(draft.video_url.clone()),
            visibility: map_draft_visibility(draft.visibility),
        };

        let mutation_id = self
            .mutation_store
            .enqueue_post_exercise(request)
            .await
            .map_err(|e| e.to_string())?;
        Ok((exercise_id, mutation_id))
    }

    /// Dispatch an update mutation from an exercise draft.
    pub async fn dispatch_update_from_draft(
        &self,
        draft: &ExerciseDraft,
    ) -> Result<MutationId, String> {
        let exercise_id = draft
            .exercise_id
            .ok_or_else(|| "Cannot dispatch update without exercise_id".to_string())?;
        let muscle_groups = map_draft_muscle_groups(&draft.muscle_groups);

        let request = PatchExerciseRequest {
            name: trim_option(draft.name.clone()),
            muscle_groups: if muscle_groups.is_empty() {
                None
            } else {
                Some(muscle_groups)
            },
            equipment: map_draft_equipment(draft.equipment.as_deref()),
            description: trim_option(draft.description.clone()),
            instructions: trim_option(draft.instructions.clone()),
            video_url: trim_option(draft.video_url.clone()),
            visibility: Some(map_draft_visibility(draft.visibility)),
        };

        self.mutation_store
            .enqueue_patch_exercise(exercise_id, request)
            .await
            .map_err(|e| e.to_string())
    }

    /// Remove pending draft create/update mutations for an exercise.
    pub async fn remove_pending_draft_mutations(&self, exercise_id: Uuid) -> Result<usize, String> {
        self.mutation_store
            .remove_pending_exercise_draft_mutations(exercise_id)
            .await
            .map_err(|e| e.to_string())
    }

    /// Queue an exercise delete mutation.
    pub async fn dispatch_delete_exercise(&self, exercise_id: Uuid) -> Result<MutationId, String> {
        self.mutation_store
            .enqueue_delete_exercise(exercise_id)
            .await
            .map_err(|e| e.to_string())
    }

    /// Return true if a mutation id still exists in outbox pending state.
    pub async fn mutation_is_pending(&self, mutation_id: &str) -> Result<bool, String> {
        let mutation_id = MutationId::from_string(mutation_id)
            .map_err(|err| format!("invalid mutation_id '{}': {}", mutation_id, err))?;
        self.mutation_store
            .mutation_is_pending(&mutation_id)
            .await
            .map_err(|e| e.to_string())
    }

    /// Return true if a mutation id exists in dead-letter storage.
    pub async fn mutation_was_rejected(&self, mutation_id: &str) -> Result<bool, String> {
        let mutation_id = MutationId::from_string(mutation_id)
            .map_err(|err| format!("invalid mutation_id '{}': {}", mutation_id, err))?;
        self.mutation_store
            .mutation_was_rejected(&mutation_id)
            .await
            .map_err(|e| e.to_string())
    }

    /// Queue a translation proposal mutation.
    pub async fn dispatch_translation_proposal(
        &self,
        exercise_id: Uuid,
        request: PostTranslationProposalRequest,
    ) -> Result<MutationId, String> {
        self.mutation_store
            .enqueue_post_translation_proposal(exercise_id, request)
            .await
            .map_err(|e| e.to_string())
    }

    /// Queue a translation moderation mutation.
    pub async fn dispatch_translation_moderation(
        &self,
        proposal_id: Uuid,
        request: PatchTranslationProposalRequest,
    ) -> Result<MutationId, String> {
        self.mutation_store
            .enqueue_patch_translation_proposal(proposal_id, request)
            .await
            .map_err(|e| e.to_string())
    }

    /// Update favorite state with optimistic local projection.
    pub async fn toggle_favorite_optimistic(
        &self,
        exercise_id: Uuid,
    ) -> Result<MutationId, String> {
        let exercise_key = ExerciseId::from_uuid(exercise_id);
        let before = ExerciseStore::get(&self.db, &exercise_key)
            .await
            .map_err(|e| format!("Failed to load cached exercise {}: {}", exercise_id, e))?;
        let next_is_favorite = before
            .as_ref()
            .map(|exercise| !exercise.is_favorite)
            .unwrap_or(true);

        if let Some(mut exercise) = before.clone() {
            exercise.is_favorite = next_is_favorite;
            ExerciseStore::put(&self.db, &exercise)
                .await
                .map_err(|e| format!("Failed to apply optimistic favorite toggle: {}", e))?;
        }

        let queued = if next_is_favorite {
            self.mutation_store
                .enqueue_put_favorite_exercise(exercise_id)
                .await
        } else {
            self.mutation_store
                .enqueue_delete_favorite_exercise(exercise_id)
                .await
        };
        match queued {
            Ok(mutation_id) => Ok(mutation_id),
            Err(err) => {
                if let Some(previous) = before {
                    if let Err(revert_err) = ExerciseStore::put(&self.db, &previous).await {
                        crate::log!(
                            "[ExerciseService] Failed to revert optimistic favorite toggle: {}",
                            revert_err
                        );
                    }
                }
                Err(err)
            }
        }
    }

    /// Load a library mode with cache-first fallback and server refresh.
    pub async fn load_library(
        &self,
        auth_header: &str,
        user_id: Uuid,
        query: &shared::ExerciseSearchQuery,
    ) -> Result<ExerciseLibrarySnapshot, String> {
        self.load_library_with_searcher(auth_header, user_id, query, |auth_header, query| {
            exercise::search_exercises(auth_header, query)
        })
        .await
    }

    async fn load_library_with_searcher<F, Fut>(
        &self,
        auth_header: &str,
        user_id: Uuid,
        query: &shared::ExerciseSearchQuery,
        searcher: F,
    ) -> Result<ExerciseLibrarySnapshot, String>
    where
        F: FnMut(String, shared::ExerciseSearchQuery) -> Fut,
        Fut: Future<Output = Result<shared::ExerciseSearchResponse, ServerFnError>>,
    {
        let cached = self.read_library_from_cache(user_id, query).await?;
        let language = query.language.as_deref().unwrap_or("en");
        let can_use_cache = can_use_cached_discovery_fallback(query, language);

        if auth_header.trim().is_empty() {
            return if can_use_cache {
                Ok(ExerciseLibrarySnapshot::from_cached(cached, language))
            } else {
                Ok(ExerciseLibrarySnapshot::empty(language))
            };
        }

        match self
            .refresh_library_with_searcher(auth_header, user_id, query, searcher)
            .await
        {
            Ok(remote) => Ok(remote),
            Err(err) if !cached.is_empty() && can_use_cache => {
                crate::log!(
                    "[ExerciseService] refresh failed for mode {:?}, using cache: {}",
                    search_query_mode(query),
                    err
                );
                Ok(ExerciseLibrarySnapshot::from_cached(cached, language))
            }
            Err(err) => Err(err),
        }
    }

    /// Refresh a library mode from exercise-api and update local cache.
    pub async fn refresh_library(
        &self,
        auth_header: &str,
        user_id: Uuid,
        query: &shared::ExerciseSearchQuery,
    ) -> Result<ExerciseLibrarySnapshot, String> {
        self.refresh_library_with_searcher(auth_header, user_id, query, |auth_header, query| {
            exercise::search_exercises(auth_header, query)
        })
        .await
    }

    async fn refresh_library_with_searcher<F, Fut>(
        &self,
        auth_header: &str,
        user_id: Uuid,
        query: &shared::ExerciseSearchQuery,
        searcher: F,
    ) -> Result<ExerciseLibrarySnapshot, String>
    where
        F: FnMut(String, shared::ExerciseSearchQuery) -> Fut,
        Fut: Future<Output = Result<shared::ExerciseSearchResponse, ServerFnError>>,
    {
        self.refresh_library_window_with_searcher(auth_header, user_id, query, searcher)
            .await
    }

    async fn refresh_library_window_with_searcher<F, Fut>(
        &self,
        auth_header: &str,
        user_id: Uuid,
        query: &shared::ExerciseSearchQuery,
        mut searcher: F,
    ) -> Result<ExerciseLibrarySnapshot, String>
    where
        F: FnMut(String, shared::ExerciseSearchQuery) -> Fut,
        Fut: Future<Output = Result<shared::ExerciseSearchResponse, ServerFnError>>,
    {
        let requested_cursor = query.cursor.clone();
        let mut next_request_cursor = None::<String>;
        let mut seen_request_cursors = HashSet::<Option<String>>::new();
        let mut accumulated = ExerciseLibrarySnapshot::default();

        loop {
            if !seen_request_cursors.insert(next_request_cursor.clone()) {
                return Err(format!(
                    "Exercise search pagination loop detected for mode '{}'",
                    query.mode.as_deref().unwrap_or("discover")
                ));
            }

            let mut page_query = query.clone();
            page_query.cursor = next_request_cursor.clone();

            let response = searcher(auth_header.to_string(), page_query)
                .await
                .map_err(|e| {
                    format!(
                        "Failed to search exercise mode '{}' from server: {}",
                        query.mode.as_deref().unwrap_or("discover"),
                        e
                    )
                })?;
            let page = map_search_response_to_snapshot(response, user_id);
            merge_snapshot_page(&mut accumulated, page);

            if next_request_cursor == requested_cursor {
                return Ok(accumulated);
            }

            let Some(next_page_cursor) = accumulated.next_cursor.clone() else {
                return Ok(accumulated);
            };
            next_request_cursor = Some(next_page_cursor);
        }
    }

    /// Read a library mode directly from local cache.
    pub async fn read_library_from_cache(
        &self,
        user_id: Uuid,
        query: &shared::ExerciseSearchQuery,
    ) -> Result<Vec<ExerciseDto>, String> {
        let mode = search_query_mode(query);
        let mut exercises = ExerciseStore::get_all(&self.db)
            .await
            .map_err(|e| format!("Failed to read exercise cache: {}", e))?
            .into_iter()
            .filter(|exercise| matches_mode(exercise, user_id, mode))
            .map(|exercise| map_exercise_to_dto(exercise, user_id))
            .collect::<Vec<_>>();

        apply_search_filter(&mut exercises, query.search.as_deref());
        apply_discovery_query_filters(&mut exercises, query);
        sort_exercises(&mut exercises);
        Ok(exercises)
    }

    /// Load detail with server-first and cache fallback behavior.
    pub async fn load_detail(
        &self,
        auth_header: &str,
        user_id: Uuid,
        exercise_id: Uuid,
    ) -> Result<ExerciseDto, String> {
        self.load_detail_with_fetcher(
            auth_header,
            user_id,
            exercise_id,
            exercise::fetch_exercise_detail,
        )
        .await
    }

    async fn load_detail_with_fetcher<F, Fut>(
        &self,
        auth_header: &str,
        user_id: Uuid,
        exercise_id: Uuid,
        fetcher: F,
    ) -> Result<ExerciseDto, String>
    where
        F: FnOnce(String, String) -> Fut,
        Fut: Future<Output = Result<shared::Exercise, ServerFnError>>,
    {
        let cache_key = ExerciseId::from_uuid(exercise_id);
        let cached = ExerciseStore::get(&self.db, &cache_key)
            .await
            .map_err(|e| format!("Failed to read cached exercise detail: {}", e))?
            .map(|exercise| map_exercise_to_dto(exercise, user_id));

        if auth_header.trim().is_empty() {
            return cached.ok_or_else(|| {
                format!(
                    "Cannot load exercise detail {} without authentication or cache",
                    exercise_id
                )
            });
        }

        match fetcher(auth_header.to_string(), exercise_id.to_string()).await {
            Ok(exercise) => {
                if let Err(err) = ExerciseStore::put(&self.db, &exercise).await {
                    crate::log!(
                        "[ExerciseService] Failed to cache exercise detail {}: {}",
                        exercise_id,
                        err
                    );
                }
                Ok(map_exercise_to_dto(exercise, user_id))
            }
            Err(err) => cached.ok_or_else(|| {
                format!(
                    "Failed to fetch exercise detail {} from server and cache is empty: {}",
                    exercise_id, err
                )
            }),
        }
    }

    /// Fetch moderation queue items for moderator workflows.
    pub async fn load_moderation_queue(
        &self,
        auth_header: &str,
        status: Option<&str>,
    ) -> Result<Vec<TranslationProposalDto>, String> {
        let summaries = exercise::fetch_moderation_proposals(
            auth_header.to_string(),
            status.map(|value| value.to_string()),
        )
        .await
        .map_err(|e| format!("Failed to fetch moderation proposals: {}", e))?;

        let proposals = summaries
            .into_iter()
            .map(|summary| TranslationProposalDto {
                id: summary.id,
                exercise_id: summary.exercise_id,
                exercise_name: format!("Exercise {}", &summary.exercise_id.to_string()[..8]),
                canonical_language: "en".to_string(),
                language_code: summary.language_code.clone(),
                proposed_name: summary.summary.clone().unwrap_or_default(),
                proposed_description: summary.summary,
                proposed_instructions: None,
                proposer_username: summary.proposed_by.to_string(),
                submitted_at: "recently".to_string(),
                moderation_status: parse_moderation_status(&summary.status),
                ai_confidence: None,
                ai_reason_codes: Vec::new(),
                proposal_status: parse_proposal_status(&summary.status),
            })
            .collect();

        Ok(proposals)
    }

    /// Cross-platform sleep used by polling loops.
    pub async fn sleep_poll_interval() {
        #[cfg(target_arch = "wasm32")]
        {
            gloo_timers::future::TimeoutFuture::new(EXERCISE_LIBRARY_POLL_INTERVAL_MS).await;
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::time::sleep(std::time::Duration::from_millis(
                EXERCISE_LIBRARY_POLL_INTERVAL_MS as u64,
            ))
            .await;
        }
    }
}

fn parse_moderation_status(status: &str) -> ModerationStatus {
    match status {
        "pending_ai_review" => ModerationStatus::PendingAiReview,
        "auto_approved" => ModerationStatus::AutoApproved,
        "blocked_by_ai" => ModerationStatus::BlockedByAi,
        "needs_human_review" => ModerationStatus::NeedsHumanReview,
        "approved_by_human" => ModerationStatus::ApprovedByHuman,
        "rejected_by_human" => ModerationStatus::RejectedByHuman,
        "pending" => ModerationStatus::NeedsHumanReview,
        "approved" => ModerationStatus::ApprovedByHuman,
        "rejected" => ModerationStatus::RejectedByHuman,
        "blocked" => ModerationStatus::BlockedByAi,
        _ => ModerationStatus::PendingAiReview,
    }
}

fn parse_proposal_status(status: &str) -> ProposalStatus {
    match status {
        "approved" => ProposalStatus::Approved,
        "rejected" => ProposalStatus::Rejected,
        _ => ProposalStatus::Pending,
    }
}

fn matches_mode(exercise: &Exercise, user_id: Uuid, mode: ExerciseListMode) -> bool {
    match mode {
        ExerciseListMode::Discover => matches!(exercise.visibility, shared::Visibility::Public),
        ExerciseListMode::Mine => is_owned_by_user(exercise, user_id),
        ExerciseListMode::Favorites => exercise.is_favorite,
    }
}

fn map_exercise_to_dto(exercise: Exercise, user_id: Uuid) -> ExerciseDto {
    let is_owner = is_owned_by_user(&exercise, user_id);
    let visibility = match exercise.visibility {
        shared::Visibility::Public => ExerciseVisibility::Public,
        shared::Visibility::Unlisted => ExerciseVisibility::Unlisted,
        shared::Visibility::Private => ExerciseVisibility::Private,
    };

    ExerciseDto {
        id: *exercise.id.as_uuid(),
        name: exercise.name,
        description: exercise.description,
        instructions: None,
        video_url: None,
        canonical_language: "en".to_string(),
        muscle_groups: exercise
            .muscle_groups
            .iter()
            .map(|group| group.display_name().to_string())
            .collect(),
        equipment: vec![exercise.equipment.display_name().to_string()],
        visibility,
        moderation_status: None,
        available_languages: vec!["en".to_string()],
        is_favorite: exercise.is_favorite,
        is_owner,
    }
}

fn is_owned_by_user(exercise: &Exercise, user_id: Uuid) -> bool {
    exercise
        .owner_id
        .as_ref()
        .map(|owner| owner.as_uuid() == &user_id)
        .unwrap_or(false)
}

fn apply_search_filter(exercises: &mut Vec<ExerciseDto>, search_query: Option<&str>) {
    let Some(search) = normalize_search_query(search_query) else {
        return;
    };

    let needle = search.to_ascii_lowercase();
    exercises.retain(|exercise| {
        exercise.name.to_ascii_lowercase().contains(&needle)
            || exercise
                .description
                .as_ref()
                .map(|value| value.to_ascii_lowercase().contains(&needle))
                .unwrap_or(false)
    });
}

fn normalize_search_query(search_query: Option<&str>) -> Option<&str> {
    search_query
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn normalize_language_code(language: &str) -> String {
    let trimmed = language.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        "en".to_string()
    } else {
        trimmed
    }
}

fn can_use_cached_discovery_fallback(
    query: &shared::ExerciseSearchQuery,
    requested_language: &str,
) -> bool {
    normalize_language_code(requested_language) == "en"
        && query.cursor.is_none()
        && matches!(
            search_query_mode(query),
            ExerciseListMode::Mine | ExerciseListMode::Favorites
        )
}

fn map_search_response_to_snapshot(
    response: shared::ExerciseSearchResponse,
    user_id: Uuid,
) -> ExerciseLibrarySnapshot {
    ExerciseLibrarySnapshot {
        items: response
            .items
            .into_iter()
            .map(|item| map_search_item_to_dto(item, user_id))
            .collect(),
        summary: Some(map_search_summary(response.summary)),
        applied_language: Some(response.applied_language),
        next_cursor: response.next_cursor,
    }
}

fn merge_snapshot_page(
    accumulated: &mut ExerciseLibrarySnapshot,
    mut page: ExerciseLibrarySnapshot,
) {
    for item in page.items.drain(..) {
        if let Some(existing) = accumulated
            .items
            .iter_mut()
            .find(|existing| existing.id == item.id)
        {
            *existing = item;
        } else {
            accumulated.items.push(item);
        }
    }

    if accumulated.summary.is_none() {
        accumulated.summary = page.summary.take();
    }
    if accumulated.applied_language.is_none() {
        accumulated.applied_language = page.applied_language.take();
    }
    accumulated.next_cursor = page.next_cursor.take();
}

fn search_query_mode(query: &shared::ExerciseSearchQuery) -> ExerciseListMode {
    match query.mode.as_deref() {
        Some("mine") => ExerciseListMode::Mine,
        Some("favorites") => ExerciseListMode::Favorites,
        _ => ExerciseListMode::Discover,
    }
}

fn apply_discovery_query_filters(
    exercises: &mut Vec<ExerciseDto>,
    query: &shared::ExerciseSearchQuery,
) {
    if !query.muscle_group.is_empty() {
        exercises.retain(|exercise| {
            exercise.muscle_groups.iter().any(|value| {
                query.muscle_group.iter().any(|selected| {
                    normalized_filter_token(value) == normalized_filter_token(selected)
                })
            })
        });
    }

    if !query.equipment.is_empty() {
        exercises.retain(|exercise| {
            exercise.equipment.iter().any(|value| {
                query.equipment.iter().any(|selected| {
                    normalized_filter_token(value) == normalized_filter_token(selected)
                })
            })
        });
    }

    if query.favorites_only {
        exercises.retain(|exercise| exercise.is_favorite);
    }

    if query.owned_only {
        exercises.retain(|exercise| exercise.is_owner);
    }

    if !query.visibility.is_empty() {
        exercises.retain(|exercise| {
            query
                .visibility
                .iter()
                .any(|selected| matches_visibility_filter(exercise, selected))
        });
    }
}

fn matches_visibility_filter(exercise: &ExerciseDto, selected: &str) -> bool {
    match selected.trim().to_ascii_lowercase().as_str() {
        "public" => matches!(exercise.visibility, ExerciseVisibility::Public),
        "private" | "unlisted" => matches!(
            exercise.visibility,
            ExerciseVisibility::Private | ExerciseVisibility::Unlisted
        ),
        _ => true,
    }
}

fn normalized_filter_token(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace([' ', '-'], "_")
}

fn map_search_item_to_dto(item: shared::ExerciseReadItem, user_id: Uuid) -> ExerciseDto {
    ExerciseDto {
        id: item.id,
        name: item.name,
        description: item.description,
        instructions: item.instructions,
        video_url: item.video_url,
        canonical_language: item.canonical_language,
        muscle_groups: item.muscle_groups,
        equipment: item
            .equipment
            .map(|equipment| vec![equipment])
            .unwrap_or_else(|| vec!["bodyweight".to_string()]),
        visibility: match item.visibility.as_str() {
            "public" => ExerciseVisibility::Public,
            "unlisted" => ExerciseVisibility::Unlisted,
            _ => ExerciseVisibility::Private,
        },
        moderation_status: Some(parse_moderation_status(&item.moderation_status)),
        available_languages: item.available_languages,
        is_favorite: item.is_favorite,
        is_owner: item.owner_id == Some(user_id),
    }
}

fn map_search_summary(summary: shared::ExerciseSearchSummary) -> ExerciseDiscoverySummary {
    ExerciseDiscoverySummary {
        total_results: summary.total_results,
        mode_counts: ExerciseModeCounts {
            discover: summary.mode_counts.discover,
            mine: summary.mode_counts.mine,
            favorites: summary.mode_counts.favorites,
        },
        translated_results: summary.translated_results,
        muscle_group_counts: map_search_facet_counts(summary.muscle_group_counts),
        equipment_counts: map_search_facet_counts(summary.equipment_counts),
        language_counts: map_search_facet_counts(summary.language_counts),
    }
}

fn map_search_facet_counts(
    counts: Vec<shared::ExerciseSearchFacetCount>,
) -> Vec<ExerciseFacetCount> {
    counts
        .into_iter()
        .map(|count| ExerciseFacetCount {
            id: count.id,
            label: count.label,
            count: count.count,
            selected: false,
        })
        .collect()
}

fn sort_exercises(exercises: &mut [ExerciseDto]) {
    exercises.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn map_draft_muscle_groups(values: &[String]) -> Vec<CommandMuscleGroup> {
    values
        .iter()
        .filter_map(|value| map_draft_muscle_group(value))
        .collect::<Vec<_>>()
}

fn map_draft_muscle_group(value: &str) -> Option<CommandMuscleGroup> {
    match value.trim().to_ascii_lowercase().as_str() {
        "chest" => Some(CommandMuscleGroup::Chest),
        "back" => Some(CommandMuscleGroup::Back),
        "shoulders" => Some(CommandMuscleGroup::Shoulders),
        "biceps" => Some(CommandMuscleGroup::Biceps),
        "triceps" => Some(CommandMuscleGroup::Triceps),
        "forearms" => Some(CommandMuscleGroup::Forearms),
        "core" => Some(CommandMuscleGroup::Core),
        "quads" | "quadriceps" => Some(CommandMuscleGroup::Quads),
        "hamstrings" => Some(CommandMuscleGroup::Hamstrings),
        "glutes" => Some(CommandMuscleGroup::Glutes),
        "calves" => Some(CommandMuscleGroup::Calves),
        "full_body" | "fullbody" | "full-body" => Some(CommandMuscleGroup::FullBody),
        _ => None,
    }
}

fn map_draft_equipment(value: Option<&str>) -> Option<CommandEquipment> {
    let equipment = match value.map(|item| item.trim().to_ascii_lowercase()) {
        Some(value) if value == "barbell" => CommandEquipment::Barbell,
        Some(value) if value == "dumbbell" => CommandEquipment::Dumbbell,
        Some(value) if value == "kettlebell" => CommandEquipment::Kettlebell,
        Some(value) if value == "cable" => CommandEquipment::Cable,
        Some(value) if value == "machine" => CommandEquipment::Machine,
        Some(value) if value == "bodyweight" => CommandEquipment::Bodyweight,
        Some(value) if value == "resistance_band" || value == "bands" => CommandEquipment::Bands,
        Some(value) if value == "none" => CommandEquipment::None,
        Some(_) => CommandEquipment::Other,
        None => return Some(CommandEquipment::Bodyweight),
    };

    Some(equipment)
}

fn map_draft_visibility(visibility: ExerciseDraftVisibility) -> shared::Visibility {
    match visibility {
        ExerciseDraftVisibility::Private => shared::Visibility::Private,
        ExerciseDraftVisibility::Unlisted => shared::Visibility::Unlisted,
        ExerciseDraftVisibility::Public => shared::Visibility::Public,
    }
}

fn trim_option(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::cell::RefCell;

    async fn test_service() -> (ExerciseService, MutationStore) {
        let db = crate::infrastructure::persistence::init_test_db()
            .await
            .expect("in-memory db should initialize");
        let db = Rc::new(db);
        let mutation_store = MutationStore::from_rc(db.clone());
        let service = ExerciseService::new(db, mutation_store.clone());
        (service, mutation_store)
    }

    fn sample_exercise(
        id: Uuid,
        owner_id: Option<Uuid>,
        is_favorite: bool,
        name: &str,
        description: Option<&str>,
    ) -> Exercise {
        Exercise {
            id: ExerciseId::from_uuid(id),
            name: name.to_string(),
            description: description.map(str::to_string),
            muscle_groups: vec![shared::MuscleGroup::Chest],
            equipment: shared::Equipment::Barbell,
            is_custom: owner_id.is_some(),
            is_favorite,
            visibility: shared::Visibility::Public,
            owner_id: owner_id.map(shared::UserId::from_uuid),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn map_muscle_groups_empty_stays_empty() {
        let groups = map_draft_muscle_groups(&[]);
        assert!(groups.is_empty());

        let blank_groups = map_draft_muscle_groups(&[" ".to_string(), "\n".to_string()]);
        assert!(
            blank_groups.is_empty(),
            "blank values should be dropped the same way as missing values"
        );
    }

    #[test]
    fn map_muscle_groups_supports_aliases() {
        let groups = map_draft_muscle_groups(&[
            " quadriceps ".to_string(),
            "quads".to_string(),
            "full-body".to_string(),
            "FULL_BODY".to_string(),
        ]);
        assert_eq!(
            groups,
            vec![
                CommandMuscleGroup::Quads,
                CommandMuscleGroup::Quads,
                CommandMuscleGroup::FullBody,
                CommandMuscleGroup::FullBody
            ]
        );
    }

    #[test]
    fn map_muscle_groups_drops_unknown_values() {
        let groups = map_draft_muscle_groups(&[
            "unknown".to_string(),
            " core ".to_string(),
            "nonsense".to_string(),
            "SHOULDERS".to_string(),
        ]);
        assert_eq!(
            groups,
            vec![CommandMuscleGroup::Core, CommandMuscleGroup::Shoulders]
        );
    }

    #[test]
    fn map_equipment_defaults_to_bodyweight() {
        assert_eq!(
            map_draft_equipment(None),
            Some(CommandEquipment::Bodyweight)
        );
        assert_eq!(
            map_draft_equipment(Some("barbell")),
            Some(CommandEquipment::Barbell)
        );
        assert_eq!(
            map_draft_equipment(Some(" bands ")),
            Some(CommandEquipment::Bands)
        );
        assert_eq!(
            map_draft_equipment(Some("mystery-device")),
            Some(CommandEquipment::Other)
        );
    }

    #[test]
    fn map_visibility_variants() {
        let cases = [
            (
                ExerciseDraftVisibility::Private,
                shared::Visibility::Private,
            ),
            (
                ExerciseDraftVisibility::Unlisted,
                shared::Visibility::Unlisted,
            ),
            (ExerciseDraftVisibility::Public, shared::Visibility::Public),
        ];

        for (input, expected) in cases {
            assert_eq!(map_draft_visibility(input), expected);
        }
    }

    #[test]
    fn moderation_and_proposal_status_mapping_covers_aliases() {
        assert_eq!(
            parse_moderation_status("pending_ai_review"),
            ModerationStatus::PendingAiReview
        );
        assert_eq!(
            parse_moderation_status("needs_human_review"),
            ModerationStatus::NeedsHumanReview
        );
        assert_eq!(
            parse_moderation_status("auto_approved"),
            ModerationStatus::AutoApproved
        );
        assert_eq!(
            parse_moderation_status("approved"),
            ModerationStatus::ApprovedByHuman
        );
        assert_eq!(
            parse_moderation_status("approved_by_human"),
            ModerationStatus::ApprovedByHuman
        );
        assert_eq!(
            parse_moderation_status("blocked"),
            ModerationStatus::BlockedByAi
        );
        assert_eq!(
            parse_moderation_status("blocked_by_ai"),
            ModerationStatus::BlockedByAi
        );
        assert_eq!(
            parse_moderation_status("rejected"),
            ModerationStatus::RejectedByHuman
        );
        assert_eq!(
            parse_moderation_status("rejected_by_human"),
            ModerationStatus::RejectedByHuman
        );
        assert_eq!(
            parse_moderation_status("pending"),
            ModerationStatus::NeedsHumanReview
        );
        assert_eq!(parse_proposal_status("approved"), ProposalStatus::Approved);
        assert_eq!(parse_proposal_status("rejected"), ProposalStatus::Rejected);
        assert_eq!(parse_proposal_status("pending"), ProposalStatus::Pending);
        assert_eq!(parse_proposal_status("unknown"), ProposalStatus::Pending);
    }

    #[test]
    fn matches_mode_respects_owner_and_favorite_flags() {
        let owner_id = Uuid::new_v4();
        let mine = sample_exercise(Uuid::new_v4(), Some(owner_id), false, "Mine", None);
        let favorite = sample_exercise(Uuid::new_v4(), None, true, "Favorite", None);
        let stranger = sample_exercise(Uuid::new_v4(), None, false, "Stranger", None);
        let owned_favorite =
            sample_exercise(Uuid::new_v4(), Some(owner_id), true, "Owned Favorite", None);
        let mut private_owned =
            sample_exercise(Uuid::new_v4(), Some(owner_id), false, "Private Owned", None);
        private_owned.visibility = shared::Visibility::Private;

        assert!(matches_mode(&mine, owner_id, ExerciseListMode::Mine));
        assert!(!matches_mode(&favorite, owner_id, ExerciseListMode::Mine));
        assert!(matches_mode(
            &favorite,
            owner_id,
            ExerciseListMode::Favorites
        ));
        assert!(matches_mode(&mine, owner_id, ExerciseListMode::Discover));
        assert!(matches_mode(
            &stranger,
            owner_id,
            ExerciseListMode::Discover
        ));
        assert!(!matches_mode(
            &stranger,
            owner_id,
            ExerciseListMode::Favorites
        ));
        assert!(matches_mode(
            &owned_favorite,
            owner_id,
            ExerciseListMode::Discover
        ));
        assert!(matches_mode(
            &owned_favorite,
            owner_id,
            ExerciseListMode::Favorites
        ));
        assert!(!matches_mode(
            &private_owned,
            owner_id,
            ExerciseListMode::Discover
        ));
        assert!(matches_mode(
            &private_owned,
            owner_id,
            ExerciseListMode::Mine
        ));
    }

    #[test]
    fn apply_search_filter_matches_name_and_description() {
        let owner_id = Uuid::new_v4();
        let mut exercises = vec![
            map_exercise_to_dto(
                sample_exercise(
                    Uuid::new_v4(),
                    Some(owner_id),
                    false,
                    "Bench Press",
                    Some("Chest strength"),
                ),
                owner_id,
            ),
            map_exercise_to_dto(
                sample_exercise(Uuid::new_v4(), None, false, "Row", Some("Upper back")),
                owner_id,
            ),
        ];

        apply_search_filter(&mut exercises, Some("chest"));
        assert_eq!(exercises.len(), 1);
        assert_eq!(exercises[0].name, "Bench Press");
        assert_eq!(exercises[0].description.as_deref(), Some("Chest strength"));

        let mut exercises = vec![
            map_exercise_to_dto(
                sample_exercise(
                    Uuid::new_v4(),
                    Some(owner_id),
                    false,
                    "Bench Press",
                    Some("Chest strength"),
                ),
                owner_id,
            ),
            map_exercise_to_dto(
                sample_exercise(Uuid::new_v4(), None, false, "Row", Some("Upper back")),
                owner_id,
            ),
        ];
        apply_search_filter(&mut exercises, Some("BENCH"));
        assert_eq!(exercises.len(), 1);
        assert_eq!(exercises[0].name, "Bench Press");

        let mut exercises = vec![
            map_exercise_to_dto(
                sample_exercise(
                    Uuid::new_v4(),
                    Some(owner_id),
                    false,
                    "Bench Press",
                    Some("Chest strength"),
                ),
                owner_id,
            ),
            map_exercise_to_dto(
                sample_exercise(Uuid::new_v4(), None, false, "Row", Some("Upper back")),
                owner_id,
            ),
        ];
        apply_search_filter(&mut exercises, Some("  "));
        assert_eq!(
            exercises.len(),
            2,
            "blank search terms should leave the list untouched"
        );

        let mut exercises = vec![
            map_exercise_to_dto(
                sample_exercise(
                    Uuid::new_v4(),
                    Some(owner_id),
                    false,
                    "Bench Press",
                    Some("Chest strength"),
                ),
                owner_id,
            ),
            map_exercise_to_dto(
                sample_exercise(Uuid::new_v4(), None, false, "Row", Some("Upper back")),
                owner_id,
            ),
        ];
        apply_search_filter(&mut exercises, Some("  BACK "));
        assert_eq!(exercises.len(), 1);
        assert_eq!(exercises[0].name, "Row");
    }

    #[test]
    fn sort_exercises_orders_case_insensitively_then_by_id() {
        let mut exercises = vec![
            ExerciseDto {
                id: Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
                name: "bench".to_string(),
                description: None,
                instructions: None,
                video_url: None,
                canonical_language: "en".to_string(),
                muscle_groups: vec![],
                equipment: vec![],
                visibility: ExerciseVisibility::Public,
                moderation_status: None,
                available_languages: vec![],
                is_favorite: false,
                is_owner: false,
            },
            ExerciseDto {
                id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
                name: "Bench".to_string(),
                description: None,
                instructions: None,
                video_url: None,
                canonical_language: "en".to_string(),
                muscle_groups: vec![],
                equipment: vec![],
                visibility: ExerciseVisibility::Public,
                moderation_status: None,
                available_languages: vec![],
                is_favorite: false,
                is_owner: false,
            },
            ExerciseDto {
                id: Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap(),
                name: "curl".to_string(),
                description: None,
                instructions: None,
                video_url: None,
                canonical_language: "en".to_string(),
                muscle_groups: vec![],
                equipment: vec![],
                visibility: ExerciseVisibility::Public,
                moderation_status: None,
                available_languages: vec![],
                is_favorite: false,
                is_owner: false,
            },
        ];

        sort_exercises(&mut exercises);

        assert_eq!(exercises[0].name, "Bench");
        assert_eq!(
            exercises[0].id,
            Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
        );
        assert_eq!(exercises[1].name, "bench");
        assert_eq!(
            exercises[1].id,
            Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap()
        );
        assert_eq!(exercises[2].name, "curl");
        assert_eq!(
            exercises[2].id,
            Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap()
        );
    }

    #[tokio::test]
    async fn mutation_lifecycle_helpers_track_pending_and_rejected_states() {
        let db = crate::infrastructure::persistence::init_test_db()
            .await
            .expect("in-memory db should initialize");
        let db = Rc::new(db);
        let mutation_store = MutationStore::from_rc(db.clone());
        let service = ExerciseService::new(db.clone(), mutation_store);

        let mutation_id = service
            .mutation_store
            .enqueue_put_favorite_exercise(Uuid::new_v4())
            .await
            .expect("favorite mutation should dispatch");
        let mutation_id_value = mutation_id.as_str();

        assert!(
            service
                .mutation_is_pending(&mutation_id_value)
                .await
                .expect("pending lookup should succeed"),
            "freshly queued mutation should be pending"
        );
        assert!(
            !service
                .mutation_was_rejected(&mutation_id_value)
                .await
                .expect("rejected lookup should succeed"),
            "freshly queued mutation should not be rejected"
        );

        service
            .mutation_store
            .mark_synced(&mutation_id)
            .await
            .expect("mutation should be marked synced");

        assert!(
            !service
                .mutation_is_pending(&mutation_id_value)
                .await
                .expect("pending lookup should succeed after sync"),
            "synced mutation should no longer be pending"
        );
        assert!(
            !service
                .mutation_was_rejected(&mutation_id_value)
                .await
                .expect("rejected lookup should succeed before dead-lettering"),
            "synced mutation should not be rejected until it is dead-lettered"
        );
        let rejected_record = crate::infrastructure::persistence::DeadLetterRecord::new(
            mutation_id_value.clone(),
            "PUT".to_string(),
            format!("/api/v1/me/favorite-exercises/{}", Uuid::new_v4()),
            "{}".to_string(),
            Utc::now().timestamp_millis(),
            None,
        );
        crate::infrastructure::persistence::DeadLetterStore::add(&db, &rejected_record)
            .await
            .expect("dead-letter insert should succeed");
        assert!(service
            .mutation_was_rejected(&mutation_id_value)
            .await
            .expect("rejected lookup should succeed"));
        assert!(
            !service
                .mutation_is_pending(&Uuid::new_v4().to_string())
                .await
                .expect("random mutation id lookup should succeed"),
            "unseen mutation ids should not be reported as pending"
        );
    }

    #[tokio::test]
    async fn mutation_lifecycle_helpers_reject_invalid_mutation_ids() {
        let db = crate::infrastructure::persistence::init_test_db()
            .await
            .expect("in-memory db should initialize");
        let db = Rc::new(db);
        let mutation_store = MutationStore::from_rc(db.clone());
        let service = ExerciseService::new(db, mutation_store);

        let pending_err = service
            .mutation_is_pending("not-a-mutation-id")
            .await
            .expect_err("invalid mutation id should fail pending lookup");
        assert!(
            pending_err.starts_with("invalid mutation_id 'not-a-mutation-id':"),
            "pending error should include the rejected mutation id, got: {}",
            pending_err
        );

        let rejected_err = service
            .mutation_was_rejected("not-a-mutation-id")
            .await
            .expect_err("invalid mutation id should fail rejected lookup");
        assert!(
            rejected_err.starts_with("invalid mutation_id 'not-a-mutation-id':"),
            "rejected error should include the rejected mutation id, got: {}",
            rejected_err
        );
        assert_eq!(
            pending_err, rejected_err,
            "pending/rejected helpers should reject malformed ids identically"
        );
        assert_eq!(
            crate::infrastructure::persistence::OutboxStore::count(service.db.as_ref())
                .await
                .expect("outbox count should succeed"),
            0,
            "invalid mutation id lookups should not mutate outbox state"
        );
        assert_eq!(
            crate::infrastructure::persistence::DeadLetterStore::count(service.db.as_ref())
                .await
                .expect("dead-letter count should succeed"),
            0,
            "invalid mutation id lookups should not mutate dead-letter state"
        );
    }

    #[tokio::test]
    async fn dispatch_create_from_draft_enqueues_create_payload() {
        let (service, mutation_store) = test_service().await;

        let draft = ExerciseDraft {
            name: Some("  Bench Press  ".to_string()),
            muscle_groups: vec!["quadriceps".to_string(), "core".to_string()],
            equipment: Some("dumbbell".to_string()),
            description: Some("  chest-focused press  ".to_string()),
            instructions: Some("  keep elbows tucked  ".to_string()),
            video_url: Some("   ".to_string()),
            visibility: ExerciseDraftVisibility::Public,
            ..Default::default()
        };

        let (exercise_id, mutation_id) = service
            .dispatch_create_from_draft(&draft)
            .await
            .expect("create dispatch should succeed");

        assert!(
            service
                .mutation_is_pending(&mutation_id.as_str())
                .await
                .expect("pending lookup should succeed"),
            "queued create mutation should be pending"
        );

        let pending = mutation_store
            .get_pending()
            .await
            .expect("outbox lookup should succeed");
        assert_eq!(pending.len(), 1, "exactly one mutation should be queued");

        let intent = pending[0]
            .to_intent()
            .expect("pending record should decode");
        assert_eq!(intent.method, "POST");
        assert_eq!(intent.path, "/api/v1/exercises");
        assert_eq!(intent.body["exercise_id"], serde_json::json!(exercise_id));
        assert_eq!(intent.body["name"], serde_json::json!("Bench Press"));
        assert_eq!(
            intent.body["muscle_groups"],
            serde_json::json!([CommandMuscleGroup::Quads, CommandMuscleGroup::Core])
        );
        assert_eq!(
            intent.body["equipment"],
            serde_json::json!(CommandEquipment::Dumbbell)
        );
        assert_eq!(
            intent.body["description"],
            serde_json::json!("chest-focused press")
        );
        assert_eq!(
            intent.body["instructions"],
            serde_json::json!("keep elbows tucked")
        );
        assert!(intent.body["video_url"].is_null());
        assert_eq!(
            intent.body["visibility"],
            serde_json::json!(shared::Visibility::Public)
        );
    }

    #[tokio::test]
    async fn dispatch_update_from_draft_enqueues_update_payload() {
        let (service, mutation_store) = test_service().await;

        let exercise_id = Uuid::new_v4();
        let draft = ExerciseDraft {
            exercise_id: Some(exercise_id),
            name: Some("  Updated Name  ".to_string()),
            muscle_groups: vec!["full-body".to_string()],
            equipment: Some("barbell".to_string()),
            description: Some("Updated description".to_string()),
            instructions: None,
            video_url: None,
            visibility: ExerciseDraftVisibility::Unlisted,
            ..Default::default()
        };

        service
            .dispatch_update_from_draft(&draft)
            .await
            .expect("update dispatch should succeed");

        let pending = mutation_store
            .get_pending()
            .await
            .expect("outbox lookup should succeed");
        assert_eq!(pending.len(), 1, "exactly one mutation should be queued");

        let intent = pending[0]
            .to_intent()
            .expect("pending record should decode");
        assert_eq!(intent.method, "PATCH");
        assert_eq!(intent.path, format!("/api/v1/exercises/{exercise_id}"));
        assert_eq!(intent.body["name"], serde_json::json!("Updated Name"));
        assert_eq!(
            intent.body["muscle_groups"],
            serde_json::json!([CommandMuscleGroup::FullBody])
        );
        assert_eq!(
            intent.body["equipment"],
            serde_json::json!(CommandEquipment::Barbell)
        );
        assert_eq!(
            intent.body["description"],
            serde_json::json!("Updated description")
        );
        assert!(intent.body["instructions"].is_null());
        assert!(intent.body["video_url"].is_null());
        assert_eq!(
            intent.body["visibility"],
            serde_json::json!(shared::Visibility::Unlisted)
        );
    }

    #[tokio::test]
    async fn dispatch_create_from_draft_validates_required_fields() {
        let (service, _) = test_service().await;

        let missing_name = ExerciseDraft {
            muscle_groups: vec!["core".to_string()],
            ..Default::default()
        };
        let missing_groups = ExerciseDraft {
            name: Some("Bench".to_string()),
            ..Default::default()
        };

        let name_err = service
            .dispatch_create_from_draft(&missing_name)
            .await
            .expect_err("missing name should fail");
        assert_eq!(name_err, "exercise_draft_missing_name");

        let groups_err = service
            .dispatch_create_from_draft(&missing_groups)
            .await
            .expect_err("missing muscle groups should fail");
        assert_eq!(groups_err, "exercise_draft_missing_muscle_groups");
    }

    #[tokio::test]
    async fn dispatch_delete_exercise_enqueues_delete_route() {
        let (service, mutation_store) = test_service().await;
        let exercise_id = Uuid::new_v4();

        let mutation_id = service
            .dispatch_delete_exercise(exercise_id)
            .await
            .expect("delete dispatch should succeed");

        assert!(
            service
                .mutation_is_pending(&mutation_id.as_str())
                .await
                .expect("pending lookup should succeed"),
            "queued delete mutation should be pending"
        );

        let pending = mutation_store
            .get_pending()
            .await
            .expect("pending outbox lookup should succeed");
        assert_eq!(pending.len(), 1, "exactly one mutation should be queued");
        let intent = pending[0]
            .to_intent()
            .expect("pending record should decode");
        assert_eq!(intent.method, "DELETE");
        assert_eq!(intent.path, format!("/api/v1/exercises/{exercise_id}"));
        assert_eq!(intent.body, serde_json::json!({}));
    }

    #[tokio::test]
    async fn toggle_favorite_optimistic_updates_cache_and_queues_correct_route() {
        let (service, mutation_store) = test_service().await;
        let user_id = Uuid::new_v4();
        let exercise_id = Uuid::new_v4();

        ExerciseStore::put(
            service.db.as_ref(),
            &sample_exercise(exercise_id, Some(user_id), false, "Bench Press", None),
        )
        .await
        .expect("exercise should seed");

        let mutation_id = service
            .toggle_favorite_optimistic(exercise_id)
            .await
            .expect("favorite toggle should enqueue");

        assert!(
            service
                .mutation_is_pending(&mutation_id.as_str())
                .await
                .expect("pending lookup should succeed"),
            "favorite toggle should be pending"
        );

        let cached = ExerciseStore::get(service.db.as_ref(), &ExerciseId::from_uuid(exercise_id))
            .await
            .expect("exercise lookup should succeed")
            .expect("exercise should remain cached");
        assert!(cached.is_favorite, "optimistic cache should be updated");

        let pending = mutation_store
            .get_pending()
            .await
            .expect("pending outbox lookup should succeed");
        let intent = pending[0]
            .to_intent()
            .expect("pending record should decode");
        assert_eq!(intent.method, "PUT");
        assert_eq!(
            intent.path,
            format!("/api/v1/me/favorite-exercises/{exercise_id}")
        );
        let second_mutation_id = service
            .toggle_favorite_optimistic(exercise_id)
            .await
            .expect("second favorite toggle should enqueue");
        assert!(service
            .mutation_is_pending(&second_mutation_id.as_str())
            .await
            .expect("pending lookup should succeed for second toggle"));
        let cached = ExerciseStore::get(service.db.as_ref(), &ExerciseId::from_uuid(exercise_id))
            .await
            .expect("exercise lookup should succeed after second toggle")
            .expect("exercise should remain cached after second toggle");
        assert!(!cached.is_favorite);
        let pending = mutation_store
            .get_pending()
            .await
            .expect("pending outbox lookup should succeed after second toggle");
        let second_intent = pending[1]
            .to_intent()
            .expect("second pending record should decode");
        assert_eq!(second_intent.method, "DELETE");
        assert_eq!(
            second_intent.path,
            format!("/api/v1/me/favorite-exercises/{exercise_id}")
        );
    }

    #[tokio::test]
    async fn read_library_from_cache_respects_mode_and_search() {
        let (service, _) = test_service().await;
        let user_id = Uuid::new_v4();
        let other_user = Uuid::new_v4();
        let mine_id = Uuid::new_v4();
        let favorite_id = Uuid::new_v4();
        let private_id = Uuid::new_v4();

        ExerciseStore::put(
            service.db.as_ref(),
            &sample_exercise(
                mine_id,
                Some(user_id),
                false,
                "Bench Press",
                Some("Chest strength"),
            ),
        )
        .await
        .expect("owned exercise should seed");
        ExerciseStore::put(
            service.db.as_ref(),
            &sample_exercise(favorite_id, Some(other_user), true, "Pull Up", Some("Back")),
        )
        .await
        .expect("favorite exercise should seed");
        let mut private_owned = sample_exercise(
            private_id,
            Some(user_id),
            false,
            "Private Deadlift",
            Some("Hidden from discover"),
        );
        private_owned.visibility = shared::Visibility::Private;
        ExerciseStore::put(service.db.as_ref(), &private_owned)
            .await
            .expect("private exercise should seed");

        let mine_query = shared::ExerciseSearchQuery {
            mode: Some("mine".to_string()),
            ..Default::default()
        };
        let mine = service
            .read_library_from_cache(user_id, &mine_query)
            .await
            .expect("mine library should load");
        assert_eq!(mine.len(), 2);
        assert_eq!(mine[0].name, "Bench Press");
        assert_eq!(mine[0].id, mine_id);
        assert!(mine[0].is_owner);
        assert!(!mine[0].is_favorite);
        assert_eq!(mine[1].name, "Private Deadlift");
        assert_eq!(mine[1].id, private_id);
        assert!(matches!(mine[1].visibility, ExerciseVisibility::Private));

        let favorites_query = shared::ExerciseSearchQuery {
            mode: Some("favorites".to_string()),
            search: Some("pull".to_string()),
            ..Default::default()
        };
        let favorites = service
            .read_library_from_cache(user_id, &favorites_query)
            .await
            .expect("favorites library should load");
        assert_eq!(favorites.len(), 1);
        assert_eq!(favorites[0].name, "Pull Up");
        assert_eq!(favorites[0].id, favorite_id);
        assert!(!favorites[0].is_owner);
        assert!(favorites[0].is_favorite);
        let description_query = shared::ExerciseSearchQuery {
            mode: Some("mine".to_string()),
            search: Some("strength".to_string()),
            ..Default::default()
        };
        let description_match = service
            .read_library_from_cache(user_id, &description_query)
            .await
            .expect("description search should load");
        assert_eq!(description_match.len(), 1);
        assert_eq!(description_match[0].name, "Bench Press");

        let discover_query = shared::ExerciseSearchQuery {
            mode: Some("discover".to_string()),
            search: Some("  ".to_string()),
            ..Default::default()
        };
        let discover = service
            .read_library_from_cache(user_id, &discover_query)
            .await
            .expect("discover library should load");
        assert_eq!(
            discover
                .iter()
                .filter(|exercise| matches!(exercise.visibility, ExerciseVisibility::Private))
                .count(),
            0,
            "discover cache must not surface private items"
        );
        assert!(
            discover.iter().all(|exercise| exercise.id != private_id),
            "private cached exercises must never leak into discover fallback"
        );
    }

    #[tokio::test]
    async fn read_library_from_cache_applies_discovery_filters() {
        let (service, _) = test_service().await;
        let user_id = Uuid::new_v4();
        let owned_private_id = Uuid::new_v4();
        let public_favorite_id = Uuid::new_v4();

        let mut owned_private = sample_exercise(
            owned_private_id,
            Some(user_id),
            false,
            "Private Row",
            Some("Back drill"),
        );
        owned_private.visibility = shared::Visibility::Private;
        owned_private.equipment = shared::Equipment::Bodyweight;
        owned_private.muscle_groups = vec![shared::MuscleGroup::Back];

        let mut public_favorite = sample_exercise(
            public_favorite_id,
            None,
            true,
            "Public Press",
            Some("Chest drill"),
        );
        public_favorite.visibility = shared::Visibility::Public;
        public_favorite.equipment = shared::Equipment::Barbell;
        public_favorite.muscle_groups = vec![shared::MuscleGroup::Chest];

        ExerciseStore::put(service.db.as_ref(), &owned_private)
            .await
            .expect("owned private exercise should seed");
        ExerciseStore::put(service.db.as_ref(), &public_favorite)
            .await
            .expect("public favorite exercise should seed");

        let filtered_query = shared::ExerciseSearchQuery {
            mode: Some("mine".to_string()),
            muscle_group: vec!["Back".to_string()],
            equipment: vec!["bodyweight".to_string()],
            owned_only: true,
            visibility: vec!["private".to_string()],
            ..Default::default()
        };
        let filtered = service
            .read_library_from_cache(user_id, &filtered_query)
            .await
            .expect("filtered cache read should load");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, owned_private_id);
        assert!(matches!(
            filtered[0].visibility,
            ExerciseVisibility::Private
        ));

        let favorites_query = shared::ExerciseSearchQuery {
            mode: Some("favorites".to_string()),
            ..Default::default()
        };
        let favorites = service
            .read_library_from_cache(user_id, &favorites_query)
            .await
            .expect("favorite-filtered cache read should load");
        assert_eq!(favorites.len(), 1);
        assert_eq!(favorites[0].id, public_favorite_id);
        assert!(favorites[0].is_favorite);

        let discover_private_query = shared::ExerciseSearchQuery {
            mode: Some("discover".to_string()),
            visibility: vec!["private".to_string()],
            ..Default::default()
        };
        let discover_private = service
            .read_library_from_cache(user_id, &discover_private_query)
            .await
            .expect("discover cache read should load");
        assert!(
            discover_private.is_empty(),
            "discover cache should not expose private visibility even when requested explicitly"
        );
    }

    #[tokio::test]
    async fn load_library_uses_cached_fallback_for_safe_english_queries_when_refresh_fails() {
        let (service, _) = test_service().await;
        let user_id = Uuid::new_v4();
        let exercise_id = Uuid::new_v4();

        ExerciseStore::put(
            service.db.as_ref(),
            &sample_exercise(
                exercise_id,
                Some(user_id),
                false,
                "Bench Press",
                Some("cached english result"),
            ),
        )
        .await
        .expect("exercise should seed");

        let query = shared::ExerciseSearchQuery {
            mode: Some("mine".to_string()),
            search: Some("bench".to_string()),
            language: Some("en".to_string()),
            ..Default::default()
        };

        let snapshot = service
            .load_library_with_searcher(
                "Bearer test-token",
                user_id,
                &query,
                |_auth, _query| async {
                    Err::<shared::ExerciseSearchResponse, ServerFnError>(
                        ServerFnError::ServerError {
                            message: "search offline".to_string(),
                            code: 503,
                            details: None,
                        },
                    )
                },
            )
            .await
            .expect("safe english fallback should use cache");

        assert_eq!(snapshot.items.len(), 1);
        assert_eq!(snapshot.items[0].id, exercise_id);
        assert_eq!(snapshot.items[0].name, "Bench Press");
        assert_eq!(snapshot.applied_language.as_deref(), Some("en"));
        assert!(snapshot.summary.is_none());
        assert!(snapshot.next_cursor.is_none());
    }

    #[tokio::test]
    async fn load_library_rejects_unsafe_discover_cache_fallback_when_refresh_fails() {
        let (service, _) = test_service().await;
        let user_id = Uuid::new_v4();

        ExerciseStore::put(
            service.db.as_ref(),
            &sample_exercise(
                Uuid::new_v4(),
                Some(user_id),
                false,
                "Pull Up",
                Some("cached discover result"),
            ),
        )
        .await
        .expect("exercise should seed");

        let query = shared::ExerciseSearchQuery {
            mode: Some("discover".to_string()),
            search: Some("pull".to_string()),
            language: Some("en".to_string()),
            ..Default::default()
        };

        let err = service
            .load_library_with_searcher(
                "Bearer test-token",
                user_id,
                &query,
                |_auth, _query| async {
                    Err::<shared::ExerciseSearchResponse, ServerFnError>(
                        ServerFnError::ServerError {
                            message: "search offline".to_string(),
                            code: 503,
                            details: None,
                        },
                    )
                },
            )
            .await
            .expect_err("discover cache fallback should stay disabled even in english");

        assert!(
            err.contains("search offline"),
            "discover refresh failure should propagate when local cache cannot honor server semantics: {err}"
        );
    }

    #[tokio::test]
    async fn load_library_rejects_non_english_cache_fallback_when_refresh_fails() {
        let (service, _) = test_service().await;
        let user_id = Uuid::new_v4();

        ExerciseStore::put(
            service.db.as_ref(),
            &sample_exercise(
                Uuid::new_v4(),
                Some(user_id),
                false,
                "Pull Up",
                Some("canonical cached result"),
            ),
        )
        .await
        .expect("exercise should seed");

        let query = shared::ExerciseSearchQuery {
            mode: Some("discover".to_string()),
            search: Some("traction".to_string()),
            language: Some("fr".to_string()),
            ..Default::default()
        };

        let err = service
            .load_library_with_searcher(
                "Bearer test-token",
                user_id,
                &query,
                |_auth, _query| async {
                    Err::<shared::ExerciseSearchResponse, ServerFnError>(
                        ServerFnError::ServerError {
                            message: "search offline".to_string(),
                            code: 503,
                            details: None,
                        },
                    )
                },
            )
            .await
            .expect_err("non-english discovery should not silently fall back to canonical cache");

        assert!(
            err.contains("search offline"),
            "refresh failure should propagate when multilingual fallback is unsafe: {err}"
        );
    }

    #[tokio::test]
    async fn load_library_without_auth_only_uses_cache_for_safe_modes() {
        let (service, _) = test_service().await;
        let user_id = Uuid::new_v4();
        let mine_id = Uuid::new_v4();

        ExerciseStore::put(
            service.db.as_ref(),
            &sample_exercise(
                mine_id,
                Some(user_id),
                false,
                "Bench Press",
                Some("cached mine result"),
            ),
        )
        .await
        .expect("exercise should seed");

        let mine_query = shared::ExerciseSearchQuery {
            mode: Some("mine".to_string()),
            language: Some("en".to_string()),
            ..Default::default()
        };
        let mine_snapshot = service
            .load_library_with_searcher("", user_id, &mine_query, |_auth, _query| async {
                unreachable!("searcher should not run without auth");
            })
            .await
            .expect("mine cache fallback should load without auth");
        assert_eq!(mine_snapshot.items.len(), 1);
        assert_eq!(mine_snapshot.items[0].id, mine_id);
        assert_eq!(mine_snapshot.applied_language.as_deref(), Some("en"));

        let discover_query = shared::ExerciseSearchQuery {
            mode: Some("discover".to_string()),
            language: Some("en".to_string()),
            ..Default::default()
        };
        let discover_snapshot = service
            .load_library_with_searcher("", user_id, &discover_query, |_auth, _query| async {
                unreachable!("searcher should not run without auth");
            })
            .await
            .expect("discover without auth should resolve safely");
        assert!(
            discover_snapshot.items.is_empty(),
            "discover should not silently project cached results when auth is missing"
        );
        assert_eq!(discover_snapshot.applied_language.as_deref(), Some("en"));
    }

    #[tokio::test]
    async fn load_detail_uses_cache_when_auth_header_is_missing() {
        let (service, _) = test_service().await;
        let user_id = Uuid::new_v4();
        let exercise_id = Uuid::new_v4();

        ExerciseStore::put(
            service.db.as_ref(),
            &sample_exercise(
                exercise_id,
                Some(user_id),
                true,
                "Incline Bench",
                Some("Upper chest"),
            ),
        )
        .await
        .expect("exercise should seed");

        let detail = service
            .load_detail("", user_id, exercise_id)
            .await
            .expect("cached detail should be returned");
        assert_eq!(detail.id, exercise_id);
        assert_eq!(detail.name, "Incline Bench");
        assert_eq!(detail.description.as_deref(), Some("Upper chest"));
        assert!(detail.is_favorite);
        assert!(detail.is_owner);
        assert_eq!(detail.canonical_language, "en");

        let missing_id = Uuid::new_v4();
        let missing_err = service
            .load_detail("", user_id, missing_id)
            .await
            .expect_err("missing cache and auth should fail");
        assert!(
            missing_err.contains("without authentication or cache"),
            "unexpected error: {missing_err}"
        );
        assert!(
            missing_err.contains(&missing_id.to_string()),
            "missing cache error should mention the requested exercise id"
        );
    }

    #[tokio::test]
    async fn load_detail_prefers_network_when_auth_header_present_and_refreshes_cache() {
        let (service, _) = test_service().await;
        let user_id = Uuid::new_v4();
        let exercise_id = Uuid::new_v4();

        ExerciseStore::put(
            service.db.as_ref(),
            &sample_exercise(
                exercise_id,
                Some(user_id),
                false,
                "Cached Bench",
                Some("stale description"),
            ),
        )
        .await
        .expect("exercise should seed");

        let remote = sample_exercise(
            exercise_id,
            Some(user_id),
            true,
            "Remote Bench",
            Some("fresh description"),
        );
        let detail = service
            .load_detail_with_fetcher("Bearer test-token", user_id, exercise_id, {
                let remote = remote.clone();
                move |auth_header, requested_id| {
                    let remote = remote.clone();
                    async move {
                        assert_eq!(auth_header, "Bearer test-token");
                        assert_eq!(requested_id, exercise_id.to_string());
                        Ok::<shared::Exercise, ServerFnError>(remote)
                    }
                }
            })
            .await
            .expect("remote detail should be preferred when auth is present");

        assert_eq!(detail.name, "Remote Bench");
        assert_eq!(detail.description.as_deref(), Some("fresh description"));
        assert!(
            detail.is_favorite,
            "remote payload should win over stale cache"
        );

        let refreshed =
            ExerciseStore::get(service.db.as_ref(), &ExerciseId::from_uuid(exercise_id))
                .await
                .expect("exercise cache lookup should succeed")
                .expect("exercise should remain cached");
        assert_eq!(refreshed.name, "Remote Bench");
        assert_eq!(refreshed.description.as_deref(), Some("fresh description"));
        assert!(refreshed.is_favorite);
    }

    #[tokio::test]
    async fn refresh_library_maps_localized_search_response_and_summary() {
        let (service, _) = test_service().await;
        let user_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let exercise_id = Uuid::new_v4();
        let query = shared::ExerciseSearchQuery {
            mode: Some("discover".to_string()),
            search: Some("traction".to_string()),
            language: Some("fr".to_string()),
            muscle_group: vec!["back".to_string()],
            equipment: vec!["bodyweight".to_string()],
            favorites_only: true,
            limit: Some(25),
            ..Default::default()
        };

        let snapshot = service
            .refresh_library_with_searcher(
                "Bearer test-token",
                user_id,
                &query,
                |auth_header, query| async move {
                    assert_eq!(auth_header, "Bearer test-token");
                    assert_eq!(query.mode.as_deref(), Some("discover"));
                    assert_eq!(query.search.as_deref(), Some("traction"));
                    assert_eq!(query.language.as_deref(), Some("fr"));
                    assert_eq!(query.muscle_group, vec!["back".to_string()]);
                    assert_eq!(query.equipment, vec!["bodyweight".to_string()]);
                    assert!(query.favorites_only);
                    assert_eq!(query.cursor, None);
                    assert_eq!(query.limit, Some(25));

                    Ok::<shared::ExerciseSearchResponse, ServerFnError>(
                        shared::ExerciseSearchResponse {
                            applied_language: "fr".to_string(),
                            items: vec![shared::ExerciseReadItem {
                                id: exercise_id,
                                owner_id: Some(owner_id),
                                name: "Traction".to_string(),
                                muscle_groups: vec!["back".to_string()],
                                equipment: Some("bodyweight".to_string()),
                                description: Some("Mouvement de tirage".to_string()),
                                instructions: Some("Tirez jusqu'au menton".to_string()),
                                video_url: None,
                                canonical_language: "en".to_string(),
                                available_languages: vec!["en".to_string(), "fr".to_string()],
                                visibility: "public".to_string(),
                                moderation_status: "approved_by_human".to_string(),
                                is_favorite: true,
                                created_at: Utc::now(),
                                updated_at: Utc::now(),
                            }],
                            next_cursor: Some("cursor page three".to_string()),
                            summary: shared::ExerciseSearchSummary {
                                total_results: 1,
                                mode_counts: shared::ExerciseSearchModeCounts {
                                    discover: 1,
                                    mine: 0,
                                    favorites: 1,
                                },
                                translated_results: Some(1),
                                muscle_group_counts: vec![shared::ExerciseSearchFacetCount {
                                    id: "back".to_string(),
                                    label: "Back".to_string(),
                                    count: 1,
                                }],
                                equipment_counts: vec![shared::ExerciseSearchFacetCount {
                                    id: "bodyweight".to_string(),
                                    label: "Bodyweight".to_string(),
                                    count: 1,
                                }],
                                language_counts: vec![shared::ExerciseSearchFacetCount {
                                    id: "fr".to_string(),
                                    label: "FR".to_string(),
                                    count: 1,
                                }],
                            },
                        },
                    )
                },
            )
            .await
            .expect("search refresh should succeed");

        assert_eq!(snapshot.items.len(), 1);
        assert_eq!(snapshot.items[0].name, "Traction");
        assert_eq!(snapshot.items[0].canonical_language, "en");
        assert_eq!(
            snapshot.items[0].available_languages,
            vec!["en".to_string(), "fr".to_string()]
        );
        assert!(!snapshot.items[0].is_owner);
        assert!(snapshot.items[0].is_favorite);

        let summary = snapshot.summary.expect("summary should be present");
        assert_eq!(summary.total_results, 1);
        assert_eq!(summary.mode_counts.discover, 1);
        assert_eq!(summary.mode_counts.favorites, 1);
        assert_eq!(summary.translated_results, Some(1));
        assert_eq!(summary.language_counts[0].id, "fr");
        assert_eq!(summary.language_counts[0].count, 1);
        assert_eq!(snapshot.next_cursor.as_deref(), Some("cursor page three"));

        let cached = ExerciseStore::get_all(service.db.as_ref())
            .await
            .expect("exercise cache lookup should succeed");
        assert!(
            cached.is_empty(),
            "localized search results should not overwrite canonical exercise cache"
        );
    }

    #[tokio::test]
    async fn refresh_library_rebuilds_visible_window_through_requested_cursor() {
        let (service, _) = test_service().await;
        let user_id = Uuid::new_v4();
        let requested_cursor = "cursor-page-three".to_string();
        let seen_cursors = Rc::new(RefCell::new(Vec::<Option<String>>::new()));

        let query = shared::ExerciseSearchQuery {
            mode: Some("discover".to_string()),
            search: Some("press".to_string()),
            language: Some("en".to_string()),
            cursor: Some(requested_cursor.clone()),
            limit: Some(2),
            ..Default::default()
        };

        let snapshot = service
            .refresh_library_with_searcher("Bearer test-token", user_id, &query, {
                let seen_cursors = seen_cursors.clone();
                move |auth_header, query| {
                    seen_cursors.borrow_mut().push(query.cursor.clone());
                    let requested_cursor = requested_cursor.clone();
                    async move {
                        assert_eq!(auth_header, "Bearer test-token");
                        match query.cursor.as_deref() {
                            None => Ok::<shared::ExerciseSearchResponse, ServerFnError>(
                                shared::ExerciseSearchResponse {
                                    applied_language: "en".to_string(),
                                    items: vec![shared::ExerciseReadItem {
                                        id: Uuid::parse_str("00000000-0000-0000-0000-000000000101")
                                            .unwrap(),
                                        owner_id: Some(user_id),
                                        name: "Bench Press".to_string(),
                                        muscle_groups: vec!["chest".to_string()],
                                        equipment: Some("barbell".to_string()),
                                        description: None,
                                        instructions: None,
                                        video_url: None,
                                        canonical_language: "en".to_string(),
                                        available_languages: vec!["en".to_string()],
                                        visibility: "public".to_string(),
                                        moderation_status: "approved_by_human".to_string(),
                                        is_favorite: false,
                                        created_at: Utc::now(),
                                        updated_at: Utc::now(),
                                    }],
                                    next_cursor: Some("cursor-page-two".to_string()),
                                    summary: shared::ExerciseSearchSummary {
                                        total_results: 3,
                                        mode_counts: shared::ExerciseSearchModeCounts {
                                            discover: 3,
                                            mine: 1,
                                            favorites: 0,
                                        },
                                        translated_results: Some(0),
                                        muscle_group_counts: Vec::new(),
                                        equipment_counts: Vec::new(),
                                        language_counts: Vec::new(),
                                    },
                                },
                            ),
                            Some("cursor-page-two") => Ok::<
                                shared::ExerciseSearchResponse,
                                ServerFnError,
                            >(
                                shared::ExerciseSearchResponse {
                                    applied_language: "en".to_string(),
                                    items: vec![shared::ExerciseReadItem {
                                        id: Uuid::parse_str("00000000-0000-0000-0000-000000000102")
                                            .unwrap(),
                                        owner_id: Some(user_id),
                                        name: "Incline Press".to_string(),
                                        muscle_groups: vec!["chest".to_string()],
                                        equipment: Some("dumbbell".to_string()),
                                        description: None,
                                        instructions: None,
                                        video_url: None,
                                        canonical_language: "en".to_string(),
                                        available_languages: vec!["en".to_string()],
                                        visibility: "public".to_string(),
                                        moderation_status: "approved_by_human".to_string(),
                                        is_favorite: false,
                                        created_at: Utc::now(),
                                        updated_at: Utc::now(),
                                    }],
                                    next_cursor: Some(requested_cursor.clone()),
                                    summary: shared::ExerciseSearchSummary {
                                        total_results: 3,
                                        mode_counts: shared::ExerciseSearchModeCounts {
                                            discover: 3,
                                            mine: 1,
                                            favorites: 0,
                                        },
                                        translated_results: Some(0),
                                        muscle_group_counts: Vec::new(),
                                        equipment_counts: Vec::new(),
                                        language_counts: Vec::new(),
                                    },
                                },
                            ),
                            Some("cursor-page-three") => Ok::<
                                shared::ExerciseSearchResponse,
                                ServerFnError,
                            >(
                                shared::ExerciseSearchResponse {
                                    applied_language: "en".to_string(),
                                    items: vec![shared::ExerciseReadItem {
                                        id: Uuid::parse_str("00000000-0000-0000-0000-000000000103")
                                            .unwrap(),
                                        owner_id: Some(user_id),
                                        name: "Push Up".to_string(),
                                        muscle_groups: vec!["chest".to_string()],
                                        equipment: Some("bodyweight".to_string()),
                                        description: None,
                                        instructions: None,
                                        video_url: None,
                                        canonical_language: "en".to_string(),
                                        available_languages: vec!["en".to_string()],
                                        visibility: "public".to_string(),
                                        moderation_status: "approved_by_human".to_string(),
                                        is_favorite: true,
                                        created_at: Utc::now(),
                                        updated_at: Utc::now(),
                                    }],
                                    next_cursor: Some("cursor-page-four".to_string()),
                                    summary: shared::ExerciseSearchSummary {
                                        total_results: 3,
                                        mode_counts: shared::ExerciseSearchModeCounts {
                                            discover: 3,
                                            mine: 1,
                                            favorites: 0,
                                        },
                                        translated_results: Some(0),
                                        muscle_group_counts: Vec::new(),
                                        equipment_counts: Vec::new(),
                                        language_counts: Vec::new(),
                                    },
                                },
                            ),
                            other => panic!("unexpected cursor request: {:?}", other),
                        }
                    }
                }
            })
            .await
            .expect("cursored refresh should succeed");

        assert_eq!(
            seen_cursors.take(),
            vec![
                None,
                Some("cursor-page-two".to_string()),
                Some("cursor-page-three".to_string())
            ],
            "service should replay prior pages and then fetch the requested page"
        );
        assert_eq!(
            snapshot
                .items
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Bench Press", "Incline Press", "Push Up"],
            "visible window should include every page through the requested cursor"
        );
        assert_eq!(
            snapshot.next_cursor.as_deref(),
            Some("cursor-page-four"),
            "load-more cursor should remain the next page after the reconstructed window"
        );
        let summary = snapshot.summary.expect("summary should remain present");
        assert_eq!(summary.total_results, 3);
        assert_eq!(summary.mode_counts.discover, 3);
    }

    #[tokio::test]
    async fn dispatch_translation_operations_enqueue_expected_paths() {
        let (service, mutation_store) = test_service().await;
        let exercise_id = Uuid::new_v4();
        let proposal_id = Uuid::new_v4();

        service
            .dispatch_translation_proposal(
                exercise_id,
                PostTranslationProposalRequest {
                    proposal_id,
                    language: shared::ContractLanguage::Fr,
                    name: Some("Développé couché".to_string()),
                    description: None,
                    instructions: None,
                },
            )
            .await
            .expect("translation proposal should dispatch");
        service
            .dispatch_translation_moderation(
                proposal_id,
                PatchTranslationProposalRequest {
                    status: shared::TranslationProposalStatus::Rejected,
                    feedback: Some("needs revision".to_string()),
                },
            )
            .await
            .expect("translation moderation should dispatch");

        let pending = mutation_store
            .get_pending()
            .await
            .expect("pending outbox lookup should succeed");
        let intents = pending
            .into_iter()
            .map(|record| record.to_intent().expect("pending record should decode"))
            .collect::<Vec<_>>();

        assert!(intents.iter().any(|intent| {
            intent.method == "POST"
                && intent.path == format!("/api/v1/exercises/{exercise_id}/translation-proposals")
                && intent.body["proposal_id"] == serde_json::json!(proposal_id)
                && intent.body["language"] == serde_json::json!("fr")
                && intent.body["name"] == serde_json::json!("Développé couché")
                && intent.body["description"].is_null()
                && intent.body["instructions"].is_null()
        }));
        assert!(intents.iter().any(|intent| {
            intent.method == "PATCH"
                && intent.path == format!("/api/v1/translation-proposals/{proposal_id}")
                && intent.body["status"] == serde_json::json!("rejected")
                && intent.body["feedback"] == serde_json::json!("needs revision")
        }));
    }
}

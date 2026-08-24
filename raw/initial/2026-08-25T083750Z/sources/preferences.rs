use std::cell::Cell;
use std::fmt;
use std::rc::Rc;

use dioxus::prelude::{spawn, ReadableExt, ServerFnError, Signal, WritableExt};
use shared::frontend::dto::MutationId;
use shared::{MutationDispatchStatus, PutUserPreferencesRequest};
use shared_ui::{Language, ThemeId};
use uuid::Uuid;
use web_time::Instant;

use crate::application::orchestration::preference_effects::apply_user_preferences_state;
use crate::domain::preferences::UserPreferences;
use crate::infrastructure::mappers::instrumentation::{
    record_preference_cache_write, record_preference_dispatch, record_preference_field_changes,
    set_preferences_dom_markers,
};
use crate::infrastructure::mappers::preferences::{
    contract_to_language, contract_to_theme_id, contract_to_weight_unit, language_to_contract,
    parse_contract_language, parse_theme, parse_units, serialize_language, serialize_theme,
    serialize_units, theme_id_to_contract, weight_unit_to_contract,
};
use crate::infrastructure::persistence::{
    CachedPreferences, Database, EntityCache, EntityType, MutationStore, PreferenceCache,
    VersionUpdateResult,
};
use crate::infrastructure::probe;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapPreferencesOutcome {
    Complete,
    Unauthorized,
}

#[derive(Debug, Clone)]
pub enum ApplyServerPreferencesError {
    CacheUpsert(String),
    CacheMarkSynced(String),
}

impl fmt::Display for ApplyServerPreferencesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CacheUpsert(err) => write!(f, "cache upsert failed: {err}"),
            Self::CacheMarkSynced(err) => write!(f, "cache mark_synced failed: {err}"),
        }
    }
}

impl std::error::Error for ApplyServerPreferencesError {}

/// Reactive signals and state dependencies for [`PreferenceService`].
#[derive(Clone)]
pub struct PreferenceSignals {
    pub preferences: Signal<UserPreferences>,
    pub theme_signal: Option<Signal<ThemeId>>,
    pub lang_signal: Option<Signal<Language>>,
    pub cache_versions: Signal<EntityCache>,
    pub preference_notice: Signal<Option<String>>,
    pub bootstrap_epoch: Rc<Cell<u64>>,
}

#[derive(Clone)]
pub struct PreferenceService {
    db: Rc<Database>,
    user_id: Uuid,
    mutation_store: MutationStore,
    preferences: Signal<UserPreferences>,
    theme_signal: Option<Signal<ThemeId>>,
    lang_signal: Option<Signal<Language>>,
    cache_versions: Signal<EntityCache>,
    preference_notice: Signal<Option<String>>,
    bootstrap_epoch: Rc<Cell<u64>>,
}

impl PreferenceService {
    pub fn new(
        db: Rc<Database>,
        user_id: Uuid,
        mutation_store: MutationStore,
        signals: PreferenceSignals,
    ) -> Self {
        Self {
            db,
            user_id,
            mutation_store,
            preferences: signals.preferences,
            theme_signal: signals.theme_signal,
            lang_signal: signals.lang_signal,
            cache_versions: signals.cache_versions,
            preference_notice: signals.preference_notice,
            bootstrap_epoch: signals.bootstrap_epoch,
        }
    }

    pub fn preferences_signal(&self) -> Signal<UserPreferences> {
        self.preferences
    }

    pub async fn bootstrap_cache_then_server(
        &self,
        access_token: String,
    ) -> BootstrapPreferencesOutcome {
        let load_epoch = self.bootstrap_epoch.get().wrapping_add(1);
        self.bootstrap_epoch.set(load_epoch);
        let is_stale = || self.bootstrap_epoch.get() != load_epoch;

        let mut cached_for_compare: Option<(String, String, String, u32)> = None;
        let mut cached_is_synced = false;
        let mut has_valid_cached_preferences = false;

        match PreferenceCache::get(self.db.as_ref(), self.user_id).await {
            Ok(Some(cached)) => {
                crate::log!("[Preferences:cache_hit] user_id={}", self.user_id);
                match Self::parse_cached_preferences(&cached) {
                    Ok(ui_prefs) => {
                        if is_stale() {
                            return BootstrapPreferencesOutcome::Complete;
                        }
                        has_valid_cached_preferences = true;
                        crate::log!(
                            "[Preferences:cache_valid] user_id={} synced={}",
                            self.user_id,
                            cached.synced_at.is_some()
                        );
                        cached_for_compare = Some(Self::cache_tuple(&cached));
                        cached_is_synced = cached.synced_at.is_some();
                        let mut prefs = self.preferences;
                        apply_user_preferences_state(
                            &mut prefs,
                            self.theme_signal,
                            self.lang_signal,
                            ui_prefs.clone(),
                            "cache-apply",
                        );
                        probe::record_bootstrap_preferences(
                            &ui_prefs,
                            true,
                            cached.synced_at.is_some(),
                        );
                        set_preferences_dom_markers(Some("cache"), None);
                    }
                    Err(errors) => {
                        for error in errors {
                            crate::log!(
                                "[Preferences:cache_invalid] user_id={} {}",
                                self.user_id,
                                error
                            );
                        }
                        crate::log!(
                            "[Preferences:cache_invalid] user_id={} reason=malformed waiting_for_server_refresh=true",
                            self.user_id
                        );
                    }
                }
            }
            Ok(None) => {
                crate::log!("[Preferences:cache_miss] user_id={}", self.user_id);
                probe::set_preferences_present(false);
            }
            Err(err) => {
                crate::log!("[Preferences:cache_read_error] {}", err);
            }
        }

        let local_version = self
            .cache_versions
            .read()
            .get_version(EntityType::UserPreferences);
        let preferences_marked_stale = self
            .cache_versions
            .read()
            .is_stale(EntityType::UserPreferences);
        let mut server_version: Option<u64> = None;
        {
            let mut client_versions = std::collections::HashMap::new();
            client_versions.insert(
                EntityType::UserPreferences.as_str().to_string(),
                local_version,
            );
            match crate::api::preferences::check_cache_versions(
                format!("Bearer {}", access_token.clone()),
                client_versions,
            )
            .await
            {
                Ok(server_versions) => {
                    if let Some(version) = server_versions.get(EntityType::UserPreferences.as_str())
                    {
                        server_version = Some(*version);
                        crate::log!(
                            "[Preferences:version_check] local_version={} server_version={} decision={}",
                            local_version,
                            version,
                            if *version == local_version {
                                "match"
                            } else {
                                "mismatch"
                            }
                        );
                    }
                }
                Err(err) => {
                    crate::log!("[Preferences:version_check_error] {}", err);
                }
            }
        }

        if server_version.is_none() {
            crate::log!(
                "[Preferences:version_check] local_version={} server_version=absent decision=fetch",
                local_version
            );
        }

        if !should_fetch_preferences_from_server(
            local_version,
            server_version,
            has_valid_cached_preferences,
            preferences_marked_stale,
        ) {
            crate::log!(
                "[Preferences:server_refresh_skip] reason=versions_equal cache_valid=true stale=false local_version={} server_version={}",
                local_version,
                server_version.unwrap_or_default()
            );
            return BootstrapPreferencesOutcome::Complete;
        }

        if !has_valid_cached_preferences {
            crate::log!(
                "[Preferences:server_refresh_fetch_reason] reason=cache_missing_or_invalid local_version={} server_version={}",
                local_version,
                server_version.unwrap_or_default()
            );
        }

        crate::log!(
            "[Preferences:server_refresh_fetch] local_version={} server_version={} has_cache={}",
            local_version,
            server_version.unwrap_or_default(),
            has_valid_cached_preferences
        );

        match crate::api::preferences::fetch_user_preferences(format!("Bearer {}", access_token))
            .await
        {
            Ok(Some(server_prefs)) => {
                if is_stale() {
                    return BootstrapPreferencesOutcome::Complete;
                }

                let incoming = (
                    serialize_units(server_prefs.units),
                    serialize_theme(server_prefs.theme),
                    serialize_language(server_prefs.language),
                    server_prefs.default_rest_seconds,
                );
                let sync_cache = should_write_cache_after_server_refresh(
                    cached_for_compare.as_ref(),
                    &incoming,
                    local_version,
                    server_version,
                    cached_is_synced,
                );

                match self
                    .apply_server_preferences(server_prefs, "server-apply", sync_cache)
                    .await
                {
                    Ok(()) => {
                        probe::set_last_applied_version(server_version);
                        if sync_cache {
                            record_preference_cache_write("server_refresh", "mark_synced", "ok");
                        }
                        let mut notice = self.preference_notice;
                        notice.set(None);
                    }
                    Err(ApplyServerPreferencesError::CacheUpsert(err)) => {
                        record_preference_cache_write("server_refresh", "upsert", "error");
                        crate::log!("[Preferences:cache_upsert_error] {}", err);
                        probe::set_sync_state(
                            probe::ProbeSyncState::Error,
                            "preferences.cache_upsert_error",
                        );
                        let mut notice = self.preference_notice;
                        notice.set(Some("Couldn't update local preferences cache.".to_string()));
                    }
                    Err(ApplyServerPreferencesError::CacheMarkSynced(err)) => {
                        record_preference_cache_write("server_refresh", "mark_synced", "error");
                        crate::log!("[Preferences:cache_mark_synced_error] {}", err);
                        probe::set_sync_state(
                            probe::ProbeSyncState::Error,
                            "preferences.cache_mark_synced_error",
                        );
                        let mut notice = self.preference_notice;
                        notice.set(Some(
                            "Preferences saved, but sync status couldn't be confirmed.".to_string(),
                        ));
                    }
                }

                let mut versions = self.cache_versions;
                mark_preferences_version_fresh(&mut versions, server_version);
            }
            Ok(None) => {
                crate::log!("[Preferences] No server preferences, using cached/defaults");
                probe::set_preferences_present(has_valid_cached_preferences);
                let mut versions = self.cache_versions;
                mark_preferences_version_fresh(&mut versions, server_version);
            }
            Err(err) => {
                if is_server_fn_unauthorized(&err) {
                    crate::log!("[Preferences] bootstrap fetch returned 401");
                    return BootstrapPreferencesOutcome::Unauthorized;
                }
                crate::log!("[Preferences:server_refresh_error] {}", err);
                probe::set_sync_state(
                    probe::ProbeSyncState::Error,
                    "preferences.server_refresh_error",
                );
            }
        }

        BootstrapPreferencesOutcome::Complete
    }

    pub fn apply_local_user_change(&self, new_prefs: UserPreferences) -> bool {
        let old_prefs = (self.preferences)();
        #[cfg(debug_assertions)]
        crate::log!(
            "[PrefsChange] old_unit={} new_unit={} old_theme={} new_theme={} old_lang={} new_lang={}",
            old_prefs.weight_unit.label(),
            new_prefs.weight_unit.label(),
            old_prefs.theme.slug(),
            new_prefs.theme.slug(),
            serialize_language(language_to_contract(old_prefs.language)),
            serialize_language(language_to_contract(new_prefs.language))
        );

        if new_prefs == old_prefs {
            return false;
        }

        record_preference_field_changes(&old_prefs, &new_prefs);

        let mut preferences = self.preferences;
        if new_prefs.theme != old_prefs.theme || new_prefs.language != old_prefs.language {
            apply_user_preferences_state(
                &mut preferences,
                self.theme_signal,
                self.lang_signal,
                new_prefs.clone(),
                "user-change",
            );
        } else {
            preferences.set(new_prefs.clone());
        }

        #[cfg(debug_assertions)]
        {
            let current = (self.preferences)();
            crate::log!(
                "[PrefsChange] after_set unit={} theme={} language={} rest={}",
                current.weight_unit.label(),
                current.theme.slug(),
                serialize_language(language_to_contract(current.language)),
                current.default_rest_seconds
            );
        }

        probe::record_local_preferences(&new_prefs);

        true
    }

    /// Canonical write path for user-driven preference updates.
    pub fn handle_local_user_change(&self, new_prefs: UserPreferences, auth_header: String) {
        self.bootstrap_epoch
            .set(self.bootstrap_epoch.get().wrapping_add(1));
        if !self.apply_local_user_change(new_prefs.clone()) {
            return;
        }

        let cache_service = self.clone();
        let cache_prefs = new_prefs.clone();
        spawn(async move {
            cache_service.persist_local_user_change(cache_prefs).await;
        });

        let dispatch_service = self.clone();
        spawn(async move {
            dispatch_service
                .dispatch_local_user_change(new_prefs, auth_header)
                .await;
        });
    }

    /// Deterministic write path for the E2E semantic action transport.
    ///
    /// Live propagation tests care about the server-observed mutation and SSE fan-out. The regular
    /// UI path still persists the local cache, but the probe path avoids letting native SQLite cache
    /// contention block the cross-platform dispatch.
    pub async fn handle_probe_user_change(&self, new_prefs: UserPreferences, auth_header: String) {
        self.bootstrap_epoch
            .set(self.bootstrap_epoch.get().wrapping_add(1));
        self.dispatch_local_user_change(new_prefs.clone(), auth_header)
            .await;
        self.apply_local_user_change(new_prefs);
    }

    pub async fn persist_local_user_change(&self, prefs: UserPreferences) {
        let units_str = serialize_units(weight_unit_to_contract(prefs.weight_unit));
        let theme_str = serialize_theme(theme_id_to_contract(prefs.theme));
        let lang_str = serialize_language(language_to_contract(prefs.language));
        probe::set_sync_state(
            probe::ProbeSyncState::Syncing,
            "preferences.cache_write_started",
        );
        set_preferences_dom_markers(None, Some("pending"));
        crate::log!(
            "[PreferenceCache::upsert:user_preference_change] user_id={}",
            self.user_id
        );

        match PreferenceCache::upsert(
            self.db.as_ref(),
            self.user_id,
            &units_str,
            &theme_str,
            &lang_str,
            prefs.default_rest_seconds,
        )
        .await
        {
            Ok(()) => {
                record_preference_cache_write("user_change", "upsert", "ok");
                let mut notice = self.preference_notice;
                notice.set(None);
            }
            Err(err) => {
                record_preference_cache_write("user_change", "upsert", "error");
                crate::log!("[Preferences:cache_upsert_error] {}", err);
                probe::set_sync_state(
                    probe::ProbeSyncState::Error,
                    "preferences.cache_upsert_error",
                );
                set_preferences_dom_markers(None, Some("error"));
                let mut notice = self.preference_notice;
                notice.set(Some(
                    "Couldn't save preferences to local cache.".to_string(),
                ));
            }
        }
    }

    pub async fn dispatch_local_user_change(&self, prefs: UserPreferences, auth_header: String) {
        let dispatch_started = Instant::now();
        let mutation_id = MutationId::new();
        let request = PutUserPreferencesRequest {
            units: weight_unit_to_contract(prefs.weight_unit),
            theme: theme_id_to_contract(prefs.theme),
            language: language_to_contract(prefs.language),
            default_rest_seconds: prefs.default_rest_seconds,
        };

        crate::log!(
            "[Preferences] Dispatching UpdatePreferences mutation {}",
            mutation_id
        );
        probe::set_sync_state(
            probe::ProbeSyncState::Syncing,
            "preferences.dispatch_started",
        );
        match crate::api::preferences::update_user_preferences(
            auth_header.clone(),
            request.clone(),
            Some(mutation_id.as_str()),
        )
        .await
        {
            Ok(response) => {
                let result_label = match response.status {
                    MutationDispatchStatus::Applied => "applied",
                    MutationDispatchStatus::Duplicate => "duplicate",
                    MutationDispatchStatus::Pending => "pending",
                };
                record_preference_dispatch(result_label, dispatch_started);
                crate::log!(
                    "[Preferences] CRUD mutation accepted with status={} job_id={}",
                    result_label,
                    response.job_id
                );
                probe::set_sync_state(
                    probe::ProbeSyncState::Syncing,
                    "preferences.dispatch_accepted",
                );
                let mut notice = self.preference_notice;
                notice.set(None);
                return;
            }
            Err(err) if is_terminal_preferences_write_error(&err) => {
                record_preference_dispatch("rejected", dispatch_started);
                crate::log!(
                    "[Preferences:dispatch_rejected] mutation_id={} error={}",
                    mutation_id,
                    err
                );
                probe::set_sync_state(
                    probe::ProbeSyncState::Error,
                    "preferences.dispatch_rejected",
                );
                let mut notice = self.preference_notice;
                notice.set(Some(
                    "The server rejected this preference change.".to_string(),
                ));
                return;
            }
            Err(err) => {
                crate::log!(
                    "[Preferences:dispatch_fallback] mutation_id={} error={}",
                    mutation_id,
                    err
                );
            }
        }

        crate::log!(
            "[Preferences] Falling back to local outbox for mutation {}",
            mutation_id
        );
        if let Err(err) = self
            .mutation_store
            .enqueue_put_user_preferences_with_id(mutation_id, request)
            .await
        {
            record_preference_dispatch("error", dispatch_started);
            crate::log!("[Preferences:dispatch_error] {}", err);
            probe::set_sync_state(probe::ProbeSyncState::Error, "preferences.dispatch_error");
            let mut notice = self.preference_notice;
            notice.set(Some(
                "Couldn't sync preference change to server yet.".to_string(),
            ));
            return;
        }

        record_preference_dispatch("queued", dispatch_started);
        crate::log!("[Preferences] Fallback mutation queued, attempting immediate sync");
        probe::set_sync_state(
            probe::ProbeSyncState::Syncing,
            "preferences.dispatch_queued",
        );
        let mut notice = self.preference_notice;
        notice.set(None);

        if let Err(err) = self.mutation_store.sync(&auth_header).await {
            crate::log!("[Preferences:sync_after_dispatch_error] {}", err);
        }
    }

    pub async fn apply_server_preferences(
        &self,
        server_prefs: shared::UserPreferencesResponse,
        source: &'static str,
        sync_cache: bool,
    ) -> Result<(), ApplyServerPreferencesError> {
        let next = UserPreferences {
            weight_unit: contract_to_weight_unit(server_prefs.units),
            theme: contract_to_theme_id(server_prefs.theme),
            language: contract_to_language(server_prefs.language),
            default_rest_seconds: server_prefs.default_rest_seconds,
        };

        let mut preferences = self.preferences;
        apply_user_preferences_state(
            &mut preferences,
            self.theme_signal,
            self.lang_signal,
            next,
            source,
        );

        if !sync_cache {
            match source {
                "sse-apply" => probe::record_sse_preferences(&preferences(), None),
                _ => probe::record_server_preferences(&preferences(), None),
            }
            set_preferences_dom_markers(Some("server"), Some("synced"));
            let mut notice = self.preference_notice;
            notice.set(None);
            return Ok(());
        }

        set_preferences_dom_markers(Some("server"), Some("pending"));

        let units_str = serialize_units(server_prefs.units);
        let theme_str = serialize_theme(server_prefs.theme);
        let lang_str = serialize_language(server_prefs.language);

        if let Err(err) = PreferenceCache::upsert(
            self.db.as_ref(),
            self.user_id,
            &units_str,
            &theme_str,
            &lang_str,
            server_prefs.default_rest_seconds,
        )
        .await
        {
            set_preferences_dom_markers(None, Some("error"));
            probe::set_sync_state(
                probe::ProbeSyncState::Error,
                "preferences.cache_upsert_error",
            );
            return Err(ApplyServerPreferencesError::CacheUpsert(err));
        }

        if let Err(err) = PreferenceCache::mark_synced(self.db.as_ref(), self.user_id).await {
            set_preferences_dom_markers(None, Some("error"));
            probe::set_sync_state(
                probe::ProbeSyncState::Error,
                "preferences.cache_mark_synced_error",
            );
            return Err(ApplyServerPreferencesError::CacheMarkSynced(err));
        }

        match source {
            "sse-apply" => probe::record_sse_preferences(&preferences(), None),
            _ => probe::record_server_preferences(&preferences(), None),
        }
        set_preferences_dom_markers(Some("server"), Some("synced"));
        let mut notice = self.preference_notice;
        notice.set(None);
        Ok(())
    }

    fn parse_cached_preferences(
        cached: &CachedPreferences,
    ) -> Result<UserPreferences, Vec<String>> {
        let parsed_units = parse_units(&cached.units);
        let parsed_theme = parse_theme(&cached.theme);
        let parsed_language = parse_contract_language(&cached.language);

        let mut errors = Vec::new();
        if let Err(err) = parsed_units.as_ref() {
            errors.push(format!("invalid units='{}': {}", cached.units, err));
        }
        if let Err(err) = parsed_theme.as_ref() {
            errors.push(format!("invalid theme='{}': {}", cached.theme, err));
        }
        if let Err(err) = parsed_language.as_ref() {
            errors.push(format!("invalid language='{}': {}", cached.language, err));
        }
        if !(shared::MIN_REST_SECONDS..=shared::MAX_REST_SECONDS)
            .contains(&cached.default_rest_seconds)
        {
            errors.push(format!(
                "invalid default_rest_seconds={}: expected {}..={}",
                cached.default_rest_seconds,
                shared::MIN_REST_SECONDS,
                shared::MAX_REST_SECONDS
            ));
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(UserPreferences {
            weight_unit: contract_to_weight_unit(parsed_units.expect("checked")),
            theme: contract_to_theme_id(parsed_theme.expect("checked")),
            language: contract_to_language(parsed_language.expect("checked")),
            default_rest_seconds: cached.default_rest_seconds,
        })
    }

    fn cache_tuple(cached: &CachedPreferences) -> (String, String, String, u32) {
        (
            cached.units.clone(),
            cached.theme.clone(),
            cached.language.clone(),
            cached.default_rest_seconds,
        )
    }
}

fn is_server_fn_unauthorized(error: &ServerFnError) -> bool {
    matches!(error, ServerFnError::ServerError { code: 401, .. })
}

fn is_terminal_preferences_write_error(error: &ServerFnError) -> bool {
    matches!(
        error,
        ServerFnError::ServerError {
            code: 400 | 401 | 403 | 404 | 409,
            ..
        }
    )
}

pub fn should_write_cache_after_server_refresh(
    cached: Option<&(String, String, String, u32)>,
    incoming: &(String, String, String, u32),
    local_version: u64,
    server_version: Option<u64>,
    cache_synced: bool,
) -> bool {
    if cached != Some(incoming) {
        return true;
    }

    if !cache_synced {
        return true;
    }

    match server_version {
        Some(version) => version != local_version,
        None => true,
    }
}

pub fn should_fetch_preferences_from_server(
    local_version: u64,
    server_version: Option<u64>,
    has_valid_cached_preferences: bool,
    is_marked_stale: bool,
) -> bool {
    if is_marked_stale {
        return true;
    }

    if !has_valid_cached_preferences {
        return true;
    }

    match server_version {
        Some(version) => version != local_version,
        None => true,
    }
}

pub fn mark_preferences_version_fresh(
    cache_versions: &mut Signal<EntityCache>,
    server_version: Option<u64>,
) {
    let Some(server_version) = server_version else {
        return;
    };

    let entity = EntityType::UserPreferences;
    let update_result = {
        let mut cache = cache_versions.write();
        cache.update_version(entity, Some(server_version))
    };

    if update_result == VersionUpdateResult::Updated {
        cache_versions.write().mark_fresh(entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use dioxus::core::RuntimeGuard;
    use dioxus::prelude::{Element, VNode, VirtualDom};
    use std::rc::Rc;

    struct PreferenceHarness {
        _vdom: VirtualDom,
        _guard: RuntimeGuard,
        service: PreferenceService,
        preferences: Signal<UserPreferences>,
        theme_signal: Signal<shared_ui::ThemeId>,
        lang_signal: Signal<shared_ui::Language>,
        cache_versions: Signal<EntityCache>,
        preference_notice: Signal<Option<String>>,
        user_id: Uuid,
    }

    async fn test_service() -> PreferenceHarness {
        let db = Rc::new(
            crate::infrastructure::persistence::init_test_db()
                .await
                .expect("in-memory db should initialize"),
        );
        let mut vdom = VirtualDom::new(|| -> Element { VNode::empty() });
        vdom.rebuild_in_place();
        let guard = RuntimeGuard::new(vdom.runtime());
        let owner = dioxus::core::ScopeId::ROOT;
        let user_id = Uuid::new_v4();
        let preferences = Signal::new_in_scope(UserPreferences::default(), owner);
        let theme_signal = Signal::new_in_scope(shared_ui::ThemeId::Athletic, owner);
        let lang_signal = Signal::new_in_scope(shared_ui::Language::En, owner);
        let cache_versions = Signal::new_in_scope(EntityCache::new(), owner);
        let preference_notice = Signal::new_in_scope(None::<String>, owner);
        let mutation_store = MutationStore::from_rc(Rc::clone(&db));

        let service = PreferenceService::new(
            db,
            user_id,
            mutation_store,
            PreferenceSignals {
                preferences,
                theme_signal: Some(theme_signal),
                lang_signal: Some(lang_signal),
                cache_versions,
                preference_notice,
                bootstrap_epoch: Rc::new(Cell::new(0_u64)),
            },
        );

        PreferenceHarness {
            _vdom: vdom,
            _guard: guard,
            service,
            preferences,
            theme_signal,
            lang_signal,
            cache_versions,
            preference_notice,
            user_id,
        }
    }

    #[test]
    fn bootstrap_cache_write_policy_skips_identical_payload() {
        let cached = (
            "kg".to_string(),
            "dark".to_string(),
            "en".to_string(),
            shared::DEFAULT_REST_SECONDS,
        );
        let incoming = (
            "kg".to_string(),
            "dark".to_string(),
            "en".to_string(),
            shared::DEFAULT_REST_SECONDS,
        );
        let cases = [
            (
                "matching payload, synced cache, matching version",
                Some(7),
                true,
                false,
            ),
            (
                "matching payload, synced cache, mismatched version",
                Some(8),
                true,
                true,
            ),
            (
                "matching payload, unsynced cache, matching version",
                Some(7),
                false,
                true,
            ),
        ];

        for (label, server_version, cache_synced, expected) in cases {
            assert_eq!(
                should_write_cache_after_server_refresh(
                    Some(&cached),
                    &incoming,
                    7,
                    server_version,
                    cache_synced,
                ),
                expected,
                "{label}"
            );
        }
    }

    #[test]
    fn bootstrap_cache_write_policy_writes_on_change() {
        let cached = (
            "kg".to_string(),
            "dark".to_string(),
            "en".to_string(),
            shared::DEFAULT_REST_SECONDS,
        );
        let changed_payloads = [
            (
                "theme change",
                (
                    "kg".to_string(),
                    "liquid".to_string(),
                    "en".to_string(),
                    shared::DEFAULT_REST_SECONDS,
                ),
            ),
            (
                "units change",
                (
                    "lb".to_string(),
                    "dark".to_string(),
                    "en".to_string(),
                    shared::DEFAULT_REST_SECONDS,
                ),
            ),
            (
                "language change",
                (
                    "kg".to_string(),
                    "dark".to_string(),
                    "fr".to_string(),
                    shared::DEFAULT_REST_SECONDS,
                ),
            ),
            (
                "rest change",
                ("kg".to_string(), "dark".to_string(), "en".to_string(), 135),
            ),
        ];

        for (label, incoming) in changed_payloads {
            assert!(
                should_write_cache_after_server_refresh(Some(&cached), &incoming, 7, Some(7), true,),
                "{label} should force a cache write"
            );
        }

        let theme_changed = (
            "kg".to_string(),
            "liquid".to_string(),
            "en".to_string(),
            shared::DEFAULT_REST_SECONDS,
        );
        assert!(
            should_write_cache_after_server_refresh(None, &theme_changed, 7, Some(7), true),
            "missing cache should still write the incoming payload"
        );
    }

    #[test]
    fn bootstrap_cache_write_policy_writes_when_sync_state_or_version_differs() {
        let cached = (
            "kg".to_string(),
            "dark".to_string(),
            "en".to_string(),
            shared::DEFAULT_REST_SECONDS,
        );
        let incoming = cached.clone();
        assert!(
            should_write_cache_after_server_refresh(Some(&cached), &incoming, 7, Some(8), true,),
            "server version drift should force a cache write"
        );
        assert!(
            should_write_cache_after_server_refresh(Some(&cached), &incoming, 7, Some(7), false,),
            "an unsynced cache should rewrite even with identical payload"
        );
        assert!(
            !should_write_cache_after_server_refresh(Some(&cached), &incoming, 7, Some(7), true,),
            "identical, synced, matching-version payloads should not rewrite"
        );
    }

    #[test]
    fn should_fetch_preferences_when_cache_missing_even_with_equal_versions() {
        assert!(
            should_fetch_preferences_from_server(7, Some(7), false, false),
            "missing valid cache should force a fetch even when versions match"
        );
        assert!(
            should_fetch_preferences_from_server(7, Some(7), false, true),
            "stale state should still fetch when the cache is missing"
        );
        assert!(
            !should_fetch_preferences_from_server(7, Some(7), true, false),
            "fresh valid cache with equal version should skip the fetch"
        );
    }

    #[test]
    fn should_fetch_preferences_when_marked_stale_even_if_versions_match() {
        assert!(
            should_fetch_preferences_from_server(7, Some(7), true, true),
            "stale marker should override equal versions"
        );
        assert!(
            should_fetch_preferences_from_server(7, Some(9), true, true),
            "stale marker should also override version drift"
        );
        assert!(
            !should_fetch_preferences_from_server(7, Some(7), true, false),
            "without stale marker the fresh cache can skip the fetch"
        );
    }

    #[test]
    fn should_fetch_preferences_when_server_version_unknown() {
        assert!(
            should_fetch_preferences_from_server(7, None, true, false),
            "unknown server version must trigger authoritative fetch"
        );
        assert!(
            should_fetch_preferences_from_server(7, None, false, false),
            "unknown server version should fetch even when cache is missing"
        );
        assert!(
            !should_fetch_preferences_from_server(7, Some(7), true, false),
            "known matching server version with valid cache can skip the fetch"
        );
    }

    #[test]
    fn bootstrap_cache_write_policy_writes_when_server_version_unknown() {
        let cached = (
            "lb".to_string(),
            "dark".to_string(),
            "en".to_string(),
            shared::DEFAULT_REST_SECONDS,
        );
        let incoming = cached.clone();
        assert!(
            should_write_cache_after_server_refresh(Some(&cached), &incoming, 7, None, true),
            "unknown server version should rewrite even an identical synced cache"
        );
        assert!(
            should_write_cache_after_server_refresh(Some(&cached), &incoming, 7, None, false),
            "unknown server version should also rewrite when the cache is unsynced"
        );
        assert!(
            !should_write_cache_after_server_refresh(Some(&cached), &incoming, 7, Some(7), true),
            "known matching server version should not rewrite an identical synced cache"
        );
    }

    #[test]
    fn preferences_write_error_classifier_only_treats_client_rejections_as_terminal() {
        let terminal_cases = [
            ("validation", 400),
            ("unauthorized", 401),
            ("forbidden", 403),
            ("missing", 404),
            ("conflict", 409),
        ];
        for (label, code) in terminal_cases {
            assert!(
                is_terminal_preferences_write_error(&ServerFnError::ServerError {
                    message: label.to_string(),
                    code,
                    details: None,
                }),
                "{label} should be treated as a terminal rejection"
            );
        }

        let retryable_cases = [
            ServerFnError::ServerError {
                message: "internal".to_string(),
                code: 500,
                details: None,
            },
            ServerFnError::ServerError {
                message: "gateway".to_string(),
                code: 502,
                details: None,
            },
            ServerFnError::Serialization("args".to_string()),
            ServerFnError::Deserialization("response".to_string()),
            ServerFnError::Response("http response".to_string()),
        ];
        for error in retryable_cases {
            assert!(
                !is_terminal_preferences_write_error(&error),
                "{error:?} should remain retryable"
            );
        }
    }

    #[test]
    fn server_fn_unauthorized_helper_only_matches_401() {
        assert!(is_server_fn_unauthorized(&ServerFnError::ServerError {
            message: "unauthorized".to_string(),
            code: 401,
            details: None,
        }));

        let non_401_cases = [
            ServerFnError::ServerError {
                message: "forbidden".to_string(),
                code: 403,
                details: None,
            },
            ServerFnError::ServerError {
                message: "bad-request".to_string(),
                code: 400,
                details: None,
            },
            ServerFnError::ServerError {
                message: "internal".to_string(),
                code: 500,
                details: None,
            },
            ServerFnError::Serialization("request".to_string()),
            ServerFnError::Deserialization("response".to_string()),
        ];

        for error in non_401_cases {
            assert!(
                !is_server_fn_unauthorized(&error),
                "{error:?} should not be treated as unauthorized"
            );
        }
    }

    #[test]
    fn parse_cached_preferences_returns_user_preferences_when_values_are_valid() {
        let cached = CachedPreferences {
            user_id: Uuid::new_v4().to_string(),
            units: "lb".to_string(),
            theme: "liquid".to_string(),
            language: "fr".to_string(),
            default_rest_seconds: 90,
            updated_at: Utc::now(),
            synced_at: Some(Utc::now()),
        };

        let parsed = PreferenceService::parse_cached_preferences(&cached)
            .expect("cached prefs should parse");

        assert_eq!(parsed.default_rest_seconds, 90);
        assert_eq!(
            serialize_units(weight_unit_to_contract(parsed.weight_unit)),
            "lb"
        );
        assert_eq!(
            serialize_theme(theme_id_to_contract(parsed.theme)),
            "liquid"
        );
        assert_eq!(
            serialize_language(language_to_contract(parsed.language)),
            "fr"
        );
    }

    #[test]
    fn parse_cached_preferences_reports_all_invalid_fields() {
        let cached = CachedPreferences {
            user_id: Uuid::new_v4().to_string(),
            units: "stone".to_string(),
            theme: "sepia".to_string(),
            language: "xx".to_string(),
            default_rest_seconds: shared::MAX_REST_SECONDS + 1,
            updated_at: Utc::now(),
            synced_at: None,
        };

        let errors = PreferenceService::parse_cached_preferences(&cached)
            .expect_err("invalid cached prefs should report errors");

        assert_eq!(errors.len(), 4);
        assert!(errors
            .iter()
            .any(|error| error.contains("invalid units='stone'")));
        assert!(errors
            .iter()
            .any(|error| error.contains("invalid theme='sepia'")));
        assert!(errors
            .iter()
            .any(|error| error.contains("invalid language='xx'")));
        assert!(errors
            .iter()
            .any(|error| error.contains("invalid default_rest_seconds=")));
    }

    #[test]
    fn apply_local_user_change_updates_preferences_theme_and_language_signals() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime should build");
        let mut harness = runtime.block_on(test_service());
        harness
            .preference_notice
            .set(Some("stale notice".to_string()));

        let next = UserPreferences {
            language: shared_ui::Language::Fr,
            weight_unit: crate::domain::preferences::WeightUnit::Lb,
            theme: shared_ui::ThemeId::Liquid,
            default_rest_seconds: 150,
        };

        assert!(harness.service.apply_local_user_change(next.clone()));
        assert_eq!(harness.preferences.read().clone(), next);
        assert_eq!(*harness.theme_signal.read(), shared_ui::ThemeId::Liquid);
        assert_eq!(*harness.lang_signal.read(), shared_ui::Language::Fr);
        assert_eq!(harness.preferences.read().weight_unit.label(), "lb");
        assert_eq!(harness.preferences.read().default_rest_seconds, 150);
        assert_eq!(
            harness.preference_notice.read().as_deref(),
            Some("stale notice")
        );
        let cached = runtime
            .block_on(PreferenceCache::get(
                harness.service.db.as_ref(),
                harness.user_id,
            ))
            .expect("cache lookup should succeed");
        assert!(
            cached.is_none(),
            "apply_local_user_change should only mutate signals, not persist cache"
        );
        let pending = runtime
            .block_on(harness.service.mutation_store.get_pending())
            .expect("pending lookup should succeed");
        assert!(
            pending.is_empty(),
            "apply_local_user_change should not queue network work by itself"
        );
        assert!(
            !harness.service.apply_local_user_change(next),
            "reapplying identical preferences should be a no-op"
        );
    }

    #[tokio::test]
    async fn persist_local_user_change_writes_cache_and_clears_notice() {
        let mut harness = test_service().await;
        harness
            .preference_notice
            .set(Some("stale notice".to_string()));

        harness
            .service
            .persist_local_user_change(UserPreferences {
                language: shared_ui::Language::De,
                weight_unit: crate::domain::preferences::WeightUnit::Lb,
                theme: shared_ui::ThemeId::Dark,
                default_rest_seconds: 135,
            })
            .await;

        let cached = PreferenceCache::get(harness.service.db.as_ref(), harness.user_id)
            .await
            .expect("cache lookup should succeed")
            .expect("preferences should be cached");
        assert_eq!(cached.units, "lb");
        assert_eq!(cached.theme, "dark");
        assert_eq!(cached.language, "de");
        assert_eq!(cached.default_rest_seconds, 135);
        assert!(
            cached.synced_at.is_none(),
            "local change should remain unsynced"
        );
        assert_eq!(
            harness.preferences.read().clone(),
            UserPreferences::default(),
            "persist_local_user_change should not mutate in-memory preferences on its own"
        );
        let pending = harness
            .service
            .mutation_store
            .get_pending()
            .await
            .expect("pending lookup should succeed");
        assert!(
            pending.is_empty(),
            "persist_local_user_change should only update cache, not queue network work"
        );
        assert_eq!(harness.preference_notice.read().as_deref(), None);
    }

    #[tokio::test]
    async fn apply_server_preferences_updates_cache_and_marks_synced() {
        let mut harness = test_service().await;
        harness.preference_notice.set(Some(
            "The server rejected this preference change.".to_string(),
        ));

        harness
            .service
            .apply_server_preferences(
                shared::UserPreferencesResponse {
                    units: shared::Units::Lb,
                    theme: shared::Theme::Dark,
                    language: shared::ContractLanguage::Fr,
                    default_rest_seconds: 105,
                },
                "test-server-apply",
                true,
            )
            .await
            .expect("server preferences should apply");

        let current = harness.preferences.read().clone();
        assert_eq!(current.weight_unit.label(), "lb");
        assert_eq!(current.theme.slug(), "dark");
        assert_eq!(shared_ui::Language::Fr, current.language);
        assert_eq!(current.default_rest_seconds, 105);
        assert_eq!(*harness.theme_signal.read(), shared_ui::ThemeId::Dark);
        assert_eq!(*harness.lang_signal.read(), shared_ui::Language::Fr);

        let cached = PreferenceCache::get(harness.service.db.as_ref(), harness.user_id)
            .await
            .expect("cache lookup should succeed")
            .expect("server apply should populate cache");
        assert_eq!(cached.units, "lb");
        assert_eq!(cached.theme, "dark");
        assert_eq!(cached.language, "fr");
        assert_eq!(cached.default_rest_seconds, 105);
        assert!(
            cached.synced_at.is_some(),
            "server apply with sync_cache should mark cache synced"
        );
        assert_eq!(
            harness.preference_notice.read().as_deref(),
            None,
            "successful server apply should clear stale rejection notices"
        );
    }

    #[tokio::test]
    async fn apply_server_preferences_can_skip_cache_write() {
        let mut harness = test_service().await;
        harness.preference_notice.set(Some(
            "The server rejected this preference change.".to_string(),
        ));
        PreferenceCache::upsert(
            harness.service.db.as_ref(),
            harness.user_id,
            "kg",
            "athletic",
            "en",
            90,
        )
        .await
        .expect("seed cache");
        let cached_before = PreferenceCache::get(harness.service.db.as_ref(), harness.user_id)
            .await
            .expect("cache lookup before apply")
            .expect("seeded cache should exist");

        harness
            .service
            .apply_server_preferences(
                shared::UserPreferencesResponse {
                    units: shared::Units::Kg,
                    theme: shared::Theme::Liquid,
                    language: shared::ContractLanguage::Es,
                    default_rest_seconds: 95,
                },
                "test-server-apply",
                false,
            )
            .await
            .expect("server preferences should apply");

        assert_eq!(harness.preferences.read().language, shared_ui::Language::Es);
        assert_eq!(*harness.theme_signal.read(), shared_ui::ThemeId::Liquid);
        assert_eq!(*harness.lang_signal.read(), shared_ui::Language::Es);
        assert_eq!(harness.preferences.read().weight_unit.label(), "kg");
        assert_eq!(harness.preferences.read().default_rest_seconds, 95);
        let cached_after = PreferenceCache::get(harness.service.db.as_ref(), harness.user_id)
            .await
            .expect("cache lookup after apply")
            .expect("cache should remain untouched");
        assert_eq!(cached_after.units, cached_before.units);
        assert_eq!(cached_after.theme, cached_before.theme);
        assert_eq!(cached_after.language, cached_before.language);
        assert_eq!(
            cached_after.default_rest_seconds,
            cached_before.default_rest_seconds
        );
        assert_eq!(cached_after.synced_at, cached_before.synced_at);
        assert_eq!(harness.preference_notice.read().as_deref(), None);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn dispatch_local_user_change_sets_terminal_rejection_notice_for_401_errors() {
        let harness = test_service().await;
        let prefs = UserPreferences {
            language: shared_ui::Language::Fr,
            weight_unit: crate::domain::preferences::WeightUnit::Kg,
            theme: shared_ui::ThemeId::Dark,
            default_rest_seconds: 95,
        };

        harness
            .service
            .dispatch_local_user_change(prefs, "invalid-auth".to_string())
            .await;

        let pending = harness
            .service
            .mutation_store
            .get_pending()
            .await
            .expect("pending lookup should succeed");
        assert_eq!(
            pending.len(),
            0,
            "terminal 401 should not enqueue fallback mutation"
        );
        assert_eq!(
            crate::infrastructure::persistence::DeadLetterStore::count(
                harness.service.db.as_ref(),
            )
            .await
            .expect("dead-letter count should succeed"),
            0,
            "terminal 401 should not dead-letter a fallback mutation"
        );
        assert_eq!(
            harness.preferences.read().clone(),
            UserPreferences::default(),
            "dispatch-only call should not mutate in-memory preferences when the server rejects it"
        );
        assert!(
            PreferenceCache::get(harness.service.db.as_ref(), harness.user_id)
                .await
                .expect("cache lookup should succeed")
                .is_none(),
            "dispatch-only terminal rejection should not create a cache row"
        );
        assert_eq!(
            harness.preference_notice.read().as_deref(),
            Some("The server rejected this preference change.")
        );
    }

    #[tokio::test]
    async fn mark_preferences_version_fresh_clears_stale_state_only_when_server_version_exists() {
        let mut harness = test_service().await;
        {
            let mut cache = harness.cache_versions.write();
            assert_eq!(
                cache.update_version(EntityType::UserPreferences, Some(2)),
                VersionUpdateResult::Updated
            );
            assert!(cache.is_stale(EntityType::UserPreferences));
        }

        mark_preferences_version_fresh(&mut harness.cache_versions, Some(3));
        assert!(
            !harness
                .cache_versions
                .read()
                .is_stale(EntityType::UserPreferences),
            "updated version should be marked fresh"
        );
        assert_eq!(
            harness
                .cache_versions
                .read()
                .get_version(EntityType::UserPreferences),
            3
        );

        let before = harness
            .cache_versions
            .read()
            .get_version(EntityType::UserPreferences);
        let stale_before = harness
            .cache_versions
            .read()
            .is_stale(EntityType::UserPreferences);
        mark_preferences_version_fresh(&mut harness.cache_versions, None);
        assert_eq!(
            harness
                .cache_versions
                .read()
                .get_version(EntityType::UserPreferences),
            before
        );
        assert_eq!(
            harness
                .cache_versions
                .read()
                .is_stale(EntityType::UserPreferences),
            stale_before
        );

        {
            let mut cache = harness.cache_versions.write();
            assert_eq!(
                cache.update_version(EntityType::UserPreferences, Some(10)),
                VersionUpdateResult::Updated
            );
            assert_eq!(
                cache.update_version(EntityType::UserPreferences, Some(10)),
                VersionUpdateResult::NoChange
            );
            assert_eq!(
                cache.update_version(EntityType::UserPreferences, Some(2)),
                VersionUpdateResult::NeedsReset
            );
            assert!(cache.is_stale(EntityType::UserPreferences));
        }

        mark_preferences_version_fresh(&mut harness.cache_versions, Some(2));
        assert!(
            harness
                .cache_versions
                .read()
                .is_stale(EntityType::UserPreferences),
            "older server versions should not clear stale state"
        );

        let mut fresh_harness = test_service().await;
        mark_preferences_version_fresh(&mut fresh_harness.cache_versions, Some(5));
        assert_eq!(
            fresh_harness
                .cache_versions
                .read()
                .get_version(EntityType::UserPreferences),
            5
        );
        assert!(
            !fresh_harness
                .cache_versions
                .read()
                .is_stale(EntityType::UserPreferences),
            "first authoritative version should establish a fresh cache state"
        );
    }
}

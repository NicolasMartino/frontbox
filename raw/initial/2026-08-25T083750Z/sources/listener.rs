//! SSE listener with cache invalidation and eager refetch.
//!
//! Handles:
//! - Version updates from SSE invalidation events
//! - Reconnect version check with server
//! - Self-correction on version anomalies
//! - Eager refetch of stale entities
//! - Preference apply/cache sync delegated to `PreferenceService`

use std::collections::HashMap;
use std::rc::Rc;

use async_trait::async_trait;
use dioxus::prelude::*;

use crate::application::services::preferences::{ApplyServerPreferencesError, PreferenceService};
use crate::debug::{log_sse_event, ConnectionStatus, SessionId, SseEventType};
use crate::infrastructure::persistence::{
    Database, EntityCache, EntityType, ExerciseStore, SessionStore, StoreResult,
    VersionUpdateResult,
};
use crate::infrastructure::probe;

use super::{MutationRejectedEvent, SseError, SseMessage, SseStream};

#[cfg(feature = "server")]
use observability::record_sse_reconnection;

#[derive(Clone)]
pub struct SseListenerConfig {
    pub cache: Signal<EntityCache>,
    pub db: Rc<Database>,
    pub sse_url: String,
    pub access_token: String,
    pub session_id: SessionId,
    pub connection_status: Signal<ConnectionStatus>,
    pub preference_service: PreferenceService,
}

#[derive(Clone)]
struct ListenerContext<'a> {
    db: &'a Database,
    auth_header: &'a str,
    preference_service: PreferenceService,
}

/// Sleep for the given number of milliseconds.
#[cfg(target_arch = "wasm32")]
async fn sleep_ms(ms: u32) {
    gloo_timers::future::TimeoutFuture::new(ms).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn sleep_ms(ms: u32) {
    tokio::time::sleep(std::time::Duration::from_millis(ms as u64)).await;
}

/// Eager refetch a single entity type from server and store locally.
async fn eager_refetch(
    entity: EntityType,
    cache: &mut Signal<EntityCache>,
    ctx: ListenerContext<'_>,
) {
    crate::log!("[SSE] Eager refetch: {}", entity.as_str());

    let result: StoreResult<()> = match entity {
        EntityType::Exercise => {
            match crate::api::preferences::fetch_exercises(ctx.auth_header.to_string()).await {
                Ok(exercises) => ExerciseStore::replace_all(ctx.db, &exercises).await,
                Err(e) => Err(format!("Fetch failed: {}", e)),
            }
        }
        EntityType::Session => {
            match crate::api::preferences::fetch_sessions(ctx.auth_header.to_string()).await {
                Ok(sessions) => SessionStore::replace_all(ctx.db, &sessions).await,
                Err(e) => Err(format!("Fetch failed: {}", e)),
            }
        }
        EntityType::Template => {
            // Template endpoint not yet implemented in workout-api
            // When added, replace this with: fetch_templates() similar to fetch_exercises
            crate::log!(
                "[SSE] Skipping eager refetch for templates (endpoint not yet implemented)"
            );
            Ok(()) // Mark as success to avoid retry loop
        }
        EntityType::UserPreferences => {
            match crate::api::preferences::fetch_user_preferences(ctx.auth_header.to_string()).await
            {
                Ok(Some(server_prefs)) => {
                    metrics::counter!("preferences.sse_refresh_total", "result" => "fetched")
                        .increment(1);
                    match ctx
                        .preference_service
                        .apply_server_preferences(server_prefs, "sse-apply", true)
                        .await
                    {
                        Ok(()) => {
                            metrics::counter!(
                                "preferences.sse_cache_write_total",
                                "phase" => "mark_synced",
                                "result" => "ok"
                            )
                            .increment(1);
                        }
                        Err(ApplyServerPreferencesError::CacheUpsert(err)) => {
                            metrics::counter!(
                                "preferences.sse_cache_write_total",
                                "phase" => "upsert",
                                "result" => "error"
                            )
                            .increment(1);
                            crate::log!("[SSE] Failed to update preference cache: {}", err);
                        }
                        Err(ApplyServerPreferencesError::CacheMarkSynced(err)) => {
                            metrics::counter!(
                                "preferences.sse_cache_write_total",
                                "phase" => "mark_synced",
                                "result" => "error"
                            )
                            .increment(1);
                            crate::log!("[SSE] Failed to mark preference cache synced: {}", err);
                        }
                    }

                    Ok(())
                }
                Ok(None) => {
                    metrics::counter!("preferences.sse_refresh_total", "result" => "empty")
                        .increment(1);
                    Ok(())
                }
                Err(e) => {
                    metrics::counter!("preferences.sse_refresh_total", "result" => "fetch_error")
                        .increment(1);
                    Err(format!("Fetch failed: {}", e))
                }
            }
        }
    };

    match result {
        Ok(()) => {
            cache.write().mark_fresh(entity);
            crate::log!("[SSE] Eager refetch complete: {}", entity.as_str());
        }
        Err(e) => {
            crate::log!("[SSE] Eager refetch failed for {}: {}", entity.as_str(), e);
            // Keep marked stale - will retry on next event or screen access
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn connect_stream(
    sse_url: &str,
    access_token: &str,
    session_id: &SessionId,
    _auth_header: &str,
) -> Result<SseStream, String> {
    SseStream::connect_with_session(sse_url, access_token, session_id)
        .map_err(|e| format!("Connection failed: {}", e))
}

#[cfg(target_arch = "wasm32")]
async fn connect_stream(
    sse_url: &str,
    _access_token: &str,
    session_id: &SessionId,
    auth_header: &str,
) -> Result<SseStream, String> {
    let ticket = crate::api::sse::create_sse_ticket(auth_header.to_string())
        .await
        .map_err(|e| format!("Failed to get SSE ticket: {}", e))?;

    SseStream::connect_with_ticket_and_session(sse_url, &ticket, session_id)
        .map_err(|e| format!("Connection failed: {}", e))
}

#[async_trait(?Send)]
trait EventStreamLike {
    async fn next_event(&mut self) -> Option<Result<SseMessage, SseError>>;
}

#[async_trait(?Send)]
impl EventStreamLike for SseStream {
    async fn next_event(&mut self) -> Option<Result<SseMessage, SseError>> {
        SseStream::next_event(self).await
    }
}

async fn listen_for_events(
    stream: &mut impl EventStreamLike,
    cache: &mut Signal<EntityCache>,
    listener_ctx: ListenerContext<'_>,
    connection_status: &mut Signal<ConnectionStatus>,
    session_id: &SessionId,
) -> u32 {
    loop {
        match stream.next_event().await {
            Some(Ok(SseMessage::Invalidation(event))) => {
                handle_invalidation_event(&event, cache, listener_ctx.clone()).await;
            }
            Some(Ok(SseMessage::MutationRejected(event))) => {
                handle_mutation_rejected_event(&event, cache, listener_ctx.clone()).await;
            }
            Some(Err(SseError::Timeout)) => {
                let plan = reconnect_plan(Some(&SseError::Timeout));
                crate::log!("[SSE] {}, reconnecting...", plan.log_message);
                connection_status.set(plan.connection_status);
                log_sse_event(
                    session_id,
                    None,
                    plan.event_type.clone(),
                    Some(plan.log_message),
                );

                #[cfg(feature = "server")]
                record_sse_reconnection(plan.metric_reason);

                return plan.delay_ms;
            }
            Some(Err(SseError::Unauthorized)) => {
                let plan = reconnect_plan(Some(&SseError::Unauthorized));
                crate::log!("[SSE] {}", plan.log_message);
                connection_status.set(plan.connection_status);
                log_sse_event(
                    session_id,
                    None,
                    plan.event_type.clone(),
                    Some(plan.log_message),
                );

                #[cfg(feature = "server")]
                record_sse_reconnection(plan.metric_reason);

                return plan.delay_ms;
            }
            Some(Err(e)) => {
                let plan = reconnect_plan(Some(&e));
                crate::log!("[SSE] {}, reconnecting...", plan.log_message);
                connection_status.set(plan.connection_status);
                log_sse_event(
                    session_id,
                    None,
                    plan.event_type.clone(),
                    Some(plan.log_message),
                );

                #[cfg(feature = "server")]
                record_sse_reconnection(plan.metric_reason);

                return plan.delay_ms;
            }
            None => {
                let plan = reconnect_plan(None);
                crate::log!("[SSE] {}, reconnecting...", plan.log_message);
                connection_status.set(plan.connection_status);
                log_sse_event(
                    session_id,
                    None,
                    plan.event_type.clone(),
                    Some(plan.log_message),
                );

                #[cfg(feature = "server")]
                record_sse_reconnection(plan.metric_reason);

                return plan.delay_ms;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReconnectPlan {
    delay_ms: u32,
    connection_status: ConnectionStatus,
    event_type: SseEventType,
    log_message: String,
    metric_reason: &'static str,
}

fn reconnect_plan(error: Option<&SseError>) -> ReconnectPlan {
    match error {
        Some(SseError::Unauthorized) => ReconnectPlan {
            delay_ms: 5000,
            connection_status: ConnectionStatus::Error,
            event_type: SseEventType::Error,
            log_message: "Unauthorized, token may be expired".to_string(),
            metric_reason: "unauthorized",
        },
        Some(SseError::Timeout) => ReconnectPlan {
            delay_ms: 1000,
            connection_status: ConnectionStatus::Error,
            event_type: SseEventType::Timeout,
            log_message: "Listener timeout".to_string(),
            metric_reason: "timeout",
        },
        Some(error) => ReconnectPlan {
            delay_ms: 1000,
            connection_status: ConnectionStatus::Error,
            event_type: SseEventType::Error,
            log_message: format!("Stream error: {}", error),
            metric_reason: "error",
        },
        None => ReconnectPlan {
            delay_ms: 1000,
            connection_status: ConnectionStatus::Disconnected,
            event_type: SseEventType::Close,
            log_message: "Stream ended".to_string(),
            metric_reason: "network",
        },
    }
}

/// Run the SSE listener with cache invalidation.
///
/// This function runs forever, reconnecting as needed.
/// It should be spawned as a background task when the user is authenticated.
pub async fn run_sse_listener(config: SseListenerConfig) {
    let mut cache = config.cache;
    let db = config.db;
    let sse_url = config.sse_url;
    let access_token = config.access_token;
    let session_id = config.session_id;
    let mut connection_status = config.connection_status;
    let preference_service = config.preference_service;
    let auth_header = format!("Bearer {}", access_token);

    loop {
        connection_status.set(ConnectionStatus::Connecting);
        probe::set_connection_status(ConnectionStatus::Connecting);
        log_sse_event(
            &session_id,
            None,
            SseEventType::Reconnect,
            Some("Connecting SSE stream".to_string()),
        );

        let mut stream =
            match connect_stream(&sse_url, &access_token, &session_id, &auth_header).await {
                Ok(s) => s,
                Err(e) => {
                    crate::log!("[SSE] {}, retrying in 5s", e);
                    connection_status.set(ConnectionStatus::Error);
                    probe::set_connection_status(ConnectionStatus::Error);
                    log_sse_event(&session_id, None, SseEventType::Error, Some(e));
                    sleep_ms(5000).await;
                    continue;
                }
            };

        let listener_ctx = ListenerContext {
            db: db.as_ref(),
            auth_header: &auth_header,
            preference_service: preference_service.clone(),
        };

        // On connect, check versions with server
        if let Err(e) = sync_versions_on_connect(&mut cache, listener_ctx.clone()).await {
            crate::log!("[SSE] Version sync failed: {}", e);
            log_sse_event(
                &session_id,
                None,
                SseEventType::Error,
                Some(format!("Version sync failed: {}", e)),
            );
        }

        crate::log!("[SSE] Connected, listening for events");
        connection_status.set(ConnectionStatus::Connected);
        probe::set_connection_status(ConnectionStatus::Connected);
        log_sse_event(
            &session_id,
            None,
            SseEventType::Open,
            Some("SSE listener connected".to_string()),
        );

        let reconnect_delay_ms = listen_for_events(
            &mut stream,
            &mut cache,
            listener_ctx,
            &mut connection_status,
            &session_id,
        )
        .await;

        // Close current stream
        stream.close();

        // Wait before reconnect
        sleep_ms(reconnect_delay_ms).await;
    }
}

/// Sync versions with server on connect.
async fn sync_versions_on_connect(
    cache: &mut Signal<EntityCache>,
    ctx: ListenerContext<'_>,
) -> Result<(), String> {
    let client_versions = cache.read().get_versions();

    match crate::api::preferences::check_cache_versions(
        ctx.auth_header.to_string(),
        client_versions,
    )
    .await
    {
        Ok(server_versions) => {
            let parsed: HashMap<EntityType, u64> = server_versions
                .into_iter()
                .filter_map(|(k, v)| EntityType::parse(&k).map(|e| (e, v)))
                .collect();
            if let Some(version) = parsed.get(&EntityType::UserPreferences) {
                probe::set_last_applied_version(Some(*version));
            }

            // Check versions and get entities that need reset
            let needs_reset = cache.write().check_versions(&parsed);

            // Reset any anomalous entities
            for entity in needs_reset {
                crate::log!("[SSE] Anomaly detected for {}, resetting", entity.as_str());
                if let Err(e) = crate::api::preferences::reset_cache_version(
                    ctx.auth_header.to_string(),
                    entity.as_str().to_string(),
                )
                .await
                {
                    crate::log!("[SSE] Reset failed for {}: {}", entity.as_str(), e);
                }
                cache.write().reset_version(entity);
            }

            // Eager refetch all stale entities after reconnect
            let stale_entities: Vec<EntityType> = cache.read().stale_entities();

            for entity in stale_entities {
                eager_refetch(entity, cache, ctx.clone()).await;
            }

            crate::log!("[SSE] Reconnect sync complete");
            Ok(())
        }
        Err(e) => {
            crate::log!("[SSE] Version check failed: {}", e);
            // Mark all stale and refetch
            cache.write().mark_all_stale();
            for entity in EntityType::all() {
                eager_refetch(*entity, cache, ctx.clone()).await;
            }
            Err(format!("Version check failed: {}", e))
        }
    }
}

/// Handle a single invalidation event.
async fn handle_invalidation_event(
    event: &shared::frontend::sse::InvalidationEvent,
    cache: &mut Signal<EntityCache>,
    ctx: ListenerContext<'_>,
) {
    if let Some(entity) = EntityType::parse(&event.entity) {
        let result = cache.write().update_version(entity, Some(event.version));

        match result {
            VersionUpdateResult::Updated => {
                crate::log!("[SSE] {} updated to v{}", event.entity, event.version);
                // Eager refetch immediately
                eager_refetch(entity, cache, ctx).await;
                if entity == EntityType::UserPreferences {
                    probe::set_last_applied_version(Some(event.version));
                }
            }
            VersionUpdateResult::NeedsReset => {
                crate::log!(
                    "[SSE] Anomaly: {} server v{} < client, resetting",
                    event.entity,
                    event.version
                );

                // Reset both sides
                if let Err(e) = crate::api::preferences::reset_cache_version(
                    ctx.auth_header.to_string(),
                    entity.as_str().to_string(),
                )
                .await
                {
                    crate::log!("[SSE] Reset failed: {}", e);
                }
                cache.write().reset_version(entity);
                // Eager refetch after reset
                eager_refetch(entity, cache, ctx).await;
                if entity == EntityType::UserPreferences {
                    probe::set_last_applied_version(Some(event.version));
                }
            }
            VersionUpdateResult::NoChange => {
                // Already in sync, no action needed
            }
        }
    } else {
        crate::log!("[SSE] Unknown entity type: {}", event.entity);
    }
}

/// Handle a `mutation_rejected` event.
async fn handle_mutation_rejected_event(
    event: &MutationRejectedEvent,
    cache: &mut Signal<EntityCache>,
    ctx: ListenerContext<'_>,
) {
    let error_message = event
        .error
        .as_ref()
        .map(|e| e.message.clone())
        .unwrap_or_else(|| "unknown rejection error".to_string());

    crate::log!(
        "[SSE] Mutation rejected (batch_id={}, status={}): {}",
        event.batch_id,
        event.status,
        error_message
    );

    // Rejections can leave optimistic preference state stale. Refreshing
    // preferences keeps UI and server state convergent.
    eager_refetch(EntityType::UserPreferences, cache, ctx).await;
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    use super::*;
    use crate::application::services::preferences::PreferenceSignals;
    use crate::application::services::PreferenceService;
    use crate::domain::preferences::UserPreferences;
    use crate::infrastructure::persistence::{init_test_db, Database, MutationStore};
    use dioxus::core::RuntimeGuard;
    use uuid::Uuid;

    struct ListenerTestHarness {
        _vdom: VirtualDom,
        _guard: RuntimeGuard,
        cache: Signal<EntityCache>,
        connection_status: Signal<ConnectionStatus>,
        db: Rc<Database>,
        preference_service: PreferenceService,
    }

    struct FakeStream {
        events: VecDeque<Result<SseMessage, SseError>>,
    }

    #[async_trait(?Send)]
    impl EventStreamLike for FakeStream {
        async fn next_event(&mut self) -> Option<Result<SseMessage, SseError>> {
            self.events.pop_front()
        }
    }

    async fn listener_test_harness() -> ListenerTestHarness {
        let db = Rc::new(
            init_test_db()
                .await
                .expect("in-memory db should initialize"),
        );
        let mut vdom = VirtualDom::new(|| -> Element { VNode::empty() });
        vdom.rebuild_in_place();
        let guard = RuntimeGuard::new(vdom.runtime());
        let owner = dioxus::core::ScopeId::ROOT;
        let cache = Signal::new_in_scope(EntityCache::new(), owner);
        let connection_status = Signal::new_in_scope(ConnectionStatus::Disconnected, owner);
        let preferences = Signal::new_in_scope(UserPreferences::default(), owner);
        let cache_versions = Signal::new_in_scope(EntityCache::new(), owner);
        let preference_notice = Signal::new_in_scope(None::<String>, owner);
        let mutation_store = MutationStore::from_rc(Rc::clone(&db));
        let preference_service = PreferenceService::new(
            Rc::clone(&db),
            Uuid::new_v4(),
            mutation_store,
            PreferenceSignals {
                preferences,
                theme_signal: None,
                lang_signal: None,
                cache_versions,
                preference_notice,
                bootstrap_epoch: Rc::new(Cell::new(0_u64)),
            },
        );

        ListenerTestHarness {
            _vdom: vdom,
            _guard: guard,
            cache,
            connection_status,
            db,
            preference_service,
        }
    }

    fn listener_ctx<'a>(harness: &'a ListenerTestHarness) -> ListenerContext<'a> {
        ListenerContext {
            db: harness.db.as_ref(),
            auth_header: "Bearer test-token",
            preference_service: harness.preference_service.clone(),
        }
    }

    #[tokio::test]
    async fn listen_for_events_sets_error_status_and_long_backoff_for_unauthorized() {
        let harness = listener_test_harness().await;
        let mut stream = FakeStream {
            events: VecDeque::from([Err(SseError::Unauthorized)]),
        };
        let mut cache = harness.cache;
        let mut connection_status = harness.connection_status;
        let listener_ctx = ListenerContext {
            db: harness.db.as_ref(),
            auth_header: "Bearer test-token",
            preference_service: harness.preference_service.clone(),
        };

        let delay = listen_for_events(
            &mut stream,
            &mut cache,
            listener_ctx,
            &mut connection_status,
            &SessionId::new(),
        )
        .await;

        assert_eq!(delay, 5000);
        assert_eq!(connection_status.read().clone(), ConnectionStatus::Error);
    }

    #[tokio::test]
    async fn listen_for_events_sets_error_status_and_short_backoff_for_timeout() {
        let harness = listener_test_harness().await;
        let mut stream = FakeStream {
            events: VecDeque::from([Err(SseError::Timeout)]),
        };
        let mut cache = harness.cache;
        let mut connection_status = harness.connection_status;
        let listener_ctx = listener_ctx(&harness);

        let delay = listen_for_events(
            &mut stream,
            &mut cache,
            listener_ctx,
            &mut connection_status,
            &SessionId::new(),
        )
        .await;

        assert_eq!(delay, 1000);
        assert_eq!(connection_status.read().clone(), ConnectionStatus::Error);
    }

    #[tokio::test]
    async fn listen_for_events_sets_error_status_and_short_backoff_for_stream_error() {
        let harness = listener_test_harness().await;
        let mut stream = FakeStream {
            events: VecDeque::from([Err(SseError::Stream("reset".to_string()))]),
        };
        let mut cache = harness.cache;
        let mut connection_status = harness.connection_status;
        let listener_ctx = listener_ctx(&harness);

        let delay = listen_for_events(
            &mut stream,
            &mut cache,
            listener_ctx,
            &mut connection_status,
            &SessionId::new(),
        )
        .await;

        assert_eq!(delay, 1000);
        assert_eq!(connection_status.read().clone(), ConnectionStatus::Error);
    }

    #[tokio::test]
    async fn listen_for_events_sets_disconnected_status_when_stream_ends() {
        let harness = listener_test_harness().await;
        let mut stream = FakeStream {
            events: VecDeque::new(),
        };
        let mut cache = harness.cache;
        let mut connection_status = harness.connection_status;
        let listener_ctx = ListenerContext {
            db: harness.db.as_ref(),
            auth_header: "Bearer test-token",
            preference_service: harness.preference_service.clone(),
        };

        let delay = listen_for_events(
            &mut stream,
            &mut cache,
            listener_ctx,
            &mut connection_status,
            &SessionId::new(),
        )
        .await;

        assert_eq!(delay, 1000);
        assert_eq!(
            connection_status.read().clone(),
            ConnectionStatus::Disconnected
        );
    }

    #[tokio::test]
    async fn handle_invalidation_event_updates_template_version_and_marks_fresh_after_refetch() {
        let harness = listener_test_harness().await;
        let mut cache = harness.cache;
        let event = shared::frontend::sse::InvalidationEvent {
            entity: EntityType::Template.as_str().to_string(),
            version: 7,
            user_id: Uuid::new_v4(),
        };

        handle_invalidation_event(&event, &mut cache, listener_ctx(&harness)).await;

        assert_eq!(cache.read().get_version(EntityType::Template), 7);
        assert!(
            !cache.read().is_stale(EntityType::Template),
            "template invalidation should be refetched through the no-network template path"
        );
    }

    #[tokio::test]
    async fn handle_invalidation_event_no_change_keeps_entity_fresh() {
        let harness = listener_test_harness().await;
        let mut cache = harness.cache;
        assert_eq!(
            cache.write().update_version(EntityType::Template, Some(3)),
            VersionUpdateResult::Updated
        );
        cache.write().mark_fresh(EntityType::Template);
        let event = shared::frontend::sse::InvalidationEvent {
            entity: EntityType::Template.as_str().to_string(),
            version: 3,
            user_id: Uuid::new_v4(),
        };

        handle_invalidation_event(&event, &mut cache, listener_ctx(&harness)).await;

        assert_eq!(cache.read().get_version(EntityType::Template), 3);
        assert!(!cache.read().is_stale(EntityType::Template));
    }

    #[tokio::test]
    async fn handle_invalidation_event_ignores_unknown_entity() {
        let harness = listener_test_harness().await;
        let mut cache = harness.cache;
        let event = shared::frontend::sse::InvalidationEvent {
            entity: "unknown_entity".to_string(),
            version: 99,
            user_id: Uuid::new_v4(),
        };

        handle_invalidation_event(&event, &mut cache, listener_ctx(&harness)).await;

        assert!(cache.read().get_versions().is_empty());
        assert!(cache.read().stale_entities().is_empty());
    }

    #[test]
    fn reconnect_delay_is_longer_for_unauthorized() {
        let unauthorized = reconnect_plan(Some(&SseError::Unauthorized));
        assert_eq!(unauthorized.delay_ms, 5000);
        assert_eq!(unauthorized.connection_status, ConnectionStatus::Error);
        assert_eq!(unauthorized.event_type, SseEventType::Error);
        assert_eq!(unauthorized.metric_reason, "unauthorized");
        assert_eq!(
            unauthorized.log_message,
            "Unauthorized, token may be expired"
        );
        let mut max_non_auth_delay = 0;
        for error in [
            SseError::Timeout,
            SseError::Connection("boom".into()),
            SseError::Stream("boom".into()),
        ] {
            let delay = reconnect_plan(Some(&error)).delay_ms;
            assert!(
                unauthorized.delay_ms > delay,
                "unauthorized reconnect delay should dominate {error:?}"
            );
            max_non_auth_delay = max_non_auth_delay.max(delay);
        }
        assert_eq!(max_non_auth_delay, 1000);
    }

    #[test]
    fn reconnect_delay_defaults_to_short_backoff() {
        let timeout = reconnect_plan(Some(&SseError::Timeout));
        assert_eq!(timeout.delay_ms, 1000);
        assert_eq!(timeout.connection_status, ConnectionStatus::Error);
        assert_eq!(timeout.event_type, SseEventType::Timeout);
        assert_eq!(timeout.metric_reason, "timeout");
        assert_eq!(timeout.log_message, "Listener timeout");

        let cases = [
            (
                reconnect_plan(Some(&SseError::Connection("refused".into()))),
                ConnectionStatus::Error,
                SseEventType::Error,
                "error",
            ),
            (
                reconnect_plan(Some(&SseError::Stream("reset".into()))),
                ConnectionStatus::Error,
                SseEventType::Error,
                "error",
            ),
            (
                reconnect_plan(None),
                ConnectionStatus::Disconnected,
                SseEventType::Close,
                "network",
            ),
        ];

        for (plan, expected_status, expected_event, expected_reason) in cases {
            assert_eq!(
                plan.delay_ms, 1000,
                "non-unauthorized errors should use the default short backoff"
            );
            assert_eq!(plan.connection_status, expected_status);
            assert_eq!(plan.event_type, expected_event);
            assert_eq!(plan.metric_reason, expected_reason);
            assert!(
                plan.delay_ms < reconnect_plan(Some(&SseError::Unauthorized)).delay_ms,
                "default reconnect backoff should stay below unauthorized backoff"
            );
        }
    }
}

//! The second domain server of the D4d multi-domain trial.
//!
//! An axum service over sqlx/SQLite speaking the same batch sync protocol `todo-server` does, for
//! a different domain. A library as well as a binary, so the trial's observations can spawn it
//! in-process and drive it over real HTTP.
//!
//! # Why a second server exists at all
//!
//! Two guarantees this project has already paid for are unprovable with one
//! (`wiki/plans/d4d-multi-domain-trial.plan.md`):
//!
//! - **Cross-service enqueue order.** Decision 036 refused to partition the outbox and paid for
//!   that refusal with head-of-line blocking, explicitly to keep enqueue order across types. Every
//!   test written before D4d sends to one server, so the guarantee's only evidence was that nothing
//!   had contradicted it.
//! - **More than one invalidation source.** Decision 038 makes multiple origins a criterion for
//!   promoting the inbound seam into core, because per-source reconnect semantics are where the
//!   design is load-bearing. One server cannot exercise them.
//!
//! # What it deliberately does not do
//!
//! It depends on neither `frontbox` nor `todo-server`. See `examples/user-server/Cargo.toml`.
#![deny(missing_docs)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use tower_http::cors::CorsLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub mod domain;
pub mod reads;
pub mod todos;
pub mod wire;

use domain::{apply, cascaded_user, Applied};
use reads::{list, one, versions};
use todos::TodoDirectory;
use wire::{BatchRequest, BatchResponse, MutationResult, Rejection, Status, User, Versions};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "frontbox user trial API",
        version = "0.0.0",
        description = "The second domain server of the D4d multi-domain trial."
    ),
    paths(sync, reads::list, reads::one, reads::versions),
    components(schemas(
        BatchRequest,
        BatchResponse,
        MutationResult,
        Rejection,
        Status,
        User,
        Versions,
        wire::Mutation
    )),
    tags(
        (name = "sync", description = "Batch replay protocol endpoints"),
        (name = "users", description = "User read endpoints"),
        (name = "invalidation", description = "Opaque entity versions")
    )
)]
struct ApiDoc;

/// Everything a handler needs.
#[derive(Clone)]
pub struct AppState {
    pub(crate) pool: SqlitePool,
    /// When set, every mutation comes back `Pending` and nothing is applied.
    hold: Arc<AtomicBool>,
    /// When set, results are omitted entirely rather than returned.
    silent: Arc<AtomicBool>,
    /// The other service, for the delete cascade. See `examples/user-server/src/todos.rs`.
    todos: TodoDirectory,
    /// When set, every *read* answers 503.
    ///
    /// Distinct from `hold` and `silent`, which are about the write path. This one exists for one
    /// observation and could not be reached without it: an invalidation source drops when its
    /// service stops answering `GET /api/v1/versions`, and a trial that can only switch off the
    /// whole network cannot tell "this source dropped" from "everything dropped" — which is the
    /// exact distinction per-source reconnect semantics turn on.
    ///
    /// **Enforced in exactly one place**, `reads::refuse_if_down`, and that is not tidiness. It used
    /// to be a line inside the helper that two of the three reads happen to share, so the third —
    /// `GET /api/v1/users/{id}`, the only one another *service* calls — kept answering while this
    /// sentence claimed otherwise. A switch documented as covering everything has to be reachable
    /// from every endpoint by reading one call.
    pub(crate) down: Arc<AtomicBool>,
}

impl AppState {
    /// Open an in-memory database and create the schema.
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn in_memory() -> Result<Self, sqlx::Error> {
        // One connection, because `:memory:` gives each connection its own private database.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect("sqlite::memory:")
            .await?;
        Self::from_pool(pool).await
    }

    /// Create the schema on an existing pool.
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn from_pool(pool: SqlitePool) -> Result<Self, sqlx::Error> {
        sqlx::query("CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
            .execute(&pool)
            .await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS applied_mutations (mutation_id TEXT PRIMARY KEY)")
            .execute(&pool)
            .await?;
        Ok(Self {
            pool,
            todos: TodoDirectory::new(None),
            hold: Arc::new(AtomicBool::new(false)),
            silent: Arc::new(AtomicBool::new(false)),
            down: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Point this server at a todo server, switching the delete cascade on.
    ///
    /// Off by default. With no todo service there is nothing to cascade to, and a delete then
    /// removes the user alone — which is correct for a deployment that has no todos.
    #[must_use]
    pub fn with_todo_server(mut self, base_url: impl Into<String>) -> Self {
        self.todos = TodoDirectory::new(Some(base_url.into()));
        self
    }

    /// Answer `Pending` to everything, applying nothing.
    pub fn hold(&self, holding: bool) {
        self.hold.store(holding, Ordering::SeqCst);
    }

    /// Return no verdicts at all, applying nothing.
    pub fn silent(&self, silent: bool) {
        self.silent.store(silent, Ordering::SeqCst);
    }

    /// Answer 503 to every read, as a service that has fallen over does.
    pub fn down(&self, down: bool) {
        self.down.store(down, Ordering::SeqCst);
    }

    /// How many mutation ids this server has recorded.
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn applied_count(&self) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM applied_mutations")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }
}

/// Build the router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/sync", post(sync))
        .route("/api/v1/users", get(list))
        // The single-user read exists for one caller: `todo-server`'s reference check. It is a
        // fixture rather than an architecture recommendation — see that server's own note.
        .route("/api/v1/users/{id}", get(one))
        .route("/api/v1/versions", get(versions))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// The batch endpoint.
///
/// One transaction per mutation, so one refusal cannot roll back its neighbours.
#[utoipa::path(
    post,
    path = "/api/v1/sync",
    tag = "sync",
    request_body = BatchRequest,
    responses(
        (status = 200, description = "Per-mutation verdicts.", body = BatchResponse),
        (
            status = 503,
            description = "A user delete could not empty that user's todos, so nothing was applied. \
                           Transient: the client should retain and retry.",
            body = Rejection
        )
    )
)]
async fn sync(
    State(state): State<AppState>,
    Json(request): Json<BatchRequest>,
) -> Result<Json<BatchResponse>, (StatusCode, Json<Rejection>)> {
    if state.silent.load(Ordering::SeqCst) {
        return Ok(Json(BatchResponse {
            results: Vec::new(),
        }));
    }
    if state.hold.load(Ordering::SeqCst) {
        return Ok(Json(BatchResponse {
            results: request
                .mutations
                .iter()
                .map(|m| MutationResult::new(&m.mutation_id, Status::Pending))
                .collect(),
        }));
    }

    // The cascade, before anything is applied and before any transaction is open.
    //
    // **Order is the whole point.** The todos go first; the user record goes only if they all did.
    // A cascade that failed halfway and deleted the user anyway would leave exactly the orphans
    // this exists to prevent, so a failure refuses the *batch* — nothing is applied, the client
    // retains, and the deterministic mutation ids make the retry free for everything already
    // deleted (`examples/user-server/src/todos.rs`).
    if state.todos.is_enabled() {
        for mutation in &request.mutations {
            let Some(user_id) = cascaded_user(mutation) else {
                continue;
            };
            if let Err(why) = state.todos.cascade(&user_id, &mutation.mutation_id).await {
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(
                        Rejection::new("cascade_failed", why.0)
                            .with_details(serde_json::json!({ "user_id": user_id })),
                    ),
                ));
            }
        }
    }

    let mut results = Vec::with_capacity(request.mutations.len());
    for mutation in &request.mutations {
        let outcome = match state.pool.begin().await {
            Ok(mut tx) => match apply(&mut tx, mutation).await {
                Ok(applied) => match tx.commit().await {
                    Ok(()) => applied,
                    // A commit failure is not a refusal. Omitting the verdict leaves the record
                    // queued, which is the honest answer: nothing is known to have happened.
                    Err(_) => continue,
                },
                Err(_) => {
                    let _ = tx.rollback().await;
                    continue;
                }
            },
            Err(_) => continue,
        };

        results.push(match outcome {
            Applied::Ok => MutationResult::new(&mutation.mutation_id, Status::Applied),
            Applied::AlreadySeen => MutationResult::new(&mutation.mutation_id, Status::Duplicate),
            Applied::Refused(error) => MutationResult::rejected(&mutation.mutation_id, error),
        });
    }

    Ok(Json(BatchResponse { results }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_documents_the_trial_routes() {
        let spec = ApiDoc::openapi().to_pretty_json().unwrap();

        assert!(spec.contains(r#""/api/v1/sync""#));
        assert!(spec.contains(r#""/api/v1/users""#));
        assert!(spec.contains(r#""/api/v1/users/{id}""#));
        assert!(spec.contains(r#""/api/v1/versions""#));
    }
}

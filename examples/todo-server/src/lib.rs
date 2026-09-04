//! The server half of the D4a offline todo trial.
//!
//! An axum service over sqlx/SQLite speaking the batch sync protocol frontbox already fixed
//! (`wiki/decisions/010-batch-wire-format.decision.md`). It is a library as well as a binary so the
//! trial's observations can spawn it in-process and drive it over real HTTP.
//!
//! # What it deliberately does not do
//!
//! It does not depend on `frontbox`. Sharing the client's types would make the two ends unable to
//! disagree, and the whole point of the trial is that they might. See `examples/todo-server/src/wire.rs`.
#![deny(missing_docs)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use tower_http::cors::CorsLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub mod direct;
pub mod domain;
pub mod reads;
pub mod users;
pub mod version;
pub mod wire;

use direct::{direct_error, remove, set_done};
use domain::{apply, referenced_user, Applied};
use reads::{list, versions};
use users::{Missing, UserDirectory};
use wire::{BatchRequest, BatchResponse, MutationResult, Rejection, Status, Todo, Versions};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "frontbox todo trial API",
        version = "0.0.0",
        description = "The hand-written server half of the D4a offline todo trial."
    ),
    paths(sync, reads::list, direct::set_done, direct::remove, reads::versions),
    components(schemas(
        BatchRequest,
        BatchResponse,
        DirectSetDoneBody,
        MutationResult,
        Rejection,
        Status,
        Todo,
        Versions,
        wire::Mutation
    )),
    tags(
        (name = "sync", description = "Batch replay protocol endpoints"),
        (name = "todos", description = "Todo read and direct-dispatch endpoints"),
        (name = "invalidation", description = "Opaque entity versions")
    )
)]
struct ApiDoc;

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct DirectSetDoneBody {
    done: bool,
}

/// Everything a handler needs.
#[derive(Clone)]
pub struct AppState {
    pub(crate) pool: SqlitePool,
    /// When set, every mutation comes back `Pending` and nothing is applied.
    ///
    /// This is not a debug hack; it is the only way to reach two of the trial's observations. A
    /// server that never says `Pending` cannot demonstrate a wedged head, and a wedged head is what
    /// `wiki/decisions/017-bounded-retention.decision.md` exists for.
    hold: Arc<AtomicBool>,
    /// When set, results are omitted entirely rather than returned.
    ///
    /// A server declining to rule is different from one saying `Pending`, and the client treats
    /// them differently: silence produces no anomaly and no verdict at all.
    silent: Arc<AtomicBool>,
    /// The other service, when D4d wired one up. See `examples/todo-server/src/users.rs`.
    users: UserDirectory,
    /// When set, every *read* answers 503.
    ///
    /// Distinct from `hold` and `silent`, which are about the write path. This one exists so a test
    /// can fail the user service's delete cascade at its first step — the `GET` that asks what to
    /// delete — and watch the user survive. Without it there is no way to tell a cascade that was
    /// refused from one that was never attempted.
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
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS todos \
             (id TEXT PRIMARY KEY, title TEXT NOT NULL, done INTEGER NOT NULL, \
              user_id TEXT NOT NULL)",
        )
        .execute(&pool)
        .await?;
        // `mutation_id` is the primary key rather than a plain column, so a double insert is a
        // database error rather than a silent second row.
        sqlx::query("CREATE TABLE IF NOT EXISTS applied_mutations (mutation_id TEXT PRIMARY KEY)")
            .execute(&pool)
            .await?;
        Ok(Self {
            pool,
            hold: Arc::new(AtomicBool::new(false)),
            silent: Arc::new(AtomicBool::new(false)),
            users: UserDirectory::new(None),
            down: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Point this server at a user server, switching the reference check on.
    ///
    /// Off by default, which is what keeps every trial written before D4d working unchanged: those
    /// have one server and no user domain, so there is nothing to check against.
    #[must_use]
    pub fn with_user_server(mut self, base_url: impl Into<String>) -> Self {
        self.users = UserDirectory::new(Some(base_url.into()));
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
        .route("/api/v1/todos", get(list))
        .route("/api/v1/versions", get(versions))
        // The direct-dispatch endpoint. It exists so the trial can test the pattern
        // `wiki/proposals/extraction-boundary.proposal.md` keeps out of core: try the write
        // now, and queue it under the same id if that does not settle.
        .route("/api/v1/todos/{id}/done", put(set_done))
        // The cascade's other half. `user-server` calls this once per todo when a user is deleted;
        // see `examples/user-server/src/todos.rs` for why the cascade lives there and what it
        // costs. Routed through `domain::apply` like every other write, so the idempotency table
        // covers it and a retried cascade is a no-op.
        .route("/api/v1/todos/{id}", delete(remove))
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
        (
            status = 200,
            description = "Per-mutation verdicts for every mutation the server ruled on.",
            body = BatchResponse
        ),
        (
            status = 404,
            description = "A mutation references a user record this server cannot find. \
                           Transient: the client should retain and retry, not dead-letter.",
            body = Rejection
        ),
        (status = 503, description = "The user server could not be reached.", body = Rejection)
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

    // The cross-service reference check, before anything is applied and before any transaction is
    // open. See `examples/todo-server/src/users.rs` for why this is a fixture.
    //
    // **The whole batch is refused, not the offending mutation.** That looks coarse and is not:
    // under decision 036 there is one queue and one `seq`, so a mutation whose prerequisite has
    // not landed is at or near the head and everything behind it is behind it anyway. Answering
    // per-mutation would also mean answering `Blocked` here, which would move the classification
    // this trial deliberately leaves to the transport
    // (`wiki/decisions/019-verdict-synthesis.decision.md`).
    if state.users.is_enabled() {
        for mutation in &request.mutations {
            let Some(user_id) = referenced_user(mutation) else {
                continue;
            };
            match state.users.require(&user_id).await {
                Ok(()) => {}
                // 404 for "a referenced record is not here". The client's transport turns this
                // into `MutationStatus::Blocked` — retained and retried, never dead-lettered,
                // because a missing prerequisite is transient by construction.
                Err(Missing::NoSuchUser) => {
                    return Err(direct_error(
                        StatusCode::NOT_FOUND,
                        Rejection::new("unknown_user", "no such user record")
                            .with_details(serde_json::json!({ "user_id": user_id })),
                    ))
                }
                // Distinct from the above on purpose. A user-server outage is this server's
                // problem, and reporting it as a missing user would present an outage to the
                // client as a durable ordering failure.
                Err(Missing::Unavailable(why)) => {
                    return Err(direct_error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        Rejection::new("user_server_unavailable", why),
                    ))
                }
            }
        }
    }

    let mut results = Vec::with_capacity(request.mutations.len());
    for mutation in &request.mutations {
        // Per-mutation transactions. The client's `apply_outcomes` is atomic across the whole
        // batch, but that is a statement about the *client's* stores; there is no reason a server
        // refusal should roll back a neighbour it has nothing to do with. This is also why
        // `Blocked` never appears here — nothing is skipped behind an earlier failure.
        let outcome = match state.pool.begin().await {
            Ok(mut tx) => match apply(&mut tx, mutation).await {
                Ok(applied) => match tx.commit().await {
                    Ok(()) => applied,
                    Err(_) => {
                        // A commit failure is not a refusal. Omitting the verdict leaves the record
                        // queued, which is the honest answer: nothing is known to have happened.
                        continue;
                    }
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
        assert!(spec.contains(r#""/api/v1/todos""#));
        assert!(spec.contains(r#""/api/v1/todos/{id}/done""#));
        assert!(spec.contains(r#""/api/v1/versions""#));
        assert!(spec.contains(r#""/api/v1/todos/{id}""#));
        assert!(spec.contains(r#""x-mutation-id""#));
    }
}

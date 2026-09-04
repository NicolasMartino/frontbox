//! The two endpoints a caller writes to one at a time, rather than through the batch.
//!
//! Split out of `lib.rs` when the cascade's `DELETE` pushed it past the four-hundred-line cap
//! `AGENTS.md` sets and `scripts/verify.sh` measures. The boundary is real: everything here takes
//! an idempotency key in a header and applies exactly one mutation, where `sync` takes a batch in a
//! body.
//!
//! **Both go through `domain::apply`**, which is the point rather than an implementation detail. A
//! direct write and its queued replay are one mutation to this server because they share a
//! `mutation_id` and the same `applied_mutations` table — without that, "direct dispatch with
//! fallback" would be a way to write twice.

use std::sync::atomic::Ordering;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;

use crate::domain::{apply, Applied};
use crate::wire::{self, Rejection};
use crate::{AppState, DirectSetDoneBody};

/// The direct write. Same idempotency table as the batch path, same transaction.
///
/// The client sends its `MutationId` in a header so a direct write and its queued replay are one
/// mutation to this server. Without that, "direct dispatch with fallback" would be a way to write
/// twice.
#[utoipa::path(
    put,
    path = "/api/v1/todos/{id}/done",
    tag = "todos",
    request_body = DirectSetDoneBody,
    params(
        ("id" = String, Path, description = "Todo row id."),
        ("x-mutation-id" = String, Header, description = "Idempotency key shared with queued replay.")
    ),
    responses(
        (status = 204, description = "Todo was updated, or the mutation id was already applied."),
        (status = 400, description = "The idempotency header is missing or invalid.", body = Rejection),
        (status = 422, description = "The server terminally refused the requested todo update.", body = Rejection),
        (status = 500, description = "Database failure.", body = Rejection),
        (status = 503, description = "Server is holding or declining to rule on writes.", body = Rejection)
    )
)]
pub(crate) async fn set_done(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode, (StatusCode, Json<Rejection>)> {
    if state.hold.load(Ordering::SeqCst) || state.silent.load(Ordering::SeqCst) {
        // Held or silent: nothing is decided. A 503 is retryable, which is what the client's
        // classifier has to work out for itself — see `examples/todo-core/src/app/mod.rs`.
        return Err(direct_error(
            StatusCode::SERVICE_UNAVAILABLE,
            Rejection::new("holding", "this server is holding every write"),
        ));
    }
    let Some(mutation_id) = headers
        .get("x-mutation-id")
        .and_then(|value| value.to_str().ok())
    else {
        // Terminal: a direct write with no idempotency key cannot be made safe by retrying it.
        return Err(direct_error(
            StatusCode::BAD_REQUEST,
            Rejection::new("invalid_input", "x-mutation-id is required")
                .with_details(serde_json::json!({ "header": "x-mutation-id" })),
        ));
    };

    let mutation = wire::Mutation {
        mutation_id: mutation_id.to_owned(),
        method: "PUT".to_owned(),
        path: format!("/api/v1/todos/{id}/done"),
        client_datetime: String::new(),
        body,
    };

    let mut tx = state.pool.begin().await.map_err(server_error)?;
    let outcome = apply(&mut tx, &mutation).await.map_err(server_error)?;
    tx.commit().await.map_err(server_error)?;

    match outcome {
        // `Duplicate` is a success for a direct write: the effect the caller asked for is in place.
        Applied::Ok | Applied::AlreadySeen => Ok(StatusCode::NO_CONTENT),
        Applied::Refused(error) => Err(direct_error(StatusCode::UNPROCESSABLE_ENTITY, error)),
    }
}

/// Delete one todo directly, for the user service's cascade.
///
/// Same idempotency table and same transaction as every other write. `DELETE` on a row that is
/// already gone succeeds, which is deliberate and is what makes a retried cascade safe: the effect
/// the caller asked for is in place either way (`examples/todo-server/src/domain.rs`).
#[utoipa::path(
    delete,
    path = "/api/v1/todos/{id}",
    tag = "todos",
    params(
        ("id" = String, Path, description = "Todo row id."),
        ("x-mutation-id" = String, Header, description = "Idempotency key.")
    ),
    responses(
        (status = 204, description = "Todo is gone, or the mutation id was already applied."),
        (status = 400, description = "The idempotency header is missing.", body = Rejection),
        (status = 500, description = "Database failure.", body = Rejection)
    )
)]
pub(crate) async fn remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, (StatusCode, Json<Rejection>)> {
    let Some(mutation_id) = headers
        .get("x-mutation-id")
        .and_then(|value| value.to_str().ok())
    else {
        return Err(direct_error(
            StatusCode::BAD_REQUEST,
            Rejection::new("invalid_input", "x-mutation-id is required")
                .with_details(serde_json::json!({ "header": "x-mutation-id" })),
        ));
    };

    let mutation = wire::Mutation {
        mutation_id: mutation_id.to_owned(),
        method: "DELETE".to_owned(),
        path: format!("/api/v1/todos/{id}"),
        client_datetime: String::new(),
        body: serde_json::Value::Null,
    };

    let mut tx = state.pool.begin().await.map_err(server_error)?;
    let outcome = apply(&mut tx, &mutation).await.map_err(server_error)?;
    tx.commit().await.map_err(server_error)?;

    match outcome {
        Applied::Ok | Applied::AlreadySeen => Ok(StatusCode::NO_CONTENT),
        Applied::Refused(error) => Err(direct_error(StatusCode::UNPROCESSABLE_ENTITY, error)),
    }
}

/// Answer a direct write with a refusal the client can parse.
///
/// `Json` rather than a `String` body, so the response carries `application/json` and says what it
/// is. The batch endpoint's refusals travel inside `MutationResult`; this is the same payload for
/// the one endpoint that answers a single write.
pub(crate) fn direct_error(status: StatusCode, error: Rejection) -> (StatusCode, Json<Rejection>) {
    (status, Json(error))
}

fn server_error(error: sqlx::Error) -> (StatusCode, Json<Rejection>) {
    direct_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        Rejection::new("server_error", error.to_string()),
    )
}

//! What a client reads, and the version that tells it when to read again.
//!
//! Split out of `lib.rs` when D4d pushed it past the four-hundred-line cap `AGENTS.md` sets and
//! `scripts/verify.sh` measures. The line is a real boundary rather than a convenient one: nothing
//! here writes, and the version endpoint exists only to describe what these reads would return.
//!
//! # Why a read model has a version at all
//!
//! An invalidation source polls `GET /api/v1/versions` and compares what it gets to what it saw
//! last (`wiki/proposals/invalidation-delivery.proposal.md`). One call answers for every entity
//! this server owns, which is why the client's contract is a staleness *budget* per entity rather
//! than a poll schedule per entity: a literal per-entity schedule is not satisfiable against a
//! single endpoint, and a budget is — by polling at the tightest budget among a source's entities
//! and over-delivering for the rest.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::version::version_of;
use crate::wire::{Todo, Versions};
use crate::AppState;

/// Narrows the read to one owner.
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
pub(crate) struct TodoFilter {
    /// Return only todos belonging to this user record.
    user_id: Option<String>,
}

/// The read endpoint.
///
/// `?user_id=` exists for one caller and it is worth naming: `user-server` reads it to find what a
/// user delete has to cascade. Filtered in SQL rather than after the fact, so a large table does
/// not travel across the service boundary to be thrown away at the other end.
#[utoipa::path(
    get,
    path = "/api/v1/todos",
    tag = "todos",
    params(TodoFilter),
    responses(
        (status = 200, description = "Current todo read model.", body = [Todo]),
        (status = 500, description = "Database failure."),
        (status = 503, description = "This service is down.")
    )
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    Query(filter): Query<TodoFilter>,
) -> Result<Json<Vec<Todo>>, StatusCode> {
    Ok(Json(read_todos(&state, filter.user_id.as_deref()).await?))
}

/// The opaque version of every entity this server owns.
#[utoipa::path(
    get,
    path = "/api/v1/versions",
    tag = "invalidation",
    responses(
        (status = 200, description = "Opaque per-entity versions.", body = Versions),
        (status = 500, description = "Database failure."),
        (status = 503, description = "This service is down.")
    )
)]
pub(crate) async fn versions(State(state): State<AppState>) -> Result<Json<Versions>, StatusCode> {
    let todos = read_todos(&state, None).await?;
    // Every field, because every field is something a client renders. A version over ids alone
    // would not change when a title did, and the cache would never learn about a rename.
    let mut fields: Vec<String> = Vec::with_capacity(todos.len() * 4);
    for todo in &todos {
        fields.push(todo.id.clone());
        fields.push(todo.title.clone());
        fields.push(u8::from(todo.done).to_string());
        fields.push(todo.user_id.clone());
    }
    Ok(Json(Versions {
        todo: version_of(fields.iter().map(String::as_str)),
    }))
}

pub(crate) async fn read_todos(
    state: &AppState,
    user_id: Option<&str>,
) -> Result<Vec<Todo>, StatusCode> {
    if state.down.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    // Two statements rather than one with `(?1 IS NULL OR user_id = ?1)`. The clever form is a
    // filter SQLite cannot use the index for, and it reads as if the parameter were optional when
    // what is optional is the whole predicate.
    let rows: Vec<(String, String, i64, String)> = match user_id {
        Some(user_id) => {
            sqlx::query_as(
                "SELECT id, title, done, user_id FROM todos WHERE user_id = ? ORDER BY id",
            )
            .bind(user_id)
            .fetch_all(&state.pool)
            .await
        }
        None => {
            sqlx::query_as("SELECT id, title, done, user_id FROM todos ORDER BY id")
                .fetch_all(&state.pool)
                .await
        }
    }
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(rows
        .into_iter()
        .map(|(id, title, done, user_id)| Todo {
            id,
            title,
            done: done != 0,
            user_id,
        })
        .collect())
}

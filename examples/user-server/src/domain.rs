//! Applying one user mutation, and the idempotency rule that makes a resend safe.

use sqlx::{Sqlite, Transaction};

use crate::wire::{Mutation, Rejection};

/// What applying one mutation produced.
pub enum Applied {
    /// The domain effect happened and the id was recorded in the same transaction.
    Ok,
    /// This id was already recorded. Nothing was done and nothing needs to be.
    AlreadySeen,
    /// Terminally refused. The client should dead-letter it, not resend it.
    Refused(Rejection),
}

/// Apply one mutation inside a caller-supplied transaction.
///
/// # The rule that makes `mutation_id` mean something
///
/// The id is inserted **in the same transaction as the domain effect**, for the reason
/// `examples/todo-server/src/domain.rs` states at length: a separate insert makes the key
/// decorative, because a crash between two commits leaves an applied write with no record that it
/// was applied and the client's resend duplicates it.
///
/// It is repeated here rather than shared because these two servers are two independent readings
/// of one protocol — see `examples/user-server/src/wire.rs`. A rule that only one of them
/// implements is a rule the trial can catch.
pub async fn apply(
    tx: &mut Transaction<'_, Sqlite>,
    mutation: &Mutation,
) -> Result<Applied, sqlx::Error> {
    let seen: Option<(String,)> =
        sqlx::query_as("SELECT mutation_id FROM applied_mutations WHERE mutation_id = ?")
            .bind(&mutation.mutation_id)
            .fetch_optional(&mut **tx)
            .await?;
    if seen.is_some() {
        return Ok(Applied::AlreadySeen);
    }

    let effect = match route(mutation) {
        Ok(effect) => effect,
        Err(refusal) => return Ok(Applied::Refused(refusal)),
    };

    match effect {
        Effect::Create { id, name } => {
            let already_exists: Option<(String,)> =
                sqlx::query_as("SELECT id FROM users WHERE id = ?")
                    .bind(&id)
                    .fetch_optional(&mut **tx)
                    .await?;
            if already_exists.is_some() {
                return Ok(Applied::Refused(
                    Rejection::new("already_exists", "user already exists")
                        .with_details(serde_json::json!({ "id": id })),
                ));
            }

            sqlx::query("INSERT INTO users (id, name) VALUES (?, ?)")
                .bind(&id)
                .bind(&name)
                .execute(&mut **tx)
                .await?;
        }
        Effect::Rename { id, name } => {
            let result = sqlx::query("UPDATE users SET name = ? WHERE id = ?")
                .bind(&name)
                .bind(&id)
                .execute(&mut **tx)
                .await?;
            if result.rows_affected() == 0 {
                return Ok(Applied::Refused(
                    Rejection::new("not_found", "user not found")
                        .with_details(serde_json::json!({ "id": id })),
                ));
            }
        }
        // Deliberately *not* a `not_found` when the row is already gone, where rename is. A delete
        // names no state to lose, so a replay against an absent row achieved what the caller asked
        // for. Two devices deleting one user offline is the ordinary case, and refusing the second
        // would dead-letter a write that did exactly what its user wanted — the same rule
        // `examples/todo-server/src/domain.rs` applies to a todo.
        Effect::Delete { id } => {
            sqlx::query("DELETE FROM users WHERE id = ?")
                .bind(&id)
                .execute(&mut **tx)
                .await?;
        }
    }

    sqlx::query("INSERT INTO applied_mutations (mutation_id) VALUES (?)")
        .bind(&mutation.mutation_id)
        .execute(&mut **tx)
        .await?;

    Ok(Applied::Ok)
}

/// The three effects this server has.
///
/// **`Delete` was deliberately absent until 2026-09-01**, and the argument for leaving it out was
/// sound as far as it went: deleting a user that todos on the other server reference leaves orphans
/// that nothing detects, because the todo server checks the reference when a todo is *written* and
/// never again.
///
/// What it was waiting for is a cascade — the todos go before the user does, and the caller learns
/// nothing happened if they cannot. That is `examples/user-server/src/todos.rs`, which also names
/// what it costs: the two services now call each other.
enum Effect {
    Create { id: String, name: String },
    Rename { id: String, name: String },
    Delete { id: String },
}

/// Which user record, if any, this mutation must empty of todos before it can be applied.
///
/// The mirror of `todo-server`'s `referenced_user`, and read for the same reason: the cascade is a
/// series of HTTP calls, and a database transaction held across a network round trip is a bad habit
/// to demonstrate even in a fixture. So the handler cascades first and opens its transaction after.
///
/// Only a delete cascades. A create or a rename touches no todo.
pub fn cascaded_user(mutation: &Mutation) -> Option<String> {
    match route(mutation) {
        // A mutation that will not route has nothing to cascade: `apply` refuses it, and refusing
        // it for the *routing* reason is more useful than failing a cascade for a path this server
        // does not have.
        Ok(Effect::Delete { id }) => Some(id),
        _ => None,
    }
}

/// Turn a replayed HTTP envelope back into a domain effect.
///
/// By-hand routing, for the reason `todo-server` gives: the envelope is method plus path plus a
/// JSON body, which is what makes the client's queue domain-neutral, and the cost is that the
/// routing a framework normally supplies happens here instead.
fn route(mutation: &Mutation) -> Result<Effect, Rejection> {
    let segments: Vec<&str> = mutation.path.trim_matches('/').split('/').collect();
    let name = || -> Result<String, Rejection> {
        let raw = mutation
            .body
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned();
        if raw.is_empty() {
            return Err(Rejection::new("invalid_input", "name must not be empty")
                .with_details(serde_json::json!({ "field": "name" })));
        }
        Ok(raw)
    };

    match (mutation.method.as_str(), segments.as_slice()) {
        ("POST", ["api", "v1", "users"]) => {
            let id = mutation
                .body
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| Rejection::new("invalid_input", "body.id is required"))?
                .to_owned();
            Ok(Effect::Create { id, name: name()? })
        }
        ("PUT", ["api", "v1", "users", id]) => Ok(Effect::Rename {
            id: (*id).to_owned(),
            name: name()?,
        }),
        ("DELETE", ["api", "v1", "users", id]) => Ok(Effect::Delete {
            id: (*id).to_owned(),
        }),
        _ => Err(Rejection::new(
            "unroutable",
            format!("no handler for {} {}", mutation.method, mutation.path),
        )),
    }
}

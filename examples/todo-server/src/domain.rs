//! Applying one mutation, and the idempotency rule that makes a resend safe.

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
/// The id is inserted **in the same transaction as the domain effect**. A separate insert would
/// make the key decorative: a crash between the two commits leaves an applied write with no record
/// that it was applied, and the client's resend duplicates it. That resend is not exotic — on the
/// web it is a page reload between the server committing and the client applying the verdict, which
/// is the window `apply_outcomes` cannot close because it is atomic on the client only.
pub async fn apply(
    tx: &mut Transaction<'_, Sqlite>,
    mutation: &Mutation,
) -> Result<Applied, sqlx::Error> {
    // The duplicate check reads inside the transaction, so two concurrent batches carrying the same
    // id cannot both pass it.
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
        Effect::Create { id, title, user_id } => {
            let already_exists: Option<(String,)> =
                sqlx::query_as("SELECT id FROM todos WHERE id = ?")
                    .bind(&id)
                    .fetch_optional(&mut **tx)
                    .await?;
            if already_exists.is_some() {
                return Ok(Applied::Refused(
                    Rejection::new("already_exists", "todo already exists")
                        .with_details(serde_json::json!({ "id": id })),
                ));
            }

            sqlx::query("INSERT INTO todos (id, title, done, user_id) VALUES (?, ?, 0, ?)")
                .bind(&id)
                .bind(&title)
                .bind(&user_id)
                .execute(&mut **tx)
                .await?;
        }
        Effect::Rename { id, title } => {
            let result = sqlx::query("UPDATE todos SET title = ? WHERE id = ?")
                .bind(&title)
                .bind(&id)
                .execute(&mut **tx)
                .await?;
            if result.rows_affected() == 0 {
                return Ok(Applied::Refused(not_found(&id)));
            }
        }
        Effect::SetDone { id, done } => {
            let result = sqlx::query("UPDATE todos SET done = ? WHERE id = ?")
                .bind(i64::from(done))
                .bind(&id)
                .execute(&mut **tx)
                .await?;
            if result.rows_affected() == 0 {
                return Ok(Applied::Refused(not_found(&id)));
            }
        }
        // Deliberately *not* a `not_found` when the row is already gone, where rename and toggle
        // are. A delete names no state to lose, so a replay against an absent row achieved what
        // the caller asked for; a rename carries a title that would be silently dropped. Two
        // devices deleting one todo offline is the ordinary case, and refusing the second would
        // dead-letter a write that did exactly what its user wanted.
        Effect::Delete { id } => {
            sqlx::query("DELETE FROM todos WHERE id = ?")
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

/// Which user record, if any, a mutation depends on already existing.
///
/// Read **before** the transaction opens rather than inside it, because confirming it is an HTTP
/// call to another service and a database transaction held across a network round trip is a bad
/// habit to demonstrate even in a fixture.
///
/// Only a create references a user. A rename or a toggle names a todo that, if it exists, was
/// already checked when it was created; re-checking would turn every write into a cross-service
/// call to learn something this server already knows.
pub fn referenced_user(mutation: &Mutation) -> Option<String> {
    match route(mutation) {
        // A mutation that will not route has no reference worth checking: `apply` refuses it, and
        // refusing it for the *routing* reason is more useful than refusing it for a missing user.
        Ok(Effect::Create { user_id, .. }) => Some(user_id),
        _ => None,
    }
}

/// The refusal for a write that names a row this server does not have.
///
/// Terminal, and correct for `PUT`: the title or flag it carries has nowhere to land, and replaying
/// it forever cannot change that. `DELETE` does not use it — see the arm above.
fn not_found(id: &str) -> Rejection {
    Rejection::new("not_found", "todo not found").with_details(serde_json::json!({ "id": id }))
}

enum Effect {
    Create {
        id: String,
        title: String,
        user_id: String,
    },
    Rename {
        id: String,
        title: String,
    },
    SetDone {
        id: String,
        done: bool,
    },
    Delete {
        id: String,
    },
}

/// Turn a replayed HTTP envelope back into a domain effect.
///
/// The envelope is `method` plus `path` plus a JSON body, which is what makes the client's queue
/// domain-neutral — and it means the routing a normal server gets from its framework has to happen
/// here by hand. That is the cost of the envelope, and it is worth seeing rather than hiding.
fn route(mutation: &Mutation) -> Result<Effect, Rejection> {
    let segments: Vec<&str> = mutation.path.trim_matches('/').split('/').collect();
    let title = || -> Result<String, Rejection> {
        let raw = mutation
            .body
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned();
        if raw.is_empty() {
            // The deliberate refusal path. Terminal, not retryable: resending the same empty title
            // produces the same answer forever, which is exactly what a dead letter is for.
            return Err(Rejection::new("invalid_input", "title must not be empty")
                .with_details(serde_json::json!({ "field": "title" })));
        }
        Ok(raw)
    };

    match (mutation.method.as_str(), segments.as_slice()) {
        ("POST", ["api", "v1", "todos"]) => {
            let id = mutation
                .body
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| Rejection::new("invalid_input", "body.id is required"))?
                .to_owned();
            let user_id = mutation
                .body
                .get("user_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    Rejection::new("invalid_input", "body.user_id is required")
                        .with_details(serde_json::json!({ "field": "user_id" }))
                })?
                .to_owned();
            Ok(Effect::Create {
                id,
                title: title()?,
                user_id,
            })
        }
        ("PUT", ["api", "v1", "todos", id]) => Ok(Effect::Rename {
            id: (*id).to_owned(),
            title: title()?,
        }),
        ("PUT", ["api", "v1", "todos", id, "done"]) => Ok(Effect::SetDone {
            id: (*id).to_owned(),
            done: mutation
                .body
                .get("done")
                .and_then(serde_json::Value::as_bool)
                .ok_or_else(|| {
                    Rejection::new("invalid_input", "body.done must be a boolean")
                        .with_details(serde_json::json!({ "field": "done" }))
                })?,
        }),
        ("DELETE", ["api", "v1", "todos", id]) => Ok(Effect::Delete {
            id: (*id).to_owned(),
        }),
        // An envelope this server cannot route is refused terminally rather than left pending.
        // Retrying it would produce the same answer, and the client's dead letter is where a
        // human can see what was sent.
        _ => Err(Rejection::new(
            "unroutable",
            format!("no handler for {} {}", mutation.method, mutation.path),
        )),
    }
}

//! The delete cascade: emptying a user's todos before the user goes.
//!
//! # This is a fixture, and it is the more questionable of the two
//!
//! `examples/todo-server/src/users.rs` already says a real service split would not make a
//! synchronous cross-service call to validate a foreign key. This one is worse in a specific way
//! and the file should say so rather than let a reader infer it: **the two services now call each
//! other.** `todo-server` asks `user-server` whether a user exists; `user-server` asks
//! `todo-server` what to delete. That is a dependency cycle, and a real system would break it —
//! with an event, an outbox on the server side, or by not having the reference at all.
//!
//! It is here because a user delete is the first flow in this trial that exercises decision 036's
//! ordering guarantee **destructively**, and doing the cascade anywhere else would not test that.
//! Compose cannot express the cycle either: `todo-server` already declares `depends_on:
//! user-server`, so the reverse would be circular and is simply absent — neither service calls the
//! other at *startup*, only per request.
//!
//! # `1 + N` calls, on purpose
//!
//! One `GET` for the list, then one `DELETE` each. A bulk `DELETE /api/v1/todos?user_id=X` would be
//! one call and is the shape a real system would want. The per-todo form is kept because this is a
//! demo whose subject is *order*: it puts one line per delete in the server log, so the cascade is
//! something you can watch rather than infer. The cost is stated here rather than discovered.
//!
//! # Idempotency without a uuid dependency
//!
//! Each delete carries `x-mutation-id: {user_delete_mutation_id}:todo:{todo_id}` — derived from the
//! mutation that caused it, so a cascade retried after a partial failure is a no-op on everything
//! it already did. `todo-server` records that id in the same transaction as the delete, exactly as
//! it does for a queued replay.

use serde::Deserialize;

/// Deletes a user's todos, by asking the todo service what they are.
///
/// Disabled when no base URL is configured, which is how a `user-server` run on its own keeps
/// working: with no todo service there is nothing to cascade to, and inventing an answer would be
/// worse than having none.
#[derive(Clone)]
pub struct TodoDirectory {
    client: reqwest::Client,
    base_url: Option<String>,
}

/// Why a cascade could not be completed.
///
/// One variant, deliberately. `todo-server`'s [`Missing`](crate::wire::Rejection) equivalent
/// separates "no such user" from "I could not ask", because those mean different things to a
/// client. Here they do not: a delete this service could not carry out is a delete it must not
/// report as done, whatever the reason — the alternative is a user record gone and its todos left
/// behind, which is precisely the orphan case the cascade exists to prevent.
#[derive(Debug)]
pub struct Unavailable(pub String);

/// Only the field the cascade needs.
///
/// Hand-written rather than shared with `todo-server`, for the reason
/// `examples/user-server/src/wire.rs` gives about the protocol: a second reading is the point.
/// `serde` ignores the fields this does not name, so the todo service can add columns freely.
#[derive(Deserialize)]
struct TodoRef {
    id: String,
}

impl TodoDirectory {
    /// A directory that cascades to `base_url`, or cascades nothing when it is `None`.
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
        }
    }

    /// Whether cascading is switched on at all.
    pub fn is_enabled(&self) -> bool {
        self.base_url.is_some()
    }

    /// Delete every todo belonging to `user_id`, and report how many.
    ///
    /// # Errors
    ///
    /// The todo service could not be reached, or refused a delete. Either way the caller must not
    /// delete the user.
    pub async fn cascade(&self, user_id: &str, mutation_id: &str) -> Result<usize, Unavailable> {
        let Some(base_url) = &self.base_url else {
            return Ok(0);
        };

        let response = self
            .client
            .get(format!("{base_url}/api/v1/todos"))
            .query(&[("user_id", user_id)])
            .send()
            .await
            .map_err(|e| Unavailable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(Unavailable(format!(
                "todo service answered {} when asked for {user_id}'s todos",
                response.status()
            )));
        }
        let todos: Vec<TodoRef> = response
            .json()
            .await
            .map_err(|e| Unavailable(e.to_string()))?;

        for todo in &todos {
            let deleted = self
                .client
                .delete(format!("{base_url}/api/v1/todos/{}", todo.id))
                .header("x-mutation-id", format!("{mutation_id}:todo:{}", todo.id))
                .send()
                .await
                .map_err(|e| Unavailable(e.to_string()))?;
            if !deleted.status().is_success() {
                // Stops here rather than pressing on. A cascade that skipped a failure would
                // report success with one todo left behind, which is the orphan this exists to
                // prevent — and the caller retries the whole thing, which the deterministic
                // mutation ids make free for everything already deleted.
                return Err(Unavailable(format!(
                    "todo service answered {} deleting {}",
                    deleted.status(),
                    todo.id
                )));
            }
        }

        Ok(todos.len())
    }
}

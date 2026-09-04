//! Who this client is, where its services are, and the user record it writes at signup.
//!
//! # "User" means two things here, and that is the lesson rather than a wart
//!
//! - The **scope** is the local queue identity. The client composes it, offline, with no server
//!   involved, and core compares it for equality and nothing else
//!   (`wiki/decisions/009-local-scope-identity.decision.md`).
//! - The **user record** is a row on the user service, created by an ordinary queued mutation like
//!   any other write.
//!
//! Same person, different lifetimes, and the whole point of decision 009 is that the first does not
//! wait for the second: **a queue can exist before any server has heard of you.** That is what D4d
//! makes observable — sign up offline, create todos immediately, reconnect, and the user record
//! must land before any todo that references it.
//!
//! Every name in this file says which of the two it means. A `user_id` is always the record.

use frontbox::{Error, RowRef, RowStore, StoredRow};

use super::{TodoApp, ROW_LOAD_LIMIT};
use crate::store::User;
use crate::trace;

/// The second entity this application stores.
///
/// frontbox's row keys are `(entity, row_id)` and it never parses either half, so the name is this
/// crate's to choose. D4a's `mod.rs` predicted a second entity would be "a second constant, not a
/// schema change"; this is that constant, and the prediction held.
pub const USER: &str = "user";

/// Everything an application needs to know before it opens storage.
///
/// # Why a struct rather than more arguments
///
/// D4d took `TodoApp::new` from four arguments to six, and two of the six are strings that mean
/// completely different things while looking identical at a call site — `scope` and `user_id` are
/// both `&str` and swapping them compiles. Naming them at the construction site is worth the type.
#[derive(Debug, Clone)]
pub struct Config {
    /// Base URL of the todo service.
    pub todo_url: String,
    /// Base URL of the user service, or [`None`] where there is not one.
    ///
    /// **An `Option` rather than "the same URL as `todo_url`", and that distinction was forced by
    /// a test.** Pointing both at one host makes a single-server deployment look like a two-service
    /// one whose user endpoints happen to 404 — and the only way to keep such a client working is
    /// to treat a 404 from a read as normal, which is precisely the failure
    /// `examples/todo-core/src/transport/mod.rs` warns about at length: a wrong base URL then
    /// presents as a healthy client with no data.
    ///
    /// `None` says the honest thing instead. There is no user service, so nothing hydrates user
    /// records, no source polls for them, and the routing transport has one destination.
    pub user_url: Option<String>,
    /// The local queue identity. Composed by this client, never issued by a server.
    pub scope: String,
    /// The user *record* id this client's todos belong to.
    pub user_id: String,
    /// Where durable storage lives — a file path natively, a database name on web.
    pub storage: String,
}

impl Config {
    /// A single-server configuration, which is what every trial before D4d had.
    ///
    /// The `user_id` is still required and still travels on every todo — the todo service needs an
    /// owner column whether or not anything validates it. What is absent is the *service*.
    pub fn single_server(
        base_url: impl Into<String>,
        scope: impl Into<String>,
        user_id: impl Into<String>,
        storage: impl Into<String>,
    ) -> Self {
        Self {
            todo_url: base_url.into(),
            user_url: None,
            scope: scope.into(),
            user_id: user_id.into(),
            storage: storage.into(),
        }
    }
}

impl TodoApp {
    /// The user record id this client writes todos under.
    pub fn user_id(&self) -> &str {
        &self.config.user_id
    }

    /// Create this client's own user record.
    ///
    /// **An ordinary queued write.** It is not a handshake, it does not contact a server, and it
    /// does not block: offline, it lands in the same outbox as everything else and drains when the
    /// network returns. That is the whole of decision 009's claim made concrete — the scope this
    /// application is already running under was never waiting for this call to succeed.
    ///
    /// Enqueued *before* the first todo, which is the only thing that makes cross-service ordering
    /// observable. One queue and one `seq` (decision 016) means this record reaches the user
    /// service before any todo referencing it reaches the todo service, and D4d's sabotage run —
    /// one scope per service — is what shows the guarantee being lost when the queue is split.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn sign_up(&self, name: &str) -> Result<(), Error> {
        trace::log(format!(
            "action sign_up user_id={} name={name:?}",
            self.config.user_id
        ));
        // Exactly `create_user` with the id this client already answers to, rather than a second
        // write path. Signing up is not a different kind of operation from adding somebody else —
        // it is the same write with a known id, which is the clearest statement of what decision
        // 009 means: the scope was never waiting for any of this.
        self.write_user(self.config.user_id.clone(), name).await?;
        Ok(())
    }

    /// Create a user record for somebody else.
    ///
    /// Returns the id, which the client generates — the same rule as a todo's, and for the same
    /// reason: the row exists locally before any server has seen it.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn create_user(&self, name: &str) -> Result<String, Error> {
        trace::log(format!("action create_user name={name:?}"));
        self.write_user(uuid::Uuid::new_v4().to_string(), name)
            .await
    }

    async fn write_user(&self, id: String, name: &str) -> Result<String, Error> {
        let user = User {
            id,
            name: name.to_owned(),
        };
        self.store.upsert_user(user.clone());
        self.runner
            .store()
            .put_rows(&[user_to_stored(&user)])
            .await?;
        self.enqueue_entity(
            USER,
            &user.id,
            "POST",
            "/api/v1/users".to_owned(),
            serde_json::json!({ "id": user.id, "name": name }),
            "create_user",
        )
        .await?;
        Ok(user.id)
    }

    /// Rename a user record.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn rename_user(&self, id: &str, name: &str) -> Result<(), Error> {
        trace::log(format!("action rename_user id={id} name={name:?}"));
        let user = User {
            id: id.to_owned(),
            name: name.to_owned(),
        };
        self.store.upsert_user(user.clone());
        self.runner
            .store()
            .put_rows(&[user_to_stored(&user)])
            .await?;
        self.enqueue_entity(
            USER,
            id,
            "PUT",
            format!("/api/v1/users/{id}"),
            serde_json::json!({ "name": name }),
            "rename_user",
        )
        .await
    }

    /// Delete a user record, and with it every todo that belongs to them.
    ///
    /// # One mutation, and the cascade is the server's
    ///
    /// This enqueues a single `DELETE /api/v1/users/{id}`. It does **not** enqueue a delete per
    /// todo: the user service empties the user of todos before it removes them, and refuses the
    /// whole thing if it cannot (`examples/user-server/src/todos.rs`). Doing it here instead would
    /// put the ordering guarantee in the client, where a second device deleting the same user would
    /// not benefit from it.
    ///
    /// What this *does* do locally is remove both, because the server is going to. That is the same
    /// optimistic rule every other write here follows — project first, enqueue second — applied to
    /// a consequence rather than to the write itself.
    ///
    /// # A queued create for this user will now fail, and that is correct
    ///
    /// A todo still queued for a user who has just been deleted drains, meets `404 unknown_user`,
    /// and is retained as `Blocked` until decision 017's retention bound dead-letters it. Nothing
    /// here tries to withdraw it: an enqueued mutation is a durable fact, and the dead-letter panel
    /// is where a human sees what became of it.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn delete_user(&self, id: &str) -> Result<(), Error> {
        trace::log(format!("action delete_user id={id}"));
        let orphaned = self.store.remove_user(id);

        let mut gone: Vec<RowRef> = orphaned
            .iter()
            .map(|todo_id| RowRef::new(super::TODO, todo_id))
            .collect();
        gone.push(RowRef::new(USER, id));
        self.runner.store().delete_rows(&gone).await?;

        self.enqueue_entity(
            USER,
            id,
            "DELETE",
            format!("/api/v1/users/{id}"),
            serde_json::json!({}),
            "delete_user",
        )
        .await
    }

    /// Replace the cached user records with the user service's own list.
    ///
    /// # Errors
    ///
    /// A transport failure.
    pub async fn refresh_users_from_server(&self) -> Result<(), Error> {
        // No user service, nothing to hydrate. Not an error and not a silent 404 swallowed one
        // layer down — see [`Config::user_url`].
        if self.config.user_url.is_none() {
            trace::log("hydrate users skipped user_url=<none>");
            return Ok(());
        }
        trace::log("hydrate users start");
        let users: Vec<User> = self.runner.transport().get_json("/api/v1/users").await?;
        trace::log(format!("hydrate users fetched={}", users.len()));

        // Merged rather than replaced, for the reason the todo list is
        // (`wiki/decisions/032-opaque-row-store.decision.md`): a user record queued and not yet
        // drained is a record the server has never seen, and writing its list over ours would drop
        // exactly the signup the user is waiting on.
        let stored: Vec<StoredRow> = users.iter().map(user_to_stored).collect();
        let skipped = self.runner.store().merge_rows(&stored).await?;
        // The same protected delete the todo hydration runs, and deliberately the same function:
        // a user row deleted while its signup is still queued takes that user's todos with it,
        // which makes this the more expensive of the two places to get it wrong
        // (`wiki/decisions/039-user-delete-cascades-server-side.decision.md`).
        let keep: std::collections::HashSet<String> = users
            .iter()
            .map(|user| user.id.clone())
            .chain(skipped.iter().map(|row| row.row_id.clone()))
            .collect();
        let prune = self.prune_rows_absent(USER, keep).await?;
        trace::log(format!(
            "hydrate users merged={} skipped={} queued={} deleted={} delete_suppressed={}",
            stored.len(),
            skipped.len(),
            prune.queued,
            prune.deleted,
            prune.suppressed
        ));
        self.load_users().await
    }

    /// Refill the cached user records from durable rows.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn load_users(&self) -> Result<(), Error> {
        let stored = self.runner.store().list_rows(USER, ROW_LOAD_LIMIT).await?;
        // Decoded once and kept. This ran `filter_map(user_from_stored)` twice over the same rows —
        // once to count the readable ones and once to build the list — which does the JSON work
        // twice per user on every hydration and every reload, for a number the first pass already
        // had.
        let users: Vec<User> = stored.iter().filter_map(user_from_stored).collect();
        // See `load_projection`: a row this version cannot read is skipped, and the count of them
        // is reported rather than left to be discovered as a missing user.
        trace::log(format!(
            "load users rows={} unreadable={}",
            users.len(),
            stored.len() - users.len()
        ));
        self.store.replace_users(users);
        Ok(())
    }
}

/// A user record as frontbox stores it: a key it compares and a blob it never reads.
pub fn user_to_stored(user: &User) -> StoredRow {
    StoredRow::new(
        RowRef::new(USER, &user.id),
        serde_json::to_value(user).unwrap_or(serde_json::Value::Null),
    )
}

/// Recover a user record from a stored row.
///
/// `None` for a blob this version cannot read, which is the honest answer for a row written by a
/// newer schema — skipped rather than raised, so one bad row cannot make the whole cache
/// unreadable.
fn user_from_stored(stored: &StoredRow) -> Option<User> {
    serde_json::from_value(stored.blob.clone()).ok()
}

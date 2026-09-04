//! Wiring: one outbox, one projection, four writes, and the direct-dispatch bet.
//!
//! A directory rather than a file because `AGENTS.md` caps a module at roughly four hundred lines,
//! and the two things that split off cleanly are the two findings: [`Direct`] and `classify` in
//! `direct.rs`, [`PendingIndexGap`] in `index.rs`. **No public path moved** — a reader and a
//! `use` statement cannot tell that `app::Direct` lives in `app/direct.rs`.
//!
//! It then grew back through the cap — to 579 lines, while this paragraph explained the cap —
//! and the second split is `sync.rs`: starting up, draining, and reconciling. The line is the one
//! the trial's findings already drew. What stays here decides *what a write means* before anything
//! durable happens; what left only reads, and writes back what it read. `scripts/verify.sh` now
//! measures the cap instead of trusting this comment to.

mod cache;
mod direct;
mod identity;
mod index;
mod prune;
mod rows;
mod startup;
mod sync;

pub use cache::{registry, Cache};
pub use direct::Direct;
pub use identity::{Config, USER};
pub use index::PendingIndexGap;
pub use startup::Startup;

use direct::{classify, Verdict};
use rows::{default_sources, from_stored, to_stored, transport_for};

use std::cell::Cell;
use std::rc::Rc;

use frontbox::{Clock, DeadLetterRecord, DeadLetterStore, Error, MutationId, ScopeKey, SyncRunner};

use crate::backend::{open_backend, Backend, Store};

use crate::invalidation::Sources;
use crate::store::{Change, Todo, TodoStore};
use crate::trace;
use crate::transport::{HttpTransport, SharedTransport};

/// The trial's application.
pub struct TodoApp {
    runner: SyncRunner<Store, SharedTransport>,
    store: TodoStore,
    /// Kept so [`reopen`](TodoApp::reopen) can open a second handle on the same storage, which is
    /// how a test says "the tab was reloaded" to a crate with no process to restart.
    backend: Backend,
    scope: ScopeKey,
    /// The cache runtime over this scope's durable versions, and the sources that feed it.
    ///
    /// Two fields rather than one, because they answer to different owners: the runner is core's
    /// and the source set is this crate's, which is decision 038's whole shape.
    cache: Cache,
    sources: Sources,
    config: Config,
    clock: Rc<dyn Clock>,
    pending_scan_limit: usize,
    index_gap: Cell<PendingIndexGap>,
}

const PENDING_INDEX_SCAN_LIMIT: usize = 10_000;

/// The one entity this application stores.
///
/// frontbox's row keys are `(entity, row_id)` and it never parses either half, so the name is
/// this crate's to choose. A second entity would be a second constant, not a schema change.
const TODO: &str = "todo";

/// How many rows one startup read will load.
const ROW_LOAD_LIMIT: usize = 10_000;

impl TodoApp {
    /// Build an application against a server and a scope.
    ///
    /// # Errors
    ///
    /// An invalid scope key.
    pub async fn new(config: Config, clock: impl Clock + 'static) -> Result<Self, Error> {
        trace::log(format!(
            "app open scope={} storage={} todo_url={} user_url={}",
            config.scope,
            config.storage,
            config.todo_url,
            config.user_url.as_deref().unwrap_or("<none>")
        ));
        // One `Rc<dyn Clock>` shared with the backend. Time enters through the caller in this
        // application for the same reason it does in core: a browser has no `SystemClock`
        // (`wiki/decisions/002-error-model.decision.md`, and `src/clock.rs`).
        let clock: Rc<dyn Clock> = Rc::new(clock);
        let key = ScopeKey::new(&config.scope)?;
        // The one line D4b changes, and `crate::backend` is the one file that knows which backend
        // that is. Everything below is what D4a already did.
        let backend = open_backend(&config.storage, &key, Rc::clone(&clock)).await?;
        let outbox = backend.open_scope(key.clone());
        let transport = transport_for(&config);
        let sources = default_sources(&transport, &config);
        Ok(Self {
            cache: Cache::new(backend.open_versions(key.clone()), cache::registry()),
            sources,
            runner: SyncRunner::new(outbox, transport),
            store: TodoStore::default(),
            backend,
            scope: key,
            config,
            clock,
            pending_scan_limit: PENDING_INDEX_SCAN_LIMIT,
            index_gap: Cell::new(PendingIndexGap::default()),
        })
    }

    /// Open a second application over the same durable storage.
    ///
    /// **What a reload is**, to a crate with no process to restart: same scope, same backend, a
    /// brand new projection and a brand new pending index. Everything that survives is what
    /// survives because it was durable, which is the whole of what D4b has to demonstrate.
    ///
    /// It shares the backend rather than the runner, so the new application has its own in-flight
    /// state — two tabs, not two handles on one tab. Single-flight still holds across both,
    /// because the claim is on the scope (`wiki/decisions/031-cross-realm-single-flight.decision.md`).
    #[must_use]
    pub fn reopen(&self) -> Self {
        let outbox = self.backend.open_scope(self.scope.clone());
        let transport = transport_for(&self.config);
        let sources = default_sources(&transport, &self.config);
        Self {
            cache: Cache::new(
                self.backend.open_versions(self.scope.clone()),
                cache::registry(),
            ),
            sources,
            runner: SyncRunner::new(outbox, transport),
            store: TodoStore::default(),
            backend: self.backend.clone(),
            scope: self.scope.clone(),
            config: self.config.clone(),
            clock: Rc::clone(&self.clock),
            pending_scan_limit: self.pending_scan_limit,
            index_gap: Cell::new(PendingIndexGap::default()),
        }
    }

    /// Set a retention bound, so a wedged record eventually terminates.
    #[must_use]
    pub fn with_retention_bound(mut self, bound: u32) -> Self {
        self.runner = self.runner.with_retention_bound(bound);
        self
    }

    /// Send one record per pass.
    #[must_use]
    pub fn with_batch_limit(mut self, limit: usize) -> Self {
        self.runner = self.runner.with_batch_limit(limit);
        self
    }

    /// Cap how many queued records one pending-index rebuild will read.
    ///
    /// The default is ten thousand. It exists because `pending_batch` takes a limit and passing
    /// `usize::MAX` is a promise no durable backend can keep — a SQL `LIMIT` binds as `i64`. A
    /// smaller value is how the trial reaches [`PendingIndexGap::beyond_scan_cap`] in a test.
    #[must_use]
    pub fn with_pending_scan_limit(mut self, limit: usize) -> Self {
        self.pending_scan_limit = limit;
        self
    }

    /// The invalidation sources this application runs.
    ///
    /// Exposed so a test can hand an event to the manual one, which is the only way to exercise
    /// the push half of the seam without a stream to push it.
    pub fn sources(&self) -> &Sources {
        &self.sources
    }

    /// Every projected todo, in id order.
    pub fn rows(&self) -> Vec<Todo> {
        self.store.rows()
    }

    /// The display name for a user record, when this client has cached one.
    pub fn user_name(&self, user_id: &str) -> Option<String> {
        self.store.user_name(user_id)
    }

    /// The projection a UI renders.
    pub fn store(&self) -> &TodoStore {
        &self.store
    }

    /// The transport, for pulling the plug.
    pub fn transport(&self) -> &HttpTransport {
        self.runner.transport()
    }

    /// The outbox.
    pub fn outbox(&self) -> &Store {
        self.runner.store()
    }

    /// What the last pending-index *reconciliation* could not account for.
    ///
    /// Empty on a healthy queue. A UI that renders "saving…" from the index should say when the
    /// index is incomplete rather than quietly render a row as saved.
    ///
    /// Set by [`refresh_pending`](TodoApp::refresh_pending), which since decision 035 runs at
    /// startup rather than after every drain — so this describes the last reconciliation, not the
    /// last sync. During a session the index is maintained exactly and this stays as the startup
    /// scan left it.
    pub fn pending_index_gap(&self) -> PendingIndexGap {
        self.index_gap.get()
    }

    /// Create a todo owned by `user_id`.
    ///
    /// # Why the owner is a parameter and not `config.user_id`
    ///
    /// It was the configured user until the UI could address any of them. Once a client manages a
    /// list of users and a list of todos inside each, "whose todo is this" stops being a property
    /// of the client and becomes a property of the call — and an implicit owner would then be a
    /// second, quieter way to say something the caller already knows.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn create(&self, user_id: &str, title: &str) -> Result<String, Error> {
        let id = uuid::Uuid::new_v4().to_string();
        trace::log(format!(
            "action create_todo id={id} user_id={user_id} title={title:?}"
        ));
        let user_id = user_id.to_owned();
        self.store.project(Change::Upsert(Todo {
            id: id.clone(),
            title: title.to_owned(),
            done: false,
            user_id: user_id.clone(),
        }));
        self.persist_row(&id).await?;
        self.enqueue(
            &id,
            "POST",
            "/api/v1/todos".to_owned(),
            serde_json::json!({ "id": id, "title": title, "user_id": user_id }),
            "create_todo",
        )
        .await?;
        Ok(id)
    }

    /// Rename a todo.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn rename(&self, id: &str, title: &str) -> Result<(), Error> {
        trace::log(format!("action rename_todo id={id} title={title:?}"));
        self.store.project(Change::Rename {
            id: id.to_owned(),
            title: title.to_owned(),
        });
        self.persist_row(id).await?;
        self.enqueue(
            id,
            "PUT",
            format!("/api/v1/todos/{id}"),
            serde_json::json!({ "title": title }),
            "rename_todo",
        )
        .await
    }

    /// Delete a todo.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn delete(&self, id: &str) -> Result<(), Error> {
        trace::log(format!("action delete_todo id={id}"));
        self.store.project(Change::Remove { id: id.to_owned() });
        self.persist_row(id).await?;
        self.enqueue(
            id,
            "DELETE",
            format!("/api/v1/todos/{id}"),
            serde_json::json!({}),
            "delete_todo",
        )
        .await
    }

    /// Toggle a todo, trying the server first.
    ///
    /// The one write that attempts direct dispatch, because a checkbox is the interaction where a
    /// round trip is cheap and the user is watching. See [`Direct`].
    ///
    /// # A row with queued work is never dispatched directly
    ///
    /// Direct dispatch is an overtake: it puts this write on the wire ahead of everything already
    /// queued. That is safe only when the queue holds nothing about *this row*. A toggle sent
    /// directly while the row's `create` is still queued reaches a server that has never heard of
    /// the row, which answers `not_found` — a terminal refusal for a write that was perfectly
    /// valid, caused entirely by the order it was sent in. So a saving row queues instead, and the
    /// ordering the outbox already guarantees does the rest.
    ///
    /// # Errors
    ///
    /// A storage failure while falling back to the queue.
    pub async fn set_done(&self, id: &str, done: bool) -> Result<Direct, Error> {
        trace::log(format!("action toggle_todo id={id} done={done}"));
        let previous = self.store.get(id);
        self.store.project(Change::SetDone {
            id: id.to_owned(),
            done,
        });
        self.persist_row(id).await?;

        // Generated once and used for *both* paths. This is the whole of what core supplies for
        // direct dispatch, and it is enough for idempotency: if the direct call actually reached
        // the server and the response was lost, the queued replay carries the same id and the
        // server answers `Duplicate`.
        let mutation_id = MutationId::new();
        let body = serde_json::json!({ "done": done });
        let path = format!("/api/v1/todos/{id}/done");

        if self.store.is_saving(id) {
            self.enqueue_with_id(mutation_id, id, "PUT", path, body, "toggle_todo")
                .await?;
            return Ok(Direct::Queued);
        }

        let attempt = self
            .runner
            .transport()
            .put_json(&path, mutation_id, &body)
            .await;

        match classify(attempt).await {
            Verdict::Applied => Ok(Direct::Applied),
            Verdict::Refused(refusal) => {
                // Roll back only while the projection still holds what this call wrote. Another
                // action can land in the window the request was open, and restoring `previous`
                // over it would undo a write no server ever refused.
                if self.store.get(id).is_some_and(|row| row.done == done) {
                    if let Some(previous) = previous {
                        self.store.project(Change::SetDone {
                            id: previous.id,
                            done: previous.done,
                        });
                    }
                }
                Ok(Direct::Refused(refusal))
            }
            Verdict::Undecided => {
                self.enqueue_with_id(mutation_id, id, "PUT", path, body, "toggle_todo")
                    .await?;
                Ok(Direct::Queued)
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // Sync
    // ---------------------------------------------------------------------------------------

    /// What the server terminally refused.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn dead_letters(&self, limit: usize) -> Result<Vec<DeadLetterRecord>, Error> {
        DeadLetterStore::list(self.runner.store(), limit).await
    }
}

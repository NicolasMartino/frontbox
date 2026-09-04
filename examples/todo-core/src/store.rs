//! The application-owned read model, and the pending index the UI actually needs.
//!
//! frontbox stores the *queue*, never the rows
//! (`wiki/decisions/023-read-model-boundary.decision.md`). Everything here is what an application
//! has to own as a result, written out rather than assumed so the trial can say what that costs.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

/// One todo, as the application projects it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Todo {
    /// Client-generated, so the row exists locally before any server has seen it.
    pub id: String,
    /// What the user typed.
    pub title: String,
    /// Whether it is ticked.
    pub done: bool,
    /// Which user *record* owns this todo.
    ///
    /// A row id on the user service, not a [`ScopeKey`](frontbox::ScopeKey). The scope is this
    /// client's local queue identity, composed offline with no server involved
    /// (`wiki/decisions/009-local-scope-identity.decision.md`); this names a row that a server
    /// either has or has not accepted yet. D4d's whole ordering story is that the second can lag
    /// the first, so anything that conflates them will read as correct and behave otherwise.
    ///
    pub user_id: String,
}

/// One user record, as the application projects it.
///
/// Read-only here: this client creates its own user record at signup and never edits anyone
/// else's. It exists so a todo can be attributed to a name rather than to an opaque id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// The user record's row id.
    pub id: String,
    /// Display name.
    pub name: String,
}

/// The optimistic projection, plus how many queued writes each row is waiting on.
///
/// # The pending index, and why it is rebuilt rather than maintained
///
/// A row rendering "saving…" has to answer *is there a queued mutation for row X*. Nothing in
/// frontbox answers that cheaply: `pending_count` and the adapter's `OutboxCounts` are
/// cardinalities, and `pending_batch` is a queue scan — fine once per drain, not once per render.
/// So the application keeps an index, which
/// `wiki/proposals/offline-todo-trial.proposal.md` predicted it would have to.
///
/// What was not predicted is that **the index could not be kept up to date from a drain's result**
/// — and that this was a gap in core rather than a fact about indexes. `DrainReport` and
/// `SyncReport` carried counts and anomalies, and the only mutation ids they named were the ones
/// something had gone *wrong* with. Nothing named the ids that drained successfully, so there was
/// no event to decrement on and the only correct move was to rebuild the whole index after every
/// drain.
///
/// That was D4a's Finding 1, and `wiki/decisions/035-reports-name-what-drained.decision.md` closed
/// it: `DrainReport::drained` now names what left the queue, so
/// [`unmark_pending`](TodoStore::unmark_pending) decrements and the rebuild is no longer the only
/// option. [`set_pending_rows`](TodoStore::set_pending_rows) remains, because a client that has
/// just reopened durable storage has an index to build rather than to correct — that is what
/// `TodoApp::refresh_pending` is for. The rule now: **rebuild at startup, decrement per drain.**
#[derive(Debug, Default)]
pub struct TodoStore {
    rows: RefCell<BTreeMap<String, Todo>>,
    /// Every user record this client has cached, by id.
    ///
    /// **Cached into this scope's row store, which is correct and reads as a leak.** The row store
    /// holds what *this client* fetched, not what this user owns — so another user's record living
    /// beside your own todos is the design working. Decision 009's isolation is about the queue and
    /// about reads through it, and a public read model that every client can fetch is neither.
    users: RefCell<BTreeMap<String, User>>,
    /// Row id to unsent-write count.
    ///
    /// **A row with no queued work has no entry**, never an entry of zero: the map is rebuilt
    /// rather than decremented, so nothing ever counts down to zero in place. `saving_rows` reads
    /// the map's length on the strength of that invariant.
    pending: RefCell<HashMap<String, usize>>,
}

impl TodoStore {
    /// Every row, in id order.
    pub fn rows(&self) -> Vec<Todo> {
        self.rows.borrow().values().cloned().collect()
    }

    /// One row.
    pub fn get(&self, id: &str) -> Option<Todo> {
        self.rows.borrow().get(id).cloned()
    }

    /// Every cached user record, in display order.
    ///
    /// **By name, not by id.** The map is keyed by id because that is the identity, and rendering
    /// in that order put a user called Bob above one called Alice for no reason a reader could see
    /// — client-generated uuids sort arbitrarily. A caller that wants a stable identity order still
    /// has the ids.
    pub fn users(&self) -> Vec<User> {
        let mut users: Vec<User> = self.users.borrow().values().cloned().collect();
        users.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        users
    }

    /// The display name for a user record, when this client has cached one.
    pub fn user_name(&self, id: &str) -> Option<String> {
        self.users.borrow().get(id).map(|user| user.name.clone())
    }

    /// Every todo belonging to one user, in id order.
    pub fn rows_for(&self, user_id: &str) -> Vec<Todo> {
        self.rows
            .borrow()
            .values()
            .filter(|todo| todo.user_id == user_id)
            .cloned()
            .collect()
    }

    /// How many todos belong to one user.
    ///
    /// Separate from [`rows_for`](TodoStore::rows_for) because a collapsed row in the UI needs the
    /// number and not the rows, and cloning a list to call `.len()` on it is the kind of thing a
    /// render does sixty times a second.
    pub fn count_for(&self, user_id: &str) -> usize {
        self.rows
            .borrow()
            .values()
            .filter(|todo| todo.user_id == user_id)
            .count()
    }

    /// Drop a user record and every todo of theirs, returning the todo ids removed.
    ///
    /// Both in one step, because the server does both — see
    /// [`TodoApp::delete_user`](crate::TodoApp::delete_user). The returned ids are what the caller
    /// needs to delete from durable storage; the projection is already done.
    pub fn remove_user(&self, id: &str) -> Vec<String> {
        self.users.borrow_mut().remove(id);
        let mut rows = self.rows.borrow_mut();
        let orphaned: Vec<String> = rows
            .values()
            .filter(|todo| todo.user_id == id)
            .map(|todo| todo.id.clone())
            .collect();
        for todo_id in &orphaned {
            rows.remove(todo_id);
            // The pending marker goes with the row it describes. Left behind it would count toward
            // `saving_rows` forever, for a row nothing renders.
            self.pending.borrow_mut().remove(todo_id);
        }
        orphaned
    }

    /// Add or replace one cached user record.
    pub fn upsert_user(&self, user: User) {
        self.users.borrow_mut().insert(user.id.clone(), user);
    }

    /// Replace the cached user records wholesale.
    pub fn replace_users(&self, users: Vec<User>) {
        *self.users.borrow_mut() = users
            .into_iter()
            .map(|user| (user.id.clone(), user))
            .collect();
    }

    /// How many rows the projection holds.
    pub fn len(&self) -> usize {
        self.rows.borrow().len()
    }

    /// Whether the projection is empty.
    pub fn is_empty(&self) -> bool {
        self.rows.borrow().is_empty()
    }

    /// Whether this row is waiting on unsent work.
    pub fn is_saving(&self, id: &str) -> bool {
        self.pending.borrow().contains_key(id)
    }

    /// Apply a write locally, before the server has seen it.
    pub fn project(&self, change: Change) {
        let mut rows = self.rows.borrow_mut();
        match change {
            Change::Upsert(todo) => {
                rows.insert(todo.id.clone(), todo);
            }
            Change::Rename { id, title } => {
                if let Some(row) = rows.get_mut(&id) {
                    row.title = title;
                }
            }
            Change::SetDone { id, done } => {
                if let Some(row) = rows.get_mut(&id) {
                    row.done = done;
                }
            }
            Change::Remove { id } => {
                rows.remove(&id);
            }
        }
    }

    /// Replace the projection with what the server holds.
    pub fn replace(&self, rows: Vec<Todo>) {
        let mut held = self.rows.borrow_mut();
        held.clear();
        for row in rows {
            held.insert(row.id.clone(), row);
        }
    }

    /// Rebuild the pending index from the row ids still queued.
    ///
    /// Duplicate ids are counted, not deduplicated: this is queued mutations per row, not merely
    /// the set of rows with any queued work. Called after a drain, not per render. See
    /// [`TodoStore`] for why it is a rebuild.
    pub fn set_pending_rows(&self, ids: impl IntoIterator<Item = String>) {
        let mut pending = self.pending.borrow_mut();
        pending.clear();
        for id in ids {
            *pending.entry(id).or_insert(0) += 1;
        }
    }

    /// Note that a row now has one more unsent write.
    pub fn mark_pending(&self, id: &str) {
        *self.pending.borrow_mut().entry(id.to_owned()).or_insert(0) += 1;
    }

    /// Note that one of a row's unsent writes has left the queue.
    ///
    /// Driven by `DrainReport::drained`, which is what made an incremental index possible at all —
    /// before it, nothing named the mutations that succeeded, so the only correct move was a full
    /// rebuild after every drain.
    ///
    /// **The entry is removed at zero rather than left holding one**, because `saving_rows` reads
    /// the map's length on the invariant that a key means outstanding work. A stale zero would
    /// render a row as "saving…" forever.
    pub fn unmark_pending(&self, id: &str) {
        let mut pending = self.pending.borrow_mut();
        if let Some(count) = pending.get_mut(id) {
            *count -= 1;
            if *count == 0 {
                pending.remove(id);
            }
        }
    }

    /// How many rows are waiting on unsent work.
    pub fn saving_rows(&self) -> usize {
        self.pending.borrow().len()
    }
}

/// A local projection step.
///
/// Applied before the write is enqueued, never after: "always enqueue" means the UI shows what
/// the user did without waiting on a network that may not be there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Insert or replace a whole row.
    Upsert(Todo),
    /// Change one row's title.
    Rename {
        /// Which row.
        id: String,
        /// The new title.
        title: String,
    },
    /// Tick or untick one row.
    SetDone {
        /// Which row.
        id: String,
        /// The new state.
        done: bool,
    },
    /// Drop a row.
    Remove {
        /// Which row.
        id: String,
    },
}

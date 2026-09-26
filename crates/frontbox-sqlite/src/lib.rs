//! Durable SQLite persistence for frontbox, on native targets.
//!
//! # What this crate is
//!
//! One of the two backends D5 owes. It implements the storage traits core defines and adds no
//! public vocabulary of its own beyond opening a database — the whole point being that an
//! application swapping `InMemoryBackend` for [`SqliteBackend`] changes one construction site and
//! nothing else.
//!
//! # `rusqlite`, and why the futures still do not `Send`
//!
//! `rusqlite` is synchronous, so every `async fn` here completes without yielding. That is honest
//! for a local file — there is no IO to await that a thread is not already blocked on — and it is
//! what keeps every future `!Send` without pretending: the connection lives behind
//! `Rc<RefCell<_>>`, exactly as the in-memory backend's state does.
//!
//! The source system holds `Arc<tokio::sync::Mutex<Connection>>` because it assumed `Send`
//! (`wiki/specs/source-frontend-cache-architecture.spec.md`). Same structure, one fewer guarantee
//! claimed. `sqlx` was rejected for the opposite reason: its futures *are* `Send`-shaped, which
//! `scripts/verify.sh`'s `no Send bound` gate exists to keep out of this dependency graph.
//!
//! # Atomicity
//!
//! `apply_outcomes` runs inside one `IMMEDIATE` transaction spanning the outbox, the dead letters
//! and the quarantine. Deferred would take the write lock late and could fail partway through work
//! already applied, which is precisely the intermediate state the trait exists to make
//! unrepresentable.

#![deny(missing_docs)]

mod backend;
mod coalescing;
mod convert;
mod migration;
mod rows;
mod schema;
mod store;
mod terminal;
mod versions;

#[cfg(feature = "testing")]
mod factory;

pub use backend::{SqliteBackend, SqliteStore};
pub use versions::SqliteVersionStore;

#[cfg(feature = "testing")]
pub use factory::SqliteFactory;

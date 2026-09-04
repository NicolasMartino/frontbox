//! The application half of the D4a offline todo trial.
//!
//! One thin flow — create, rename, toggle, delete — migrated onto frontbox abstractions, with the
//! optimistic projection and the read model that decision 023 says an application owns. No Dioxus:
//! if the application logic needed the adapter, the adapter would be leaking, and keeping this
//! crate framework-free is what makes that falsifiable.
//!
//! It is also what lets the trial's observations run as ordinary tests against a real server
//! instead of as "start it and look". See `examples/todo-core/tests/observations/main.rs`.
//!
//! # What this crate exists to find out
//!
//! Not whether the code works. Whether the API *expresses a real flow without churn* — which is the
//! roadmap's stated reason for putting the migration trial before the durable backends. The
//! findings are recorded in `wiki/plans/d4a-offline-todo-trial.plan.md`; the two sharpest are on
//! [`TodoStore::pending`](store::TodoStore) and [`app::Direct`].
//!
//! The application runtime is intentionally `!Send`. The error code is pinned, because a bare
//! `compile_fail` also passes when the snippet fails to build for an unrelated reason — a renamed
//! import would leave this reading as a proof while proving nothing:
//!
//! ```compile_fail,E0277
//! use frontbox::{InMemoryStore, SyncRunner};
//! use todo_core::HttpTransport;
//!
//! fn assert_send<T: Send>() {}
//!
//! assert_send::<SyncRunner<InMemoryStore, HttpTransport>>();
//! ```
#![deny(missing_docs)]

pub mod app;
pub mod backend;
pub mod invalidation;
pub mod store;
mod trace;
pub mod transport;

pub use app::{Config, Direct, PendingIndexGap, Startup, TodoApp, USER};
pub use store::{Change, Todo, TodoStore, User};
pub use transport::{HttpTransport, SharedTransport};

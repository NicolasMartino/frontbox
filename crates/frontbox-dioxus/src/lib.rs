//! # frontbox-dioxus
//!
//! Dioxus bindings for [`frontbox`]: a background loop that drains the outbox on a cadence, the
//! counts a component renders, and the wait that loop serves.
//!
//! ## What is here, and what is deliberately not
//!
//! The sync *loop* is not here. [`SyncRunner::drain`](frontbox::SyncRunner::drain) lives in core,
//! because it needs no timer and no framework and because a native application wants it just as
//! much as a Dioxus one (`wiki/decisions/028-drain-loop-boundary.decision.md`).
//!
//! What this crate adds is the part that genuinely needs a host: **the cadence**. How long to wait
//! before draining again is a policy about a wall clock, and core has no clock of its own by
//! design. [`use_sync_loop`] is that policy, and [`SyncCadence`] is where a caller states it.
//!
//! ## The sleep is injected
//!
//! Dioxus has no portable sleep, and the platform answers differ — `gloo-timers` on web,
//! `tokio::time` on desktop. Rather than take a conditional dependency on both, this crate takes a
//! [`Sleeper`] the application builds from whichever it already has. That is the same seam
//! [`Clock`](frontbox::Clock) draws in core: time enters through the caller.
//!
//! ```ignore
//! use frontbox_dioxus::{use_sync_loop, CountsStep, OutboxCounts, Sleeper, SyncCadence, SyncStep};
//!
//! fn App() -> Element {
//!     // Two closures, not a runner. Syncing is rarely just `drain` — an application usually has
//!     // something to do afterwards — and an application that owns its runtime has no runner to
//!     // hand over. Both capabilities are closures, so the hook asks for closures.
//!     let sync = use_sync_loop(
//!         SyncStep::new(move || async move { app.sync().await }),
//!         CountsStep::new(move || async move { OutboxCounts::read(app.outbox()).await }),
//!         SyncCadence::default(),
//!         Sleeper::new(gloo_timers::future::TimeoutFuture::new),
//!     );
//!
//!     rsx! { "{sync.counts.read().pending} pending" }
//! }
//! ```
//!
//! `ignore` because the snippet needs a Dioxus runtime, an application type and a timer crate that
//! this crate does not depend on. `examples/todo-app/src/sync.rs` is the same code, compiled.
//!
//! ## Shape
//!
//! Nothing here adds a `Send` bound, for the same reason nothing in core does: an IndexedDB future
//! is `!Send` and cannot be wrapped into one. Dioxus agrees — `spawn` takes a plain
//! `Future<Output = ()> + 'static`.
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

mod counts;
#[cfg(feature = "native")]
mod native;
mod sync;
#[cfg(feature = "web")]
mod web;

pub use counts::OutboxCounts;
#[cfg(feature = "native")]
pub use native::{use_lifecycle_wake, Lifecycle};
pub use sync::{use_sync_loop, CountsStep, Sleeper, SyncCadence, SyncState, SyncStep, Wake};
#[cfg(feature = "web")]
pub use web::{use_bfcache_wake, WebClock};

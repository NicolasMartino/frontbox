//! Durable IndexedDB persistence for frontbox, on web targets.
//!
//! # The constraint that shapes every method here
//!
//! An IndexedDB transaction is active only while a request against it is outstanding, and goes
//! inactive as soon as control returns to the event loop with nothing pending. So the rule for
//! everything in this crate is: **inside a transaction, await IDB requests and nothing else.** No
//! timers, no `fetch`, no channel resolved by anything but a request's own callback. The
//! `request` module is where that rule is made followable rather than merely stated.
//!
//! It is the reason this crate uses `web-sys` directly instead of a wrapper such as `rexie`, which
//! is what the source system used: [`RowStore::merge_rows`](frontbox::RowStore::merge_rows) has to
//! read the queue and write the rows as one unit, and a wrapper that decides when requests are
//! issued is a wrapper that can silently break that.
//!
//! # `!Send`, unavoidably
//!
//! Every `web-sys` handle is `!Send`, and the futures here hold them across await points. That is
//! the target this whole crate family was shaped around — decision 001's no-`Send`-bound rule is
//! not a preference, it is what makes this backend expressible at all.
//!
//! # Sequence numbers
//!
//! `seq` comes from an object store with `autoIncrement`, which IndexedDB guarantees is monotonic
//! per store and never reuses a value after a delete. That is decision 016's requirement met with
//! no extra round trip, and it is the same property `AUTOINCREMENT` supplies in SQLite.

#![deny(missing_docs)]
#![cfg(target_arch = "wasm32")]

mod backend;
mod coalescing;
mod convert;
mod locks;
mod request;
mod rows;
mod scan;
mod store;
mod versions;

#[cfg(feature = "testing")]
mod factory;

pub use backend::{IdbBackend, IdbStore};
pub use versions::IdbVersionStore;

#[cfg(feature = "testing")]
pub use factory::IdbFactory;

//! Regression tests for startup and read-model hydration.

// `#[path]` because the facade moved one directory down; the helpers stay where every suite finds
// them.
#[path = "../common/mod.rs"]
mod common;

mod protection;
mod shared_realm;
mod startup;

//! Regression tests for defects found in the D4a worktree review.

// `#[path]` because the facade moved one directory down; the helpers stay where every suite finds
// them.
#[path = "../common/mod.rs"]
mod common;

mod destinations;
mod dispatch;
mod queue;

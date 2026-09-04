//! D4d's observations: two domains, one queue, and the first invalidation that ever runs.
//!
//! Numbered in the D4a style, because that is what made the earlier trials evidence rather than
//! demos. Each `observation_N_*` is one claim, asserted against two real servers over real HTTP.
//!
//! # What this file exists to prove
//!
//! Two guarantees this project had already paid for and never tested
//! (`wiki/plans/d4d-multi-domain-trial.plan.md`):
//!
//! - **Cross-service enqueue order.** Decision 036 refused to partition the outbox and accepted
//!   head-of-line blocking as the price, explicitly to keep enqueue order across types. Every test
//!   written before this file sends to one server, so the guarantee's only evidence was that
//!   nothing had contradicted it.
//! - **More than one invalidation source.** Decision 038 makes multiple origins a criterion for
//!   promoting the inbound seam into core, because per-source reconnect semantics are where the
//!   design is load-bearing.
//!
//! And one thing that had never happened at all: **an application calling
//! `InvalidationRunner::apply`.**

// `#[path]` because the facade now lives one directory down: `mod common;` from here would look for
// `multi_domain/common.rs`. The helpers stay where every other suite finds them.
#[path = "../common/mod.rs"]
mod common;

// The split is by what each group observes, which is the same line the plan draws:
// the queue and its order, the invalidation runner, and user records as rows.
mod invalidation;
mod queue;
mod users;

/// Read the user service's list over HTTP.
async fn read_users(servers: &common::Servers) -> Vec<serde_json::Value> {
    reqwest::get(format!("{}/api/v1/users", servers.user_url))
        .await
        .expect("read users")
        .json()
        .await
        .expect("json")
}

/// Read the todo service's list over HTTP.
async fn read_todos(servers: &common::Servers) -> Vec<serde_json::Value> {
    reqwest::get(format!("{}/api/v1/todos", servers.todo_url))
        .await
        .expect("read todos")
        .json()
        .await
        .expect("json")
}

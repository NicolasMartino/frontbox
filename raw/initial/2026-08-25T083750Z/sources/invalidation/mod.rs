//! Invalidation fan-out support for multi-instance fullstack deployments.

mod consumer;
mod mapper;

pub use consumer::{consumer_group_for_instance, InvalidationConsumer};
pub use mapper::invalidation_entities_for_outcome;

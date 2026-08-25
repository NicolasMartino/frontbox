//! The shared conformance suite, run against the in-memory backend.
//!
//! Nothing here is specific to that backend. When the durable SQLite and IndexedDB backends land
//! they run these same functions through the same macros, which is what makes "all backends behave
//! identically" a checkable claim.
//!
//! Requires the `testing` feature: `cargo test --all-features`.
#![cfg(feature = "testing")]

use frontbox::memory::InMemoryFactory;

frontbox::frontbox_conformance_tests! {
    #[test]
    factory: InMemoryFactory::new(),
    block_on: pollster::block_on,
}

frontbox::frontbox_fault_injection_tests! {
    #[test]
    factory: InMemoryFactory::new(),
    block_on: pollster::block_on,
}

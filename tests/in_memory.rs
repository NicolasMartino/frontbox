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

frontbox::frontbox_cache_tests! {
    #[test]
    factory: InMemoryFactory::new(),
    block_on: pollster::block_on,
}

frontbox::frontbox_single_flight_tests! {
    #[test]
    factory: InMemoryFactory::new(),
    block_on: pollster::block_on,
}

frontbox::frontbox_blocking_only_tests! {
    #[test]
    factory: InMemoryFactory::new(),
    block_on: pollster::block_on,
}

frontbox::frontbox_row_tests! {
    #[test]
    factory: InMemoryFactory::new(),
    block_on: pollster::block_on,
}

/// The async emission shape, compiled but not run.
///
/// The backend that needs it is IndexedDB, which is D5 and does not exist yet, and no attribute
/// available to a native test both accepts an `async fn` and runs it without adding a runtime
/// dependency this crate has no other use for. So these are emitted with `#[allow(dead_code)]`:
/// nothing executes, but the macro is expanded and type-checked on every build.
///
/// That is the failure this guards against. A macro nobody instantiates is a macro that compiles
/// until the day someone tries it — which for the async variant would be halfway through the
/// IndexedDB port, when a missing `.await` or a stale case name is at its most expensive to
/// discover. The cases themselves are already proven by the synchronous suite above; what is
/// unproven without this is the wrapper.
mod async_shape {
    use frontbox::memory::InMemoryFactory;

    frontbox::frontbox_conformance_tests_async! {
        #[allow(dead_code)]
        factory: InMemoryFactory::new(),
    }

    frontbox::frontbox_fault_injection_tests_async! {
        #[allow(dead_code)]
        factory: InMemoryFactory::new(),
    }

    frontbox::frontbox_cache_tests_async! {
        #[allow(dead_code)]
        factory: InMemoryFactory::new(),
    }

    frontbox::frontbox_single_flight_tests_async! {
        #[allow(dead_code)]
        factory: InMemoryFactory::new(),
    }

    frontbox::frontbox_row_tests_async! {
        #[allow(dead_code)]
        factory: InMemoryFactory::new(),
    }
}

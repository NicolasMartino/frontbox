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

frontbox::frontbox_coalescing_tests! {
    #[test]
    factory: InMemoryFactory::new(),
    block_on: pollster::block_on,
}

/// The async emission shape, compiled but not run.
///
/// The backend that needs it is IndexedDB, which now exists and runs these suites for real in
/// headless Chrome — so this is no longer the only proof the async wrappers compile. It is kept
/// because it is the *cheap* proof: no attribute available to a native test both accepts an
/// `async fn` and runs it without a runtime dependency this crate has no other use for, so these
/// are emitted with `#[allow(dead_code)]` and nothing executes, but the macro is expanded and
/// type-checked on every ordinary `cargo test`.
///
/// That is what it still guards. A stale case name or a missing `.await` in an async wrapper is
/// otherwise invisible until someone runs the browser gate — which `scripts/verify.sh` announces
/// as SKIPPED when `CHROMEDRIVER` is unset, so it is the gate most likely to be absent on the
/// machine where the mistake is made. The cases themselves are proven by the synchronous suite
/// above; what this keeps honest is the wrapper.
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

    frontbox::frontbox_coalescing_tests_async! {
        #[allow(dead_code)]
        factory: InMemoryFactory::new(),
    }
}

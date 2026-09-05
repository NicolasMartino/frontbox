//! The macros that turn the case functions into a backend's test suite.
//!
//! `#[macro_export]` puts these at the crate root regardless of which module defines them, so this
//! file is an organizational boundary only — `frontbox::frontbox_conformance_tests!` is the path
//! either way.
//!
//! # Why the case list appears once
//!
//! Four public entry points share two case lists. Repeating the names per entry point is how a
//! suite silently stops running a case: someone adds case 34, updates the list they were looking
//! at, and the wasm backend quietly tests one fewer thing than the native one — with both reporting
//! a full pass. So each list lives in exactly one `__frontbox_*_suite` macro, and the public macros
//! differ only in the `run:` marker they hand it.

/// Emit one test per conformance case.
///
/// `factory` is evaluated once per case. `block_on` runs the returned future — this crate has no
/// runtime of its own, so the caller supplies one.
///
/// Use [`frontbox_conformance_tests_async`](crate::frontbox_conformance_tests_async) instead where
/// no such executor exists, which on `wasm32` is everywhere.
///
/// ```ignore
/// frontbox::frontbox_conformance_tests! {
///     #[test]
///     factory: MyFactory::new(),
///     block_on: pollster::block_on,
/// }
/// ```
#[macro_export]
macro_rules! frontbox_conformance_tests {
    ($(#[$attr:meta])* factory: $factory:expr, block_on: $block_on:path $(,)?) => {
        $crate::__frontbox_conformance_suite! {
            run: { block_on: $block_on },
            attr: [$(#[$attr])*],
            factory: $factory,
        }
    };
}

/// Emit one `async` test per conformance case.
///
/// Identical coverage to [`frontbox_conformance_tests`](crate::frontbox_conformance_tests), with
/// the case awaited directly instead of driven by a caller-supplied `block_on`. The test attribute
/// therefore has to accept an `async fn` — `#[wasm_bindgen_test]`, `#[tokio::test]`, and
/// `#[async_std::test]` do; plain `#[test]` does not.
///
/// This is the form a browser backend needs. An IndexedDB future cannot make progress until the
/// stack unwinds and the JavaScript event loop runs, so there is no `block_on` on wasm that would
/// not deadlock.
///
/// ```ignore
/// frontbox::frontbox_conformance_tests_async! {
///     #[wasm_bindgen_test::wasm_bindgen_test]
///     factory: IndexedDbFactory::new(),
/// }
/// ```
#[macro_export]
macro_rules! frontbox_conformance_tests_async {
    ($(#[$attr:meta])* factory: $factory:expr $(,)?) => {
        $crate::__frontbox_conformance_suite! {
            run: { asynchronous },
            attr: [$(#[$attr])*],
            factory: $factory,
        }
    };
}

/// Emit the cache version and invalidation cases.
///
/// Separate from [`frontbox_conformance_tests`](crate::frontbox_conformance_tests) because it needs
/// [`VersionStoreFactory`](crate::testing::VersionStoreFactory). A backend with no version store
/// yet simply does not invoke this, which leaves the gap visible in its test file rather than
/// hidden behind a runtime skip.
#[macro_export]
macro_rules! frontbox_cache_tests {
    ($(#[$attr:meta])* factory: $factory:expr, block_on: $block_on:path $(,)?) => {
        $crate::__frontbox_cache_suite! {
            run: { block_on: $block_on },
            attr: [$(#[$attr])*],
            factory: $factory,
        }
    };
}

/// Emit the cache version and invalidation cases as `async` tests.
///
/// The async counterpart of [`frontbox_cache_tests`](crate::frontbox_cache_tests); see
/// [`frontbox_conformance_tests_async`](crate::frontbox_conformance_tests_async) for why a wasm
/// backend needs one.
#[macro_export]
macro_rules! frontbox_cache_tests_async {
    ($(#[$attr:meta])* factory: $factory:expr $(,)?) => {
        $crate::__frontbox_cache_suite! {
            run: { asynchronous },
            attr: [$(#[$attr])*],
            factory: $factory,
        }
    };
}

/// Emit the single-flight profile: the cases that only mean anything at `batch_limit = 1`.
///
/// # Why this is a separate list rather than the outbox list re-run
///
/// Re-emitting the outbox cases at a limit of one does not work, and the reason is not arithmetic.
/// **Four of them lose their subject entirely**, so no rewrite recovers them:
///
/// - `case_06_mixed_batch_applies_every_status` — there is no mixed batch.
/// - `case_19_blocked_clears_on_the_next_sync` — `Blocked` means "skipped because an earlier
///   mutation *in the same batch* failed", and at a limit of one there is no same batch.
/// - `case_32_a_repeated_verdict_applies_neither` — the uncontested record it needs alongside the
///   contested one is not in the request.
/// - `case_34_an_unknown_status_retains_and_reports` — it asserts that an unrecognised status "must
///   not spoil the verdicts around it", and there are no verdicts around it.
///
/// They are named here rather than skipped, because four cases quietly absent from a green run is a
/// coverage claim that is not true. **The batched path is what covers them**, and it is the reason
/// `DEFAULT_BATCH_LIMIT` stays at 100: multi-verdict handling in `SyncRunner` is exercised only by
/// those four, so dropping the batched path would delete the coverage rather than inherit it.
///
/// Three more cases — 07, 20 and 33 — assert `sent` or `retained` equal to the number of records
/// seeded. That is right at the default limit and cannot hold here, so
/// [`case_52`](crate::testing::cases::case_52_one_record_per_pass_whatever_the_outcome) states the
/// property they were reaching for against the limit instead of the seed.
///
/// # This profile assumes a retention bound is available
///
/// At a limit of one, a single retained record freezes the whole queue rather than starving a
/// window, so single-flight without a bound is a configuration with no liveness argument at all.
/// [`case_53`](crate::testing::cases::case_53_a_wedged_head_freezes_the_queue_until_the_bound)
/// asserts both halves of that (`wiki/decisions/017-bounded-retention.decision.md`).
///
/// ```ignore
/// frontbox::frontbox_single_flight_tests! {
///     #[test]
///     factory: MyFactory::new(),
///     block_on: pollster::block_on,
/// }
/// ```
#[macro_export]
macro_rules! frontbox_single_flight_tests {
    ($(#[$attr:meta])* factory: $factory:expr, block_on: $block_on:path $(,)?) => {
        $crate::__frontbox_single_flight_suite! {
            run: { block_on: $block_on },
            attr: [$(#[$attr])*],
            factory: $factory,
        }
    };
}

/// Emit the single-flight profile as `async` tests.
///
/// The async counterpart of
/// [`frontbox_single_flight_tests`](crate::frontbox_single_flight_tests); see
/// [`frontbox_conformance_tests_async`](crate::frontbox_conformance_tests_async) for why a wasm
/// backend needs one.
#[macro_export]
macro_rules! frontbox_single_flight_tests_async {
    ($(#[$attr:meta])* factory: $factory:expr $(,)?) => {
        $crate::__frontbox_single_flight_suite! {
            run: { asynchronous },
            attr: [$(#[$attr])*],
            factory: $factory,
        }
    };
}

/// Emit the conformance cases that need fault injection.
///
/// Separate from [`frontbox_conformance_tests`](crate::frontbox_conformance_tests) so that a
/// backend which cannot inject faults leaves a visible gap rather than a silent pass.
#[macro_export]
macro_rules! frontbox_fault_injection_tests {
    ($(#[$attr:meta])* factory: $factory:expr, block_on: $block_on:path $(,)?) => {
        $crate::__frontbox_fault_injection_suite! {
            run: { block_on: $block_on },
            attr: [$(#[$attr])*],
            factory: $factory,
        }
    };
}

/// Emit the fault-injection cases as `async` tests.
///
/// The async counterpart of
/// [`frontbox_fault_injection_tests`](crate::frontbox_fault_injection_tests); see
/// [`frontbox_conformance_tests_async`](crate::frontbox_conformance_tests_async) for why a wasm
/// backend needs one.
#[macro_export]
macro_rules! frontbox_fault_injection_tests_async {
    ($(#[$attr:meta])* factory: $factory:expr $(,)?) => {
        $crate::__frontbox_fault_injection_suite! {
            run: { asynchronous },
            attr: [$(#[$attr])*],
            factory: $factory,
        }
    };
}

/// Emit the read-model row cases as blocking tests.
///
/// Separate from [`frontbox_conformance_tests`](crate::frontbox_conformance_tests) for the reason
/// [`frontbox_cache_tests`](crate::frontbox_cache_tests) is: a backend that has not built decision
/// 032's row store leaves a visible gap in its test file rather than a silent pass.
#[macro_export]
macro_rules! frontbox_row_tests {
    ($(#[$attr:meta])* factory: $factory:expr, block_on: $block_on:path $(,)?) => {
        $crate::__frontbox_row_suite! {
            run: { block_on: $block_on },
            attr: [$(#[$attr])*],
            factory: $factory,
        }
    };
}

/// Emit the read-model row cases as `async` tests.
///
/// The async counterpart of [`frontbox_row_tests`](crate::frontbox_row_tests). An IndexedDB backend
/// needs this one: its `merge_rows` has to read the queue and write the rows inside a single
/// transaction, and a transaction there closes the moment the event loop yields.
#[macro_export]
macro_rules! frontbox_row_tests_async {
    ($(#[$attr:meta])* factory: $factory:expr $(,)?) => {
        $crate::__frontbox_row_suite! {
            run: { asynchronous },
            attr: [$(#[$attr])*],
            factory: $factory,
        }
    };
}

/// Emit the queued-write coalescing cases as blocking tests.
///
/// Separate from [`frontbox_conformance_tests`](crate::frontbox_conformance_tests) for the reason
/// [`frontbox_row_tests`](crate::frontbox_row_tests) is: coalescing is opt-in and adds two required
/// [`OutboxStore`](crate::store::OutboxStore) methods, so a backend that has not ported them leaves
/// a visible gap in its test file instead of failing the whole suite for one missing feature.
#[macro_export]
macro_rules! frontbox_coalescing_tests {
    ($(#[$attr:meta])* factory: $factory:expr, block_on: $block_on:path $(,)?) => {
        $crate::__frontbox_coalescing_suite! {
            run: { block_on: $block_on },
            attr: [$(#[$attr])*],
            factory: $factory,
        }
    };
}

/// Emit the queued-write coalescing cases as `async` tests.
///
/// The async counterpart of [`frontbox_coalescing_tests`](crate::frontbox_coalescing_tests). An
/// IndexedDB backend needs this one: both new methods have to do their read and their write inside
/// a single transaction, and a transaction there closes the moment the event loop yields.
#[macro_export]
macro_rules! frontbox_coalescing_tests_async {
    ($(#[$attr:meta])* factory: $factory:expr $(,)?) => {
        $crate::__frontbox_coalescing_suite! {
            run: { asynchronous },
            attr: [$(#[$attr])*],
            factory: $factory,
        }
    };
}

/// Emit the cases that require a **synchronous** backend.
///
/// There is deliberately no `_async` counterpart, and that absence is the point.
///
/// # Why a case can be backend-shaped rather than backend-neutral
///
/// [`case_30_a_cancelled_pass_does_not_wedge_the_runner`] cancels a pass *mid-flight*, and the only
/// way to get a future into that state is to poll it by hand and drop it. That works when every
/// step before the transport resolves on its first poll — true of the in-memory and SQLite
/// backends, whose storage calls never actually suspend.
///
/// An IndexedDB backend suspends on a browser event before the transport is ever reached, so the
/// first poll is pending for a reason the case cannot distinguish from the one it is testing, and
/// no amount of re-polling helps: the event that would complete it is a *task*, and nothing this
/// crate can await from inside a case yields to the task queue.
///
/// So the case is split out rather than skipped at runtime, exactly as
/// [`frontbox_fault_injection_tests`](crate::frontbox_fault_injection_tests) is. What it covers is
/// [`SyncRunner`](crate::SyncRunner) logic, identical on every backend, so a backend that cannot
/// invoke this loses no coverage of its own behaviour — and the gap is visible in its test file.
///
/// [`case_30_a_cancelled_pass_does_not_wedge_the_runner`]: crate::testing::cases::case_30_a_cancelled_pass_does_not_wedge_the_runner
#[macro_export]
macro_rules! frontbox_blocking_only_tests {
    ($(#[$attr:meta])* factory: $factory:expr, block_on: $block_on:path $(,)?) => {
        $crate::__frontbox_emit_cases! {
            run: { block_on: $block_on },
            attr: [$(#[$attr])*],
            factory: $factory,
            cases: [
                case_30_a_cancelled_pass_does_not_wedge_the_runner,
            ]
        }
    };
}

mod suites;

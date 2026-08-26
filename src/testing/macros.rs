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

/// The one list of conformance cases, shared by the sync and async entry points.
#[doc(hidden)]
#[macro_export]
macro_rules! __frontbox_conformance_suite {
    (run: { $($run:tt)* }, attr: [$($attr:tt)*], factory: $factory:expr $(,)?) => {
        $crate::__frontbox_emit_cases! {
            run: { $($run)* },
            attr: [$($attr)*],
            factory: $factory,
            cases: [
                case_01_applied_is_deleted,
                case_02_duplicate_is_deleted,
                case_03_rejected_becomes_a_dead_letter,
                case_04_blocked_stays_queued,
                case_05_pending_stays_queued,
                case_06_mixed_batch_applies_every_status,
                case_07_offline_leaves_work_untouched,
                case_08_transport_failure_is_an_attempted_send,
                case_10_pending_batch_respects_limit,
                case_11_ordering_is_oldest_first,
                case_12_same_timestamp_orders_stably_by_id,
                case_13_rejected_at_comes_from_the_clock,
                case_14_purge_uses_the_supplied_cutoff,
                case_15_corrupt_record_leaves_the_pending_total,
                case_16_unknown_result_is_an_anomaly,
                case_17_rejection_payload_round_trips,
                case_18_sweep_finds_rows_outcomes_cannot_name,
                case_19_blocked_clears_on_the_next_sync,
                case_20_fully_retained_batch_is_no_progress,
                case_21_pending_prefix_starves_the_tail_observably,
                case_22_scopes_cannot_observe_each_other,
                case_23_empty_scope_key_is_rejected,
                case_24_operation_meta_survives_every_transition,
                case_25_invalid_json_cannot_enter_the_outbox,
                case_26_unrepresentable_timestamp_is_corruption_not_panic,
                case_27_wire_payload_matches_the_source_protocol,
                case_28_reentrant_sync_does_not_double_send,
                case_29_scope_keys_are_not_normalized,
                case_30_a_cancelled_pass_does_not_wedge_the_runner,
                case_31_duplicate_outcomes_are_rejected,
                case_32_a_repeated_verdict_applies_neither,
                case_33_an_omitted_verdict_retains_the_record,
                case_34_an_unknown_status_retains_and_reports,
            ]
        }
    };
}

/// The one list of cache and invalidation cases, shared by the sync and async entry points.
#[doc(hidden)]
#[macro_export]
macro_rules! __frontbox_cache_suite {
    (run: { $($run:tt)* }, attr: [$($attr:tt)*], factory: $factory:expr $(,)?) => {
        $crate::__frontbox_emit_cases! {
            run: { $($run)* },
            attr: [$($attr)*],
            factory: $factory,
            cases: [
                case_35_an_unknown_entity_is_reported_not_applied,
                case_36_reconnect_reports_unknown_names,
                case_37_a_server_behind_local_needs_reset,
                case_38_mark_fresh_clears_staleness_only,
                case_39_staleness_survives_a_reopen,
                case_40_versions_are_scoped,
                case_41_staleness_reports_pending_conflict,
                case_42_a_classifier_narrows_the_conflict,
                case_43_a_stuck_record_does_not_suppress_staleness,
            ]
        }
    };
}

/// The one list of fault-injection cases, shared by the sync and async entry points.
#[doc(hidden)]
#[macro_export]
macro_rules! __frontbox_fault_injection_suite {
    (run: { $($run:tt)* }, attr: [$($attr:tt)*], factory: $factory:expr $(,)?) => {
        $crate::__frontbox_emit_cases! {
            run: { $($run)* },
            attr: [$($attr)*],
            factory: $factory,
            cases: [
                case_09_failed_apply_leaves_no_partial_state,
            ]
        }
    };
}

// Emitted one case at a time rather than with a single `$(...)*` over the case list, because the
// attributes and the case names repeat at different depths and macro_rules cannot nest those.
#[doc(hidden)]
#[macro_export]
macro_rules! __frontbox_emit_cases {
    (
        run: { $($run:tt)* },
        attr: [$($attr:tt)*],
        factory: $factory:expr,
        cases: []
    ) => {};

    (
        run: { block_on: $block_on:path },
        attr: [$($attr:tt)*],
        factory: $factory:expr,
        cases: [$case:ident $(, $rest:ident)* $(,)?]
    ) => {
        $($attr)*
        fn $case() {
            $block_on($crate::testing::cases::$case(&$factory))
                .expect(concat!("conformance case failed: ", stringify!($case)));
        }

        $crate::__frontbox_emit_cases! {
            run: { block_on: $block_on },
            attr: [$($attr)*],
            factory: $factory,
            cases: [$($rest),*]
        }
    };

    (
        run: { asynchronous },
        attr: [$($attr:tt)*],
        factory: $factory:expr,
        cases: [$case:ident $(, $rest:ident)* $(,)?]
    ) => {
        $($attr)*
        async fn $case() {
            $crate::testing::cases::$case(&$factory)
                .await
                .expect(concat!("conformance case failed: ", stringify!($case)));
        }

        $crate::__frontbox_emit_cases! {
            run: { asynchronous },
            attr: [$($attr)*],
            factory: $factory,
            cases: [$($rest),*]
        }
    };
}

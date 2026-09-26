//! The case lists themselves, and the expander every entry point funnels through.
//!
//! Split from `super` for length. The line is the one the `#[doc(hidden)]` attributes already
//! draw: everything there is a documented entry point a backend calls, everything here is
//! machinery those entry points share.
//!
//! Module boundaries are not visibility boundaries for `#[macro_export]`, so every macro below is
//! still `$crate::__frontbox_…` and moving them changed no path. That is also why the split is
//! safe: nothing outside this crate could have been depending on where they lived.

/// The one list of row-store cases, shared by the sync and async entry points.
#[doc(hidden)]
#[macro_export]
macro_rules! __frontbox_row_suite {
    (run: { $($run:tt)* }, attr: [$($attr:tt)*], factory: $factory:expr $(,)?) => {
        $crate::__frontbox_emit_cases! {
            run: { $($run)* },
            attr: [$($attr)*],
            factory: $factory,
            cases: [
                case_62_rows_round_trip_and_stay_scoped,
                case_63_a_merge_skips_rows_with_queued_work,
                case_64_unbound_work_protects_nothing_and_markers_die_with_rows,
            ]
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
                case_11_ordering_is_enqueue_order,
                case_12_same_timestamp_keeps_enqueue_order,
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
                case_31_duplicate_outcomes_are_rejected,
                case_32_a_repeated_verdict_applies_neither,
                case_33_an_omitted_verdict_retains_the_record,
                case_34_an_unknown_status_retains_and_reports,
                case_46_enqueue_order_survives_a_reopen,
                case_47_retention_bound_dead_letters_without_a_rejection,
                case_48_no_verdict_is_not_an_attempt,
                case_49_collision_prone_scope_keys_are_isolated,
                case_50_a_failed_apply_rolls_back_quarantine,
                case_51_trace_context_survives_and_stays_out_of_the_payload,
                case_55_a_precondition_survives_and_stays_out_of_the_payload,
                case_56_the_three_parking_reasons_are_distinguishable,
                case_57_a_drain_empties_what_one_pass_cannot,
                case_58_a_drain_stops_when_a_pass_drains_nothing,
                case_59_an_offline_drain_stops_at_the_first_pass,
                case_65_a_report_names_what_drained,
                case_66_a_retained_record_is_not_drained_and_says_why,
                case_68_terminal_stores_are_ordered_by_time_not_arrival,
                case_69_a_batch_is_not_shortened_by_corruption_ahead_of_it,
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
                case_37_a_differing_identity_is_an_update_not_a_reset,
                case_38_mark_fresh_clears_staleness_only,
                case_39_staleness_survives_a_reopen,
                case_40_versions_are_scoped,
                case_41_staleness_reports_pending_conflict,
                case_42_a_classifier_narrows_the_conflict,
                case_43_a_stuck_record_does_not_suppress_staleness,
                case_44_a_zero_identity_is_distinct_from_no_identity,
                case_67_stale_with_no_version_survives_a_reopen,
            ]
        }
    };
}

/// The one list of single-flight cases, shared by the sync and async entry points.
#[doc(hidden)]
#[macro_export]
macro_rules! __frontbox_single_flight_suite {
    (run: { $($run:tt)* }, attr: [$($attr:tt)*], factory: $factory:expr $(,)?) => {
        $crate::__frontbox_emit_cases! {
            run: { $($run)* },
            attr: [$($attr)*],
            factory: $factory,
            cases: [
                case_52_one_record_per_pass_whatever_the_outcome,
                case_53_a_wedged_head_freezes_the_queue_until_the_bound,
                case_54_records_reach_the_server_one_at_a_time_in_order,
                case_60_a_drain_clears_a_single_flight_backlog,
                case_61_two_handles_on_one_scope_do_not_drain_together,
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

/// The one list of queued-write coalescing cases, shared by the sync and async entry points.
///
/// Its own suite rather than lines in the conformance list, because coalescing is an opt-in feature
/// with two new required store methods: a backend that has not ported them says so by not invoking
/// this, instead of failing forty unrelated cases.
#[doc(hidden)]
#[macro_export]
macro_rules! __frontbox_coalescing_suite {
    (run: { $($run:tt)* }, attr: [$($attr:tt)*], factory: $factory:expr $(,)?) => {
        $crate::__frontbox_emit_cases! {
            run: { $($run)* },
            attr: [$($attr)*],
            factory: $factory,
            cases: [
                case_70_a_second_edit_replaces_the_first_body,
                case_71_a_replacement_keeps_the_slot_and_takes_the_content,
                case_72_a_record_read_for_sending_is_not_coalescible,
                case_73_append_if_missing_never_drops_a_write,
                case_74_require_existing_names_its_refusals,
                case_75_matching_needs_row_method_and_path,
                case_76_coalescing_cannot_reach_another_scope,
                case_77_a_replaced_record_drains_once_with_the_newer_body,
                case_78_an_offline_probe_reads_nothing_and_preserves_coalescing,
                case_79_offline_during_a_send_still_spends_coalescibility,
                case_80_a_retained_verdict_spends_coalescibility,
                case_81_a_failed_transport_spends_coalescibility,
                case_82_an_omitted_verdict_leaves_the_record_spent,
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

/// The shared list of atomic storage migration cases.
#[doc(hidden)]
#[macro_export]
macro_rules! __frontbox_migration_suite {
    (run: { $($run:tt)* }, attr: [$($attr:tt)*], factory: $factory:expr $(,)?) => {
        $crate::__frontbox_emit_cases! {
            run: { $($run)* }, attr: [$($attr)*], factory: $factory,
            cases: [
                case_83_migration_preserves_order_and_audit,
                case_84_migration_failure_rolls_back_every_store,
                case_85_migration_is_scoped_and_keeps_quarantine,
                case_86_migration_rekeys_without_reordering,
                case_87_migration_and_drain_share_exclusion,
                case_88_migration_rejects_unknown_and_corrupt_records,
                case_89_migration_write_failure_rolls_back,
                case_90_migration_keeps_unacknowledged_transport_start,
            ]
        }
    };
}

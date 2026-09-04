//! What the two terminal records promise, and the invariant `from_raw` exists to hold.
//!
//! A file of its own for the reason `src/rfc3339/tests.rs` is: the module is at the line cap
//! `AGENTS.md` sets, and a test block is the part that can move without a public path moving with
//! it.

use super::*;
use crate::record::MutationIntent;

fn scope() -> ScopeKey {
    ScopeKey::new("user:alice@tenant:acme").expect("valid")
}

fn refusal() -> RemoteRejection {
    RemoteRejection::new("title must not be empty").with_code("invalid_input")
}

/// `rejection()` answers one question — *did a server refuse this and say why* — and its `None`
/// covers three different facts.
///
/// This is the convenience the type documents as lossy, and the test exists to keep it lossy in
/// exactly the documented way: a caller that needs to tell a refusal-without-payload from a
/// retention bound from a caller parking must match on `reason` instead. If `rejection()` ever
/// started distinguishing them, the doc comment would be wrong rather than the code better.
#[test]
fn a_rejection_payload_is_reachable_only_for_a_server_refusal_that_explained_itself() {
    let parked = |reason| {
        DeadLetterRecord::from_record(
            OutboxRecord::stamp(
                MutationIntent::new(
                    MutationId::from_uuid(uuid::Uuid::from_u128(1)),
                    "POST",
                    "/api/v1/things",
                    serde_json::json!({}),
                    1,
                ),
                scope(),
                1,
            ),
            7,
            reason,
        )
    };

    assert_eq!(
        parked(DeadLetterReason::Rejected {
            error: Some(refusal())
        })
        .rejection(),
        Some(&refusal())
    );

    for silent in [
        DeadLetterReason::Rejected { error: None },
        DeadLetterReason::RetentionBound,
        DeadLetterReason::Caller("watchdog".into()),
    ] {
        assert_eq!(
            parked(silent.clone()).rejection(),
            None,
            "{silent:?} carries no server payload, and `rejection` must not invent one"
        );
    }
}

/// The transition carries the envelope across, and stamps the parking with the injected clock.
///
/// `attempts` is carried rather than recomputed, which is what makes the count on a dead letter
/// the number of attempts that were actually made — no off-by-one to explain to whoever reads
/// it later (`wiki/decisions/017-bounded-retention.decision.md`).
#[test]
fn moving_a_record_carries_the_envelope_and_stamps_the_parking() {
    let mut record = OutboxRecord::stamp(
        MutationIntent::new(
            MutationId::from_uuid(uuid::Uuid::from_u128(2)),
            "PUT",
            "/api/v1/things/2",
            serde_json::json!({ "name": "Morning" }),
            1_700_000_000_000,
        )
        .with_op(OperationMeta::new("rename").with_version("1.2.0"))
        .with_traceparent("00-4bf9-00f0-01")
        .with_precondition("9f2c8a1e"),
        scope(),
        5,
    );
    record.attempts = 3;

    let parked =
        DeadLetterRecord::from_record(record, 1_700_000_009_999, DeadLetterReason::RetentionBound);

    assert_eq!(parked.method, "PUT");
    assert_eq!(parked.path, "/api/v1/things/2");
    assert_eq!(parked.body, serde_json::json!({ "name": "Morning" }));
    assert_eq!(parked.created_at, 1_700_000_000_000);
    assert_eq!(
        parked.op.as_ref().map(|op| op.name.as_str()),
        Some("rename")
    );
    assert_eq!(parked.traceparent.as_deref(), Some("00-4bf9-00f0-01"));
    assert_eq!(
        parked.precondition.as_deref(),
        Some("9f2c8a1e"),
        "requeueing without it would re-run the unchecked write it exists to prevent"
    );
    assert_eq!(parked.scope, scope());
    assert_eq!(parked.attempts, 3, "carried, not recounted");
    assert_eq!(parked.rejected_at, 1_700_000_009_999);
}

/// **The invariant `from_raw` exists to hold.** `mutation_id` is derived from
/// `raw_mutation_id`, never taken alongside it.
///
/// An entry whose parsed and raw identifiers disagreed would be a lie no caller could detect,
/// and the constructor's signature is what makes that unrepresentable for an out-of-tree
/// backend. Both halves are asserted: a readable id parses through, and an unreadable one stays
/// `None` while the raw text survives for a human to read.
#[test]
fn a_quarantined_identifier_is_derived_from_the_raw_text_never_taken_beside_it() {
    let readable = QuarantinedRecord::from_raw(
        "00000000-0000-0000-0000-000000000009",
        "POST",
        "/api/v1/things",
        "{ not json",
        1,
        scope(),
        "body is not valid JSON",
        42,
    );
    assert_eq!(
        readable.mutation_id,
        Some(MutationId::from_uuid(uuid::Uuid::from_u128(9)))
    );
    assert_eq!(
        readable.raw_mutation_id,
        "00000000-0000-0000-0000-000000000009"
    );
    assert_eq!(readable.raw_body, "{ not json", "kept exactly as stored");
    assert_eq!(readable.quarantined_at, 42);
    assert!(readable.op.is_none(), "nothing is invented for it");

    // The id-less path: only `sweep_corrupt` can reach this row, because no `Outcome` can name
    // it (`wiki/decisions/006-corrupt-record-policy.decision.md`).
    let unreadable = QuarantinedRecord::from_raw(
        "not-a-uuid",
        "POST",
        "/api/v1/things",
        r#"{"ok":true}"#,
        1,
        scope(),
        "unparseable mutation_id",
        42,
    );
    assert_eq!(unreadable.mutation_id, None);
    assert_eq!(
        unreadable.raw_mutation_id, "not-a-uuid",
        "the raw text survives, or the row is undiagnosable"
    );
}

/// `with_op` is how a backend carries the caller's label across a transition core did not make.
#[test]
fn a_quarantined_row_can_carry_the_operation_label() {
    let op = OperationMeta::new("start_session").with_version("1.2.0");
    let record = QuarantinedRecord::from_raw(
        "not-a-uuid",
        "POST",
        "/api/v1/sessions",
        "{}",
        1,
        scope(),
        "unparseable mutation_id",
        42,
    )
    .with_op(op.clone());

    assert_eq!(record.op.as_ref(), Some(&op));
}

/// The reason round-trips, `Rejected`'s optional payload included.
///
/// A durable backend stores this as a discriminant plus a payload, and both SQLite and
/// IndexedDB rebuild it from those two columns. The serde round trip is the shape they are
/// mapping to, so a change here is a change to two schemas.
#[test]
fn every_parking_reason_round_trips() {
    for reason in [
        DeadLetterReason::Rejected {
            error: Some(refusal()),
        },
        DeadLetterReason::Rejected { error: None },
        DeadLetterReason::RetentionBound,
        DeadLetterReason::Caller("watchdog".into()),
        DeadLetterReason::Caller(String::new()),
    ] {
        let json = serde_json::to_string(&reason).expect("serializes");
        let back: DeadLetterReason = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back, reason, "round trip failed for {json}");
    }
}

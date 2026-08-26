//! The wire-format oracle, ported from the source system's `frontend/dto.rs`.
//!
//! These are genuine oracles: they assert behaviour the source and this crate *share*, and they
//! fail if this crate drifts. `wiki/decisions/010-batch-wire-format.decision.md` commits the batch
//! payload to what the source server already accepts, which makes a transport for that server a
//! passthrough rather than a mapping layer — a commitment only a test can keep.
//!
//! Conformance case 27 asserts the exact key set and the exact `client_datetime` string from
//! inside the suite. These come at it from outside, through the public API, in the shape the
//! source's own tests used.
//!
//! Split from `tests/source_oracle.rs`, which ports `persistence/mutations.rs` and records the
//! deliberate divergences from it. The two files are separate test binaries and share nothing but
//! the crate under test.

use frontbox::{
    Clock, ManualClock, MutationBatchRequest, MutationBatchResponse, MutationId, MutationIntent,
    MutationResult, MutationStatus,
};

fn id(n: u128) -> MutationId {
    MutationId::from_uuid(uuid::Uuid::from_u128(n))
}

/// Source: `test_mutation_id_serialization_roundtrip`.
#[test]
fn mutation_id_round_trips() {
    let original = id(0x1234_5678);
    let json = serde_json::to_string(&original).expect("serialize");
    let back: MutationId = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(original, back);
    assert_eq!(
        json,
        format!("\"{original}\""),
        "serialized as a bare string"
    );
}

/// Source: `test_mutation_intent_roundtrip`.
#[test]
fn mutation_intent_round_trips() {
    let intent = MutationIntent::new(
        id(1),
        "PATCH",
        "/api/v1/exercises/123",
        serde_json::json!({ "name": "Bench Press" }),
        1_700_000_000_000,
    );

    let json = serde_json::to_string(&intent).expect("serialize");
    let back: MutationIntent = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(back.mutation_id, id(1));
    assert_eq!(back.method, "PATCH");
    assert_eq!(back.path, "/api/v1/exercises/123");
    assert_eq!(back.body, serde_json::json!({ "name": "Bench Press" }));
    assert_eq!(back.created_at, 1_700_000_000_000);
}

/// Source: `test_mutation_batch_request_and_response_roundtrip`.
#[test]
fn batch_request_and_response_round_trip() {
    let request = MutationBatchRequest::new(vec![MutationIntent::new(
        id(1),
        "PUT",
        "/api/v1/me/preferences",
        serde_json::json!({ "default_rest_seconds": 120 }),
        1_700_000_000_000,
    )]);
    let json = serde_json::to_string(&request).expect("serialize");
    let back: MutationBatchRequest = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(request, back);

    let response =
        MutationBatchResponse::new(vec![MutationResult::new(id(1), MutationStatus::Applied)]);
    let json = serde_json::to_string(&response).expect("serialize");
    let back: MutationBatchResponse = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(response, back);
}

/// Source: `test_mutation_batch_request_new_sets_client_timestamp`.
///
/// The source asserts `client_datetime >= before`, which is all you can assert when the constructor
/// calls `Utc::now()` itself. Here the timestamp is a parameter, so the assertion is exact — and
/// `Utc::now()` does not compile in this crate, because `chrono` is depended on without its `clock`
/// feature precisely to keep it that way.
#[test]
fn the_client_timestamp_is_supplied_not_captured() {
    let clock = ManualClock::new(1_700_000_000_123);
    let intent = MutationIntent::new(
        id(1),
        "POST",
        "/api/v1/things",
        serde_json::json!({ "name": "Morning Workout" }),
        clock.now_ms(),
    );

    assert_eq!(intent.created_at, 1_700_000_000_123);

    let value = serde_json::to_value(&intent).expect("serialize");
    assert_eq!(
        value["client_datetime"],
        serde_json::json!("2023-11-14T22:13:20.123Z")
    );
}

/// A server refusal deserializes from the source's `ApiError` shape unchanged.
///
/// The source sends `{ "code": ..., "message": ... }`. This crate's payload adds an optional
/// `details`, and makes `code` optional so a server that refuses without one is still usable.
#[test]
fn the_source_api_error_shape_still_deserializes() {
    let response: MutationBatchResponse = serde_json::from_str(
        r#"{"results":[{"mutation_id":"00000000-0000-0000-0000-000000000001",
             "status":"Rejected",
             "error":{"code":"invalid_input","message":"Title is required"}}]}"#,
    )
    .expect("deserialize the source's response shape");

    let error = response.results[0].error.as_ref().expect("error");
    assert_eq!(error.code.as_deref(), Some("invalid_input"));
    assert_eq!(error.message, "Title is required");
    assert_eq!(error.details, None);
}

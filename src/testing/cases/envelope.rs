//! Cases 24, 25, 27, 51, 55: what an envelope carries and what it serializes to.

use super::prelude::*;

/// Operation metadata survives every transition, and core never reads it.
pub async fn case_24_operation_meta_survives_every_transition<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let op = OperationMeta::new("start_session").with_version("1.2.0");

    // Into a dead letter, on a server refusal.
    let (store, _) = open(factory).await?;
    store.enqueue(intent(1, 1).with_op(op.clone())).await?;

    let runner = runner(store, Reply::All(MutationStatus::Rejected));
    runner.sync_once().await?;

    let dead = DeadLetterStore::list(runner.store(), 10).await?;
    assert_eq!(dead[0].op.as_ref(), Some(&op));

    // And into quarantine, on a local integrity failure.
    let (other, _) = open(factory).await?;
    other.enqueue(intent(2, 2).with_op(op.clone())).await?;
    other
        .apply_outcomes(&[Outcome::new(
            id(2),
            Disposition::Quarantine {
                reason: "manufactured".to_string(),
            },
        )])
        .await?;

    let quarantined = QuarantineStore::list(&other, 10).await?;
    let carried = quarantined
        .iter()
        .find(|r| r.mutation_id == Some(id(2)))
        .expect("quarantined record");
    assert_eq!(carried.op.as_ref(), Some(&op));
    Ok(())
}

/// A body that is not valid JSON cannot enter the outbox, and every body that is survives exactly.
///
/// # The first half is a type, not a check
///
/// There is no code path to reject a malformed body, because [`MutationIntent`] holds a parsed
/// [`serde_json::Value`] and a malformed body is not a value that exists. The source stores
/// pre-serialized text, so a bad body is only discovered at replay, after a reconnect, as a
/// corrupt-record case. This narrows the corrupt-record surface; it does not remove it — durable
/// corruption still happens after a successful write, which is what the sweep is for.
///
/// # The second half is what a backend can actually get wrong
///
/// An earlier version of this case stopped after asserting that `serde_json` rejects `{ not json`,
/// which is a test of `serde_json`. What it did not test is the claim the type makes: that a body
/// which *did* parse comes back **identical**. Every backend serializes the value to store it and
/// parses it back to return it, and that round trip is where a body changes shape.
///
/// The shapes below are chosen to break a naive one. A large `i64` is the sharp one: every
/// JavaScript number is an `f64`, so a value above 2^53 silently loses precision in any backend
/// that round-trips through a JS number — the finding that produced `i64_text` in the IndexedDB
/// backend's converter. The rest cover the assumptions a backend makes without noticing: that the
/// body is an object, that it is not `null`, that strings are ASCII, and that `1.0` and `1` are the
/// same thing.
pub async fn case_25_invalid_json_cannot_enter_the_outbox<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;

    let parsed: Result<serde_json::Value, _> = serde_json::from_str("{ not json");
    assert!(parsed.is_err(), "the malformed body must fail to parse");

    // The failure happens before an intent exists, so nothing reached storage.
    assert_eq!(store.pending_count().await?, 0);

    let bodies = [
        serde_json::json!({ "ok": true }),
        // Above 2^53. An f64 round trip renders this as 9007199254740993 or 9007199254740992.
        serde_json::json!({ "id": 9_007_199_254_740_993_i64 }),
        serde_json::json!({ "id": -9_007_199_254_740_993_i64 }),
        // A float that is not an integer, beside one that is: a backend that normalises numbers
        // turns `1.0` into `1` and the two stop being distinguishable.
        serde_json::json!({ "a": 1.5, "b": 1.0, "c": 1 }),
        serde_json::json!({ "text": "quote \" newline \n emoji 🧊 nul-ish \u{7f}" }),
        // Not an object at the top level, and empty containers, which a backend that assumes a map
        // will mishandle.
        serde_json::json!([1, 2, 3]),
        serde_json::json!({}),
        serde_json::json!([]),
        serde_json::Value::Null,
    ];

    for (offset, body) in bodies.iter().enumerate() {
        let mutation = id(offset as u128 + 1);
        store
            .enqueue(MutationIntent::new(
                mutation,
                "POST",
                "/api/v1/things",
                body.clone(),
                offset as i64 + 1,
            ))
            .await?;

        let queued = store.pending_batch(bodies.len()).await?;
        let stored = queued
            .iter()
            .find(|record| record.mutation_id == mutation)
            .expect("the record just enqueued must come back");
        assert_eq!(
            &stored.body, body,
            "a body that parsed must come back identical, not merely equivalent"
        );
    }

    assert_eq!(store.pending_count().await?, bodies.len());
    Ok(())
}

/// The wire payload is exactly the shape the existing server already accepts.
///
/// Field-for-field: an intent with no operation metadata serializes to the five keys the source
/// protocol defines, with the client timestamp as an RFC 3339 instant. A server that rejects
/// unknown keys must see nothing new until a caller opts into `op`.
pub async fn case_27_wire_payload_matches_the_source_protocol<F: StoreFactory>(
    _factory: &F,
) -> Result<(), Error> {
    let bare = MutationIntent::new(
        id(1),
        "PATCH",
        "/api/v1/exercises/123",
        serde_json::json!({ "name": "Bench Press" }),
        1_700_000_000_123,
    );

    let value = serde_json::to_value(&bare).map_err(Error::serialization)?;
    let object = value.as_object().expect("intent serializes to an object");

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["body", "client_datetime", "method", "mutation_id", "path"],
        "an intent without operation metadata must carry no extra keys"
    );
    assert_eq!(
        object["client_datetime"],
        serde_json::json!("2023-11-14T22:13:20.123Z")
    );

    // And it round-trips.
    let back: MutationIntent = serde_json::from_value(value).map_err(Error::serialization)?;
    assert_eq!(back, bare);

    // Opting in adds the key, and only then.
    let labelled = bare.with_op(OperationMeta::new("rename_exercise"));
    let value = serde_json::to_value(&labelled).map_err(Error::serialization)?;
    assert!(value.as_object().expect("object").contains_key("op"));
    Ok(())
}

/// Trace context survives every transition, stays out of the payload, and is never invented.
///
/// Three properties in one case, because they only mean anything together.
///
/// **It is durable.** The interval worth measuring for an outbox is enqueue-to-send, which for an
/// offline queue is measured in days and spans restarts. A `tracing` span cannot model that — a
/// span is an in-memory, in-process object — so the context has to be stored and replayed rather
/// than created at drain (`wiki/decisions/022-durable-trace-context.decision.md`).
///
/// **It is not in the body.** Trace context travels as a header. Serializing it into the payload
/// would put it in the wrong place and change the wire shape decision 010 fixed, so
/// `MutationIntent` marks it `#[serde(skip)]` and case 27 still sees exactly five keys.
///
/// **Core does not mint one.** The caller supplies it, exactly as it supplies a `MutationId`, and
/// for the same reason: a W3C `traceparent` is mostly randomness and core generates none. An
/// application already inside a span when the user acts holds the *correct* parent; a fresh root
/// minted here would discard the link to the user action, which is the link worth keeping.
pub async fn case_51_trace_context_survives_and_stays_out_of_the_payload<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let traceparent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

    let (store, _) = open(factory).await?;
    store
        .enqueue(intent(1, 1).with_traceparent(traceparent))
        .await?;

    // Durable: it comes back off the store, and replays onto the intent the transport will send.
    let record = store.pending_batch(10).await?.remove(0);
    assert_eq!(record.traceparent.as_deref(), Some(traceparent));
    assert_eq!(
        record.to_intent().traceparent.as_deref(),
        Some(traceparent),
        "the transport reads it off the intent to set the header"
    );

    // Absent from the payload, so the wire shape is unchanged.
    let payload = serde_json::to_value(record.to_intent()).expect("serializes");
    assert!(
        payload.get("traceparent").is_none(),
        "trace context is a header, not a body field: {payload}"
    );

    // Carried across the transition, which is the one moment anybody goes looking.
    let runner = runner(store, Reply::All(MutationStatus::Rejected));
    runner.sync_once().await?;
    let parked = DeadLetterStore::list(runner.store(), 10).await?;
    assert_eq!(parked[0].traceparent.as_deref(), Some(traceparent));

    // And a caller that supplies nothing gets nothing invented for it.
    let (plain, _) = open(factory).await?;
    plain.enqueue(intent(2, 2)).await?;
    assert_eq!(plain.pending_batch(10).await?[0].traceparent, None);
    Ok(())
}

/// A precondition survives every transition, stays out of the payload, and is never invented.
///
/// Case 51's shape, for a value with a sharper failure mode. Trace context going missing degrades
/// diagnostics; **a precondition going missing disables conflict detection with no symptom at
/// all** — the write succeeds, a concurrent edit is clobbered, and nothing reports it. That
/// asymmetry is why it is durable on the record rather than recomputed at drain or held beside the
/// outbox in application storage (`wiki/decisions/026-replayable-preconditions.decision.md`).
///
/// It is opaque on purpose. The two values here are the two forms one real server uses — a base
/// hash for an update, and a wildcard for a create — and core cannot tell them apart because a
/// `PUT` does not reveal which a mutation is. Only the application does.
pub async fn case_55_a_precondition_survives_and_stays_out_of_the_payload<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let update = "9f2c8a1e";
    let create = "*";

    let (store, _) = open(factory).await?;
    store
        .enqueue(intent(1, 1).with_precondition(update))
        .await?;
    store
        .enqueue(intent(2, 2).with_precondition(create))
        .await?;

    // Durable, and replayed onto the intent the transport will send.
    let batch = store.pending_batch(10).await?;
    assert_eq!(batch[0].precondition.as_deref(), Some(update));
    assert_eq!(
        batch[1].precondition.as_deref(),
        Some(create),
        "core stores both forms without distinguishing them"
    );
    assert_eq!(batch[0].to_intent().precondition.as_deref(), Some(update));

    // Absent from the payload, so the wire shape decision 010 fixed is unchanged.
    let payload = serde_json::to_value(batch[0].to_intent()).expect("serializes");
    assert!(
        payload.get("precondition").is_none(),
        "a precondition is a header, not a body field: {payload}"
    );

    // Carried across the transition. A dead letter that failed on a conflict has to say what state
    // it expected, or requeueing it re-runs the unchecked write.
    let runner = runner(store, Reply::All(MutationStatus::Rejected));
    runner.sync_once().await?;
    let mut parked = DeadLetterStore::list(runner.store(), 10).await?;
    parked.sort_by_key(|record| record.mutation_id);
    assert_eq!(parked[0].precondition.as_deref(), Some(update));
    assert_eq!(parked[1].precondition.as_deref(), Some(create));

    // And a caller that supplies nothing gets nothing invented for it.
    let plain = factory.open(scope("user:noprecond@tenant:acme")).await?;
    plain.enqueue(intent(3, 3)).await?;
    assert_eq!(plain.pending_batch(10).await?[0].precondition, None);
    Ok(())
}

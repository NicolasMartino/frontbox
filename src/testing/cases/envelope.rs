//! Cases 24, 25, 27: what an envelope carries and what it serializes to.

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

/// A body that is not valid JSON cannot enter the outbox at all.
///
/// Not "is rejected at enqueue" — there is no code path to reject, because
/// [`MutationIntent`] holds a parsed [`serde_json::Value`] and a malformed body is not a value that
/// exists. The source stores pre-serialized text, so a bad body is only discovered at replay, after
/// a reconnect, as a corrupt-record case.
///
/// This narrows the corrupt-record surface; it does not remove it. Durable corruption still happens
/// after a successful write, which is what the sweep is for.
pub async fn case_25_invalid_json_cannot_enter_the_outbox<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;

    let parsed: Result<serde_json::Value, _> = serde_json::from_str("{ not json");
    assert!(parsed.is_err(), "the malformed body must fail to parse");

    // The failure happens before an intent exists, so nothing reached storage.
    assert_eq!(store.pending_count().await?, 0);

    // The well-formed path is the only one available.
    store
        .enqueue(MutationIntent::new(
            id(1),
            "POST",
            "/api/v1/things",
            parsed.unwrap_or(serde_json::json!({ "ok": true })),
            1,
        ))
        .await?;
    assert_eq!(store.pending_count().await?, 1);
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

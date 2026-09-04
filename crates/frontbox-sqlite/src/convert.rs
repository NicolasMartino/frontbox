//! Turning stored columns into records, and back.
//!
//! The decode is fallible in exactly the three ways a row can be unusable, and each one is a
//! quarantine case rather than an error: an unparseable identifier, a body that is not JSON, and a
//! timestamp the wire format cannot express. Reads skip such rows and only `sweep_corrupt`
//! surfaces them, which is the contract `OutboxStore` states and
//! `wiki/decisions/006-corrupt-record-policy.decision.md` explains.

use frontbox::{
    DeadLetterReason, DeadLetterRecord, MutationId, MutationIntent, OperationMeta, OutboxRecord,
    QuarantinedRecord, RowRef, ScopeKey,
};
use rusqlite::Row;

/// One outbox row exactly as stored, before anything is trusted.
pub(crate) struct RawRow {
    pub seq: u64,
    pub raw_mutation_id: String,
    pub method: String,
    pub path: String,
    pub raw_body: String,
    pub created_at: i64,
    pub op: Option<OperationMeta>,
    pub traceparent: Option<String>,
    pub precondition: Option<String>,
    pub row: Option<RowRef>,
    pub attempts: u32,
    pub last_error: Option<String>,
}

/// Columns the outbox `SELECT`s share, so the reader and the query cannot drift apart.
pub(crate) const OUTBOX_COLUMNS: &str = "seq, mutation_id, method, path, raw_body, created_at, \
     op_name, op_version, traceparent, precondition, row_entity, row_id, attempts, last_error";

pub(crate) fn read_raw(row: &Row<'_>) -> rusqlite::Result<RawRow> {
    let op_name: Option<String> = row.get(6)?;
    let op_version: Option<String> = row.get(7)?;
    let row_entity: Option<String> = row.get(10)?;
    let row_id: Option<String> = row.get(11)?;
    Ok(RawRow {
        seq: row.get::<_, i64>(0)? as u64,
        raw_mutation_id: row.get(1)?,
        method: row.get(2)?,
        path: row.get(3)?,
        raw_body: row.get(4)?,
        created_at: row.get(5)?,
        op: op_name.map(|name| {
            let meta = OperationMeta::new(name);
            match op_version {
                Some(version) => meta.with_version(version),
                None => meta,
            }
        }),
        traceparent: row.get(8)?,
        precondition: row.get(9)?,
        // Both halves or neither. A row with one is a row this backend wrote wrong, and treating it
        // as unbound is the reading that cannot corrupt a merge.
        row: match (row_entity, row_id) {
            (Some(entity), Some(id)) => Some(RowRef::new(entity, id)),
            _ => None,
        },
        attempts: row.get::<_, i64>(12)? as u32,
        last_error: row.get(13)?,
    })
}

/// Build the record a caller sees, or say why the row is unusable.
pub(crate) fn decode(raw: &RawRow, scope: &ScopeKey) -> Result<OutboxRecord, String> {
    let mutation_id: MutationId = raw
        .raw_mutation_id
        .parse()
        .map_err(|_| format!("unparseable mutation_id: {:?}", raw.raw_mutation_id))?;

    let body: serde_json::Value = serde_json::from_str(&raw.raw_body)
        .map_err(|failure| format!("body is not valid JSON: {failure}"))?;

    // The wire format renders this as an RFC 3339 instant, so a value it cannot express is a record
    // that could never be sent. Caught here, it is a quarantine case rather than a serialization
    // failure discovered mid-batch. The in-memory backend makes the same check, and the two must
    // agree or the suite is testing two different contracts.
    if frontbox::timestamp_is_representable(raw.created_at) {
        let mut intent = MutationIntent::new(
            mutation_id,
            raw.method.clone(),
            raw.path.clone(),
            body,
            raw.created_at,
        );
        if let Some(op) = raw.op.clone() {
            intent = intent.with_op(op);
        }
        if let Some(traceparent) = raw.traceparent.clone() {
            intent = intent.with_traceparent(traceparent);
        }
        if let Some(precondition) = raw.precondition.clone() {
            intent = intent.with_precondition(precondition);
        }
        if let Some(bound) = raw.row.clone() {
            intent = intent.with_row(bound);
        }
        let mut record = OutboxRecord::stamp(intent, scope.clone(), raw.seq);
        record.attempts = raw.attempts;
        record.last_error = raw.last_error.clone();
        Ok(record)
    } else {
        Err(format!(
            "created_at outside representable range: {}",
            raw.created_at
        ))
    }
}

/// Move an unusable row into the quarantine shape.
pub(crate) fn quarantine_from(
    raw: &RawRow,
    scope: &ScopeKey,
    reason: String,
    quarantined_at: i64,
) -> QuarantinedRecord {
    let record = QuarantinedRecord::from_raw(
        raw.raw_mutation_id.clone(),
        raw.method.clone(),
        raw.path.clone(),
        raw.raw_body.clone(),
        raw.created_at,
        scope.clone(),
        reason,
        quarantined_at,
    );
    match raw.op.clone() {
        Some(op) => record.with_op(op),
        None => record,
    }
}

/// How a dead letter's reason is split across two columns.
///
/// A discriminant plus an optional payload, rather than one serialized blob, so the reason is
/// queryable and a human reading the table can see which kind it was without parsing JSON —
/// which is the whole complaint decision 027 fixed at the type level.
pub(crate) fn reason_columns(reason: &DeadLetterReason) -> (&'static str, Option<String>) {
    match reason {
        DeadLetterReason::Rejected { error } => (
            "rejected",
            error
                .as_ref()
                .and_then(|error| serde_json::to_string(error).ok()),
        ),
        DeadLetterReason::RetentionBound => ("retention_bound", None),
        DeadLetterReason::Caller(reason) => ("caller", Some(reason.clone())),
        // `DeadLetterReason` is `#[non_exhaustive]`, so a variant added later reaches here. Stored
        // under a discriminant that says so rather than silently flattened into an existing one:
        // a reason this backend cannot name must not come back as a reason it can.
        other => ("unknown", Some(format!("{other:?}"))),
    }
}

/// Rebuild a reason from its two columns.
///
/// An unrecognised discriminant becomes a caller reason carrying the raw text rather than a panic
/// or a silent `RetentionBound`: this is a row a *newer* version wrote, and inventing a retention
/// bound would put words in a server's mouth exactly as decision 017 refuses.
pub(crate) fn reason_from(kind: &str, body: Option<String>) -> DeadLetterReason {
    match kind {
        "rejected" => DeadLetterReason::Rejected {
            error: body.and_then(|body| serde_json::from_str(&body).ok()),
        },
        "retention_bound" => DeadLetterReason::RetentionBound,
        "caller" => DeadLetterReason::Caller(body.unwrap_or_default()),
        other => DeadLetterReason::Caller(format!("unrecognised reason {other:?}")),
    }
}

/// A stored dead letter that will not parse back.
///
/// # Why not `rusqlite::Error::InvalidQuery`
///
/// That is what this returned, and it names the wrong thing. `InvalidQuery` means *the SQL was
/// wrong* — an operator reading it goes and looks at the statement, which is fine. The actual
/// condition is that a row this backend wrote does not read back: the identifier does not parse, or
/// the body is not JSON. The statement is correct and the data is not.
///
/// The mapping has to land in `rusqlite::Error` because `read_dead_letter` is a `query_map`
/// callback, so the message is where the truth goes. `FromSqlConversionFailure` is the variant that
/// means what happened, and it carries the underlying cause rather than discarding it.
///
/// # Why the cause is rendered rather than boxed as-is
///
/// `FromSqlConversionFailure` holds a `Box<dyn Error + Send + Sync>`, so taking the cause by that
/// bound would write `Send` into this crate's source — and `scripts/verify.sh`'s `no Send bound`
/// gate would go red, correctly. Decision 001's rule is about futures rather than about a
/// synchronous string conversion, but the gate is deliberately textual and a red one is meant to be
/// read, not exempted.
///
/// Rendering to a `String` at the boundary is the better shape anyway: it asks the caller for
/// [`Display`](std::fmt::Display) and nothing more, and the concrete error boxed below satisfies
/// `Send + Sync` by construction without either word appearing here.
fn stored_is_corrupt(cause: impl std::fmt::Display) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(CorruptStoredValue(cause.to_string())),
    )
}

/// A stored value this backend wrote that will not read back.
#[derive(Debug)]
struct CorruptStoredValue(String);

impl std::fmt::Display for CorruptStoredValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "stored value is corrupt: {}", self.0)
    }
}

impl std::error::Error for CorruptStoredValue {}

/// Read one dead letter back.
pub(crate) fn read_dead_letter(
    row: &Row<'_>,
    scope: &ScopeKey,
) -> rusqlite::Result<DeadLetterRecord> {
    let op_name: Option<String> = row.get(5)?;
    let op_version: Option<String> = row.get(6)?;
    let kind: String = row.get(11)?;
    let reason_body: Option<String> = row.get(12)?;

    // Rebuilt through `stamp` and `from_record` rather than a struct literal, because decision
    // 008's `#[non_exhaustive]` blocks literals outside core. The `seq` is not stored on a dead
    // letter and is not part of its identity — it ordered the record while it was queued, and the
    // queue is what it has left.
    let mut intent = MutationIntent::new(
        row.get::<_, String>(0)?
            .parse::<MutationId>()
            .map_err(stored_is_corrupt)?,
        row.get::<_, String>(1)?,
        row.get::<_, String>(2)?,
        serde_json::from_str(&row.get::<_, String>(3)?).map_err(stored_is_corrupt)?,
        row.get(4)?,
    );
    if let Some(name) = op_name {
        let meta = OperationMeta::new(name);
        intent = intent.with_op(match op_version {
            Some(version) => meta.with_version(version),
            None => meta,
        });
    }
    if let Some(traceparent) = row.get::<_, Option<String>>(7)? {
        intent = intent.with_traceparent(traceparent);
    }
    if let Some(precondition) = row.get::<_, Option<String>>(8)? {
        intent = intent.with_precondition(precondition);
    }
    let mut record = OutboxRecord::stamp(intent, scope.clone(), 0);
    record.attempts = row.get::<_, i64>(9)? as u32;
    Ok(DeadLetterRecord::from_record(
        record,
        row.get(10)?,
        reason_from(&kind, reason_body),
    ))
}

/// Read one quarantined row back.
pub(crate) fn read_quarantined(
    row: &Row<'_>,
    scope: &ScopeKey,
) -> rusqlite::Result<QuarantinedRecord> {
    let op_name: Option<String> = row.get(5)?;
    let op_version: Option<String> = row.get(6)?;
    let record = QuarantinedRecord::from_raw(
        row.get::<_, String>(0)?,
        row.get::<_, String>(1)?,
        row.get::<_, String>(2)?,
        row.get::<_, String>(3)?,
        row.get(4)?,
        scope.clone(),
        row.get::<_, String>(7)?,
        row.get(8)?,
    );
    Ok(match op_name {
        Some(name) => {
            let meta = OperationMeta::new(name);
            record.with_op(match op_version {
                Some(version) => meta.with_version(version),
                None => meta,
            })
        }
        None => record,
    })
}

/// Serialize a body for storage, failing loudly rather than storing something unreadable.
pub(crate) fn body_text(body: &serde_json::Value) -> Result<String, frontbox::Error> {
    serde_json::to_string(body).map_err(frontbox::Error::serialization)
}

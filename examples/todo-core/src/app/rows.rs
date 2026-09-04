//! How a write becomes durable: the row codec, the row write-through, and the enqueue.
//!
//! Split from `super` when D4d pushed it past the four-hundred-line cap `AGENTS.md` sets and
//! `scripts/verify.sh` measures. The boundary is the same one the file has been splitting along
//! since D4a: `mod.rs` decides *what a write means*, and everything downstream of that decision
//! lives beside it. Nothing here is a visibility boundary — every method below is still
//! `TodoApp::…` to a caller.

use frontbox::{
    Error, MutationId, MutationIntent, OperationMeta, OutboxStore, RowRef, RowStore, StoredRow,
};

use super::{Config, TodoApp, TODO};
use crate::invalidation::{ManualSource, Source, Sources, VersionPollSource};
use crate::store::Todo;
use crate::trace;
use crate::transport::{HttpTransport, SharedTransport};

impl TodoApp {
    /// Write one row's current local state through to durable storage.
    ///
    /// Called after every projection step, so what the user did survives a reload even though the
    /// server has not seen it. A row the projection no longer holds is deleted rather than left
    /// behind — and its staleness marker goes with it, which is the growth problem decision 023
    /// owed a policy for and decision 032 dissolved.
    pub(super) async fn persist_row(&self, id: &str) -> Result<(), Error> {
        match self.store.get(id) {
            Some(todo) => self.runner.store().put_rows(&[to_stored(&todo)]).await,
            None => {
                self.runner
                    .store()
                    .delete_rows(&[RowRef::new(TODO, id)])
                    .await?;
                Ok(())
            }
        }
    }

    pub(super) async fn enqueue(
        &self,
        row_id: &str,
        method: &str,
        path: String,
        body: serde_json::Value,
        op: &str,
    ) -> Result<(), Error> {
        self.enqueue_with_id(MutationId::new(), row_id, method, path, body, op)
            .await
    }

    pub(super) async fn enqueue_entity(
        &self,
        entity: &'static str,
        row_id: &str,
        method: &str,
        path: String,
        body: serde_json::Value,
        op: &str,
    ) -> Result<(), Error> {
        self.enqueue_bound(MutationId::new(), entity, row_id, method, path, body, op)
            .await
    }

    pub(super) async fn enqueue_with_id(
        &self,
        mutation_id: MutationId,
        row_id: &str,
        method: &str,
        path: String,
        body: serde_json::Value,
        op: &str,
    ) -> Result<(), Error> {
        self.enqueue_bound(mutation_id, TODO, row_id, method, path, body, op)
            .await
    }

    #[allow(clippy::too_many_arguments)] // Every argument names a distinct part of one envelope.
    pub(super) async fn enqueue_bound(
        &self,
        mutation_id: MutationId,
        entity: &'static str,
        row_id: &str,
        method: &str,
        path: String,
        body: serde_json::Value,
        op: &str,
    ) -> Result<(), Error> {
        let created_at = self.clock.now_ms();
        trace::log(format!(
            "enqueue op={op} entity={entity} row={row_id} method={method} path={path}"
        ));
        self.runner
            .store()
            .enqueue(
                MutationIntent::new(mutation_id, method, path, body, created_at)
                    .with_op(OperationMeta::new(op).with_version("1"))
                    // The application knows which row it is writing, here, and used to throw that
                    // away and recover it later by parsing the path. Binding it is what lets a
                    // hydration skip this row and a report name it
                    // (`wiki/decisions/032-opaque-row-store.decision.md`).
                    .with_row(RowRef::new(entity, row_id)),
            )
            .await?;
        // The pending index is the *todo* list's "saving…" marker, so only a todo marks one. A
        // queued user record is not a row the todo list renders, and marking it would leave a
        // permanent marker on a row id no todo has.
        if entity == TODO {
            self.store.mark_pending(row_id);
        }
        Ok(())
    }
}

/// A todo as frontbox stores it: a key it compares and a blob it never reads.
///
/// # Where a shape stamp would go, if this application wanted one
///
/// Inside the blob. `StoredRow` carried a `schema` number until 2026-09-01, and this function set
/// it to `1` and nothing ever read it back — which is the evidence that retired it. A field beside
/// a blob core cannot interpret duplicates what the blob can already say, and the blob's bytes are
/// wholly this crate's: a `#[serde(default)] shape: u32` on [`Todo`] would version it here, where
/// the code that understands both lives.
pub(super) fn to_stored(todo: &Todo) -> StoredRow {
    StoredRow::new(
        RowRef::new(TODO, &todo.id),
        serde_json::to_value(todo).unwrap_or(serde_json::Value::Null),
    )
}

/// Recover a todo from a stored row.
///
/// `None` for a blob this version cannot read, which is the honest answer for a row written by a
/// newer schema. It is skipped rather than raised, for the reason `pending_batch` skips an
/// undecodable envelope: one bad row must not make the whole projection unreadable.
pub(super) fn from_stored(stored: &StoredRow) -> Option<Todo> {
    serde_json::from_value(stored.blob.clone()).ok()
}

/// The routing transport this configuration describes.
///
/// Two routes and a fallback: user paths to the user service, everything else to the todo service.
/// Decision 037 puts this here rather than in core — the path already carries the destination, and
/// a durable origin column would be a second blessed way to say the same thing.
pub(super) fn transport_for(config: &Config) -> SharedTransport {
    let transport = HttpTransport::new(config.todo_url.clone());
    SharedTransport::new(match &config.user_url {
        Some(user_url) => transport.routing("/api/v1/users", user_url.clone()),
        // One destination, and the fallback route answers for every path — which is what every
        // trial before D4d had, expressed as an absent route rather than as a duplicate one.
        None => transport,
    })
}

const BROWSER_OBSERVABLE_STALENESS_MS: i64 = 15_000;

/// One polling source per service, at the tightest budget among that service's entities.
///
/// # The budgets are the application's, and they are not the same
///
/// This is a browser demo whose visible requirement is cross-window propagation. Both domains
/// therefore use the same fifteen-second budget as the visible UI tick: long enough not to flood
/// the two toy services, short enough that "eventually" is observable while a person is watching.
/// Stating them as budgets rather than intervals is what lets one endpoint answer for several
/// entities without either lying about the schedule or multiplying the requests — see
/// `examples/todo-core/src/invalidation.rs`.
///
/// # A source names its service, and the routing table does not decide for it
///
/// Both services answer `GET /api/v1/versions`, so the path cannot say which one is meant — the
/// routing transport keys on path prefix and would send both to the same host. That is not a flaw
/// in decision 037: the routing table exists to place *mutations*, whose paths are already
/// domain-specific, and a versions endpoint is per-service by construction. So a source carries its
/// base URL, which is the honest shape — a source is a thing that speaks for one origin.
pub(super) fn default_sources(transport: &SharedTransport, config: &Config) -> Sources {
    let mut sources = Sources::default().with(Source::Polling(poll_versions(
        "todo-service",
        vec![TODO],
        BROWSER_OBSERVABLE_STALENESS_MS,
        config.todo_url.clone(),
        transport.handle(),
    )));
    // A source speaks for one origin, so an absent origin is an absent source rather than a source
    // that fails on every tick and marks its entity stale forever.
    if let Some(user_url) = &config.user_url {
        sources = sources.with(Source::Polling(poll_versions(
            "user-service",
            vec![super::identity::USER],
            BROWSER_OBSERVABLE_STALENESS_MS,
            user_url.clone(),
            transport.handle(),
        )));
    }
    sources
        // A third source with no schedule, for whatever hands events in: a test today, a stream
        // later. It costs nothing when nothing feeds it — `budget_ms` is `i64::MAX`, so it is never
        // due — and it is what keeps the seam honest about push, which polling alone would let it
        // quietly stop supporting.
        .with(Source::Manual(ManualSource::new(
            "handed-in",
            vec![TODO, super::identity::USER],
        )))
}

/// Build a polling source over one service's versions endpoint.
fn poll_versions(
    name: &'static str,
    entities: Vec<&'static str>,
    budget_ms: i64,
    base_url: String,
    transport: std::rc::Rc<HttpTransport>,
) -> VersionPollSource {
    VersionPollSource::new(name, entities, budget_ms, move || {
        let transport = std::rc::Rc::clone(&transport);
        let base_url = base_url.clone();
        Box::pin(async move {
            // Through the drain's own transport, so the trial's offline switch covers polling as
            // well as writing. A client where "offline" stopped writes but not polls is not one
            // anybody ships.
            trace::log(format!(
                "invalidation fetch source={name} url={base_url}/api/v1/versions"
            ));
            let map: std::collections::BTreeMap<String, String> =
                transport.get_json_at(&base_url, "/api/v1/versions").await?;
            trace::log(format!(
                "invalidation fetched source={name} versions={}",
                map.len()
            ));
            Ok(map.into_iter().collect())
        })
    })
}

//! The read half: starting up, draining, and reconciling the server against what is queued.
//!
//! Split from `super` for length, along the line the trial's own findings drew. Everything here
//! *reads* — from durable storage, from the server, or from the outbox — and writes only what it
//! read back into the projection. The four user-facing writes stay next door, because they are the
//! half that has to decide what a mutation means before anything durable happens.
//!
//! Nothing here is new; the module boundary is not a visibility boundary, so every method below is
//! still `TodoApp::…` to a caller.

use frontbox::{DrainReport, Error, OutboxStore, RowStore, StoredRow};

use super::{from_stored, to_stored, PendingIndexGap, Startup, TodoApp, ROW_LOAD_LIMIT, TODO};
use crate::store::Todo;
use crate::trace;

impl TodoApp {
    /// Bring a freshly built application up: read the server's list, then rebuild the pending
    /// index.
    ///
    /// # Why this is a method and not three lines in the caller
    ///
    /// Because it was three lines in the caller, and the caller did not write them. Every test in
    /// this crate performed the sequence itself, so [`refresh_from_server`](TodoApp::refresh_from_server)
    /// was thoroughly proven to *work* while nothing proved the application *called* it — and
    /// `examples/todo-app`, which has no tests, did not. The browser UI was write-only against the
    /// server: rows drained and appeared in the API, and a reload showed an empty list.
    ///
    /// The sequence now lives where a test can reach it. See
    /// `wiki/plans/d4a-offline-todo-trial.plan.md`, `## Finding 6`.
    ///
    /// # Starting offline is not a failure
    ///
    /// [`Error::is_offline`] gets a `hydrated: false` and an `Ok`, leaving the projection as it
    /// was. This is the same posture the runner takes — `SyncPass::Offline` is a clean `Ok` and
    /// every other transport error is an `Err` — and for the same reason: an offline start is the
    /// condition this application exists to tolerate, so refusing to start would be refusing to do
    /// the one thing it promises. Every other transport failure propagates, because a wrong base
    /// URL should be loud.
    ///
    /// # Errors
    ///
    /// A storage failure, or a transport failure that is not being offline.
    pub async fn start(&self) -> Result<Startup, Error> {
        trace::log("startup begin");
        // **Durable rows first, before the network is even attempted.** This is what an offline
        // start now means: the projection comes back from storage, not from an empty map waiting
        // on a server. Before decision 032 there was nowhere to load it from, so a reload with no
        // network showed nothing at all.
        let restored = self.load_projection().await?;
        // D4d's second entity, restored from durable rows for the same reason the todos are: a
        // reload with no network must still be able to say whose todo a todo is.
        self.load_users().await?;

        let (hydrated, protected) = match self.refresh_from_server().await {
            Ok(skipped) => (true, skipped),
            Err(failure) if failure.is_offline() => (false, Vec::new()),
            Err(failure) => return Err(failure),
        };
        // Attempted only once the todo service has answered, so an offline start does not make two
        // doomed requests. Tolerated the same way: a user service that is down leaves the cached
        // records in place rather than failing the startup.
        if hydrated {
            match self.refresh_users_from_server().await {
                Ok(()) => {}
                Err(failure) if failure.is_offline() => {}
                Err(failure) => return Err(failure),
            }
        }
        // Unconditional, including after a hydrate that did not happen: the index describes the
        // queue, not the server, and an offline start is exactly when the queue has something in
        // it worth indexing.
        let gap = self.refresh_pending().await?;
        trace::log(format!(
            "startup complete hydrated={} restored={} protected={} pending_gap={:?}",
            hydrated,
            restored,
            protected.len(),
            gap
        ));
        Ok(Startup {
            hydrated,
            gap,
            restored,
            protected,
        })
    }

    /// Drain the queue, then bring the pending index back in step.
    ///
    /// The drain's report is the return value. What the index rebuild could not account for is not
    /// folded into it — `DrainReport` is core's type and this is an application's problem — so
    /// it is left on [`pending_index_gap`](TodoApp::pending_index_gap) for a caller to render.
    ///
    /// # Errors
    ///
    /// A storage or transport failure. An incomplete index rebuild is neither.
    pub async fn sync(&self) -> Result<DrainReport, Error> {
        trace::log("sync drain start");
        let report = self.runner.drain().await?;
        trace::log(format!(
            "sync drain end ended={:?} passes={} sent={} drained={}",
            report.ended,
            report.passes,
            report.sent,
            report.drained.len()
        ));
        // **Finding 1, closed.** This used to be `refresh_pending()` — a full scan of the queue
        // after every drain, because a report named a mutation only when something went wrong and
        // there was no success event to decrement on. `DrainReport::drained` is that event
        // (`wiki/decisions/035-reports-name-what-drained.decision.md`), so the index moves by
        // exactly what left the queue instead of being rebuilt from scratch.
        //
        // A dead letter decrements too: the write did not happen, but the record is no longer
        // queued, and "saving…" is about the queue. What the server refused is the dead-letter
        // panel's business.
        let mut accounted = 0_usize;
        for drained in &report.drained {
            if let Some(row) = drained.row.as_ref().filter(|row| row.entity == TODO) {
                self.store.unmark_pending(&row.row_id);
                accounted += 1;
            }
        }

        // **And the decrement is not enough on its own, because this index is not the only reader
        // of that queue.** Decision 035's incremental move is correct for work *this* client
        // drained; two records leave the queue without ever appearing in `drained`:
        //
        // - **Another realm drained it.** On web the outbox is one origin-scoped IndexedDB
        //   database, so two browser windows are two `TodoStore`s over one queue while the
        //   "saving…" index is per window and in memory. Whichever window wins the Web Lock
        //   decrements *its* index, and the other never hears about it — the row it enqueued stays
        //   on "saving…" for the rest of the session. Measured: with the second window online and
        //   the first switched offline, the record reaches the server and the two windows disagree
        //   about it, `saving` against not.
        // - **The record was quarantined.** `sweep_corrupt` removes it before the batch is built
        //   and `Disposition::Quarantine` is deliberately excluded from `drained`, so nothing here
        //   would ever clear the marker.
        //
        // So a pass that accounted for nothing while the index still claims outstanding work
        // reconciles against the queue instead of guessing. It costs a bounded scan
        // (`with_pending_scan_limit`) and only in that case: the ordinary single-window path
        // decrements and never reaches this line, which is what keeps decision 035 the steady
        // state rather than a rebuild wearing its name.
        if accounted == 0 && self.store.saving_rows() > 0 {
            trace::log(format!(
                "sync reconciling pending index saving_rows={}",
                self.store.saving_rows()
            ));
            self.refresh_pending().await?;
        }
        Ok(report)
    }

    /// Rebuild the row-to-pending index from what is still queued.
    ///
    /// # This is reconciliation now, not the steady state
    ///
    /// It used to run after every drain, because nothing in a [`DrainReport`] named the mutations
    /// that drained and a full scan was the only correct move — Finding 1. Decision 035 added
    /// `drained`, so [`sync`](TodoApp::sync) decrements instead and this runs at startup, where a
    /// durable queue has to be turned back into an index with no history of enqueues to replay —
    /// and after any drain pass that accounted for nothing while the index still claims outstanding
    /// work, which is how a record another browser window drained out of the shared queue stops
    /// leaving a permanent "saving…" behind. See [`sync`](TodoApp::sync) for that case in full.
    ///
    /// **The incremental index is the more accurate of the two**, which is worth saying because the
    /// rebuild sounds authoritative — and it is only more accurate *within one realm*, which is
    /// exactly the assumption the second window breaks. `mark_pending` and `unmark_pending` see
    /// every enqueue and every drain this client made exactly once; this scan sees only as far as
    /// [`with_pending_scan_limit`](TodoApp::with_pending_scan_limit) reaches, and reports the
    /// shortfall rather than closing it.
    ///
    /// Returns what the rebuild could not account for. An incomplete scan is reported, never
    /// raised: the index is still published, because the alternative is leaving an older and more
    /// wrong one on screen, and an `Err` here would discard a [`DrainReport`] for work the server
    /// has already committed.
    ///
    /// # Errors
    ///
    /// A storage failure. Neither a capped scan nor an unreadable envelope is one.
    pub async fn refresh_pending(&self) -> Result<PendingIndexGap, Error> {
        trace::log("pending refresh start");
        let pending = self.runner.store().pending_count().await?;
        let queued = self
            .runner
            .store()
            .pending_batch(self.pending_scan_limit)
            .await?;

        let mut rows = Vec::with_capacity(queued.len());
        let mut unrecoverable = 0;
        for record in &queued {
            // Read off the record rather than parsed out of its path. `row_id_of` used to guess
            // this from URL segments and is gone; an envelope that names no row is now a caller
            // that did not bind one, which is a different and much narrower thing than a route
            // this crate failed to recognise.
            //
            // **A record belonging to another entity is not unplaceable.** It used to be counted
            // as one, and the banner said so: sign up while offline, reload, and a perfectly
            // healthy queue holding one `user` mutation rendered "index incomplete: 1 unplaceable".
            // This index is the todo list's "saving…" marker and a queued user record is simply
            // not its business — the only thing it genuinely cannot place is a record that names
            // no row at all.
            match record.row.as_ref() {
                Some(row) if row.entity == TODO => rows.push(row.row_id.clone()),
                Some(_) => {}
                None => unrecoverable += 1,
            }
        }
        self.store.set_pending_rows(rows);

        let gap = PendingIndexGap {
            beyond_scan_cap: pending.saturating_sub(queued.len()),
            unrecoverable,
        };
        self.index_gap.set(gap);
        trace::log(format!(
            "pending refresh pending={} queued={} gap={:?}",
            pending,
            queued.len(),
            gap
        ));
        Ok(gap)
    }

    /// Replace the projection with the server's own list.
    ///
    /// # Errors
    ///
    /// A transport failure.
    pub async fn refresh_from_server(&self) -> Result<Vec<String>, Error> {
        trace::log("hydrate todos start");
        let rows: Vec<Todo> = self.runner.transport().get_json("/api/v1/todos").await?;
        trace::log(format!("hydrate todos fetched={}", rows.len()));

        // Merged, never replaced. The server has not seen whatever is still queued, so writing its
        // list over the local rows would drop exactly the edits the user is waiting on — they come
        // back a drain later, and for a client that is still offline "later" has no end. frontbox
        // decides which rows to protect because it is the only thing that can see both the rows
        // and the queue (`wiki/decisions/032-opaque-row-store.decision.md`).
        //
        // The server is authoritative about which rows *exist*, though, so rows it did not mention
        // and nothing is queued for are deleted rather than left behind.
        let stored: Vec<StoredRow> = rows.iter().map(to_stored).collect();
        let skipped = self.runner.store().merge_rows(&stored).await?;
        // Which rows the queue protects, and whether the delete may run at all, is
        // `prune_rows_absent`'s call — it is the same decision the user hydration makes, and it is
        // the one that costs data when it is wrong.
        let keep: std::collections::HashSet<String> = rows
            .iter()
            .map(|todo| todo.id.clone())
            .chain(skipped.iter().map(|row| row.row_id.clone()))
            .collect();
        let prune = self.prune_rows_absent(TODO, keep).await?;

        self.load_projection().await?;
        trace::log(format!(
            "hydrate todos merged={} skipped={} queued={} deleted={} delete_suppressed={}",
            stored.len(),
            skipped.len(),
            prune.queued,
            prune.deleted,
            prune.suppressed
        ));
        Ok(skipped.into_iter().map(|row| row.row_id).collect())
    }

    /// Refill the in-memory projection from durable rows.
    ///
    /// The read model stays a `BTreeMap` in memory because that is what a render needs; what
    /// changed is where it comes from. Before decision 032 there was nowhere to load it *from*, so
    /// a reload started empty and the server was the only source — which is why an offline reload
    /// showed nothing.
    async fn load_projection(&self) -> Result<usize, Error> {
        let stored = self.runner.store().list_rows(TODO, ROW_LOAD_LIMIT).await?;
        let rows: Vec<Todo> = stored.iter().filter_map(from_stored).collect();
        let loaded = rows.len();
        // The shortfall is reported rather than closed, which is the same posture `refresh_pending`
        // takes towards a capped scan. `from_stored` skips a blob this version cannot read — one
        // bad row must not make the whole projection unreadable — but a projection quietly shorter
        // than its own storage is exactly the kind of thing that gets diagnosed as "the server lost
        // my todo", so the count is said out loud.
        trace::log(format!(
            "load todos rows={loaded} unreadable={}",
            stored.len() - loaded
        ));
        self.store.replace(rows);
        Ok(loaded)
    }
}

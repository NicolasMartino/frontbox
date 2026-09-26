//! One sync pass, and the loop that runs them back to back.
//!
//! A pass sweeps corrupt rows, reads a bounded batch, sends it, and applies the server's verdicts
//! atomically. [`SyncRunner::drain`] repeats that until the queue stops draining.
//!
//! What is still an adapter concern is the *cadence*: how long to wait before draining again.
//! That needs a timer, and this crate has none (`wiki/decisions/028-drain-loop-boundary.decision.md`).

use std::collections::{HashMap, HashSet};

use crate::error::Error;
use crate::id::MutationId;
use crate::protocol::{MutationBatchRequest, MutationStatus};
use crate::record::{DeadLetterReason, RowRef};
use crate::store::{Disposition, OutboxStore, Outcome};
use crate::transport::SyncTransport;

/// How many records one pass sends by default.
pub const DEFAULT_BATCH_LIMIT: usize = 100;

mod config;
mod drain;
pub(crate) mod exclusion;
mod report;

pub use drain::{DrainEnd, DrainReport};
pub use report::{
    Anomaly, AnomalyKind, Drained, DrainedAs, SyncOutcomeCounts, SyncPass, SyncReport,
};

/// Runs sync passes against a store and a transport.
///
/// Both are generic parameters rather than trait objects: `async fn` in trait is not
/// dyn-compatible, and this crate does not add the adapter layer that would make it so.
pub struct SyncRunner<S, T> {
    store: S,
    transport: T,
    batch_limit: usize,
    retention_bound: Option<u32>,
}

impl<S, T> SyncRunner<S, T>
where
    S: OutboxStore,
    T: SyncTransport,
{
    /// Run one pass.
    ///
    /// Starting a pass while one is in flight **on this scope** returns
    /// [`SyncPass::AlreadyRunning`] without sending anything, so a periodic loop overlapping a
    /// manual trigger cannot double-send a batch.
    ///
    /// # The claim is on the scope, not on this runner
    ///
    /// A guard on the runner would exclude only a second pass by the *same* runner, and two
    /// handles opened on one scope are two views of one queue: two runners over them would each
    /// find their own guard free and both send the same batch. What that costs is specific —
    /// idempotency survives, because the server dedupes on `mutation_id`, while ordering and the
    /// retention bound do not (`wiki/decisions/031-cross-realm-single-flight.decision.md`). So the
    /// claim is keyed by [`ScopeKey`](crate::scope::ScopeKey), and a second runner opened under the
    /// same key reports `AlreadyRunning` rather than racing the first.
    ///
    /// **Within this realm only.** Two browser tabs are two realms over one durable store, and
    /// neither can see the other's claim; extending the exclusion across realms is an obligation on
    /// the backend, stated on [`OutboxStore`].
    ///
    /// # Cancellation
    ///
    /// Dropping the returned future is safe: the claim is released by a guard, so a cancelled pass
    /// does not wedge the scope. This matters because cancellation is routine on the target
    /// runtimes — a Dioxus `use_future` is dropped whenever its component re-renders — and a claim
    /// released only on the success path would leave every later call returning
    /// [`SyncPass::AlreadyRunning`] forever.
    ///
    /// What a cancelled pass leaves behind depends on where it was dropped. Before
    /// `apply_outcomes`, nothing changed and the work is still queued. After it, the outcomes are
    /// committed but the [`SyncReport`] is lost, so the caller learns what happened from the store
    /// rather than from a return value. Neither case loses or double-applies work, because
    /// `apply_outcomes` is atomic and `mutation_id` is the idempotency key on any resend.
    ///
    /// # Errors
    ///
    /// A storage failure, or a transport failure where a request was actually attempted. Being
    /// offline is not an error: it returns [`SyncPass::Offline`] with every record still queued.
    /// See that variant for what a pass which never read differs in from one that read and then
    /// found the network gone.
    pub async fn sync_once(&self) -> Result<SyncReport, Error> {
        let Some(_lease) = crate::DrainLease::claim(&self.store).await? else {
            return Ok(SyncReport::ended(SyncPass::AlreadyRunning));
        };
        self.run().await
    }

    async fn run(&self) -> Result<SyncReport, Error> {
        // Asked before anything is read, because `read_for_send` durably marks what it returns and
        // that mark is never cleared. A client that already knows it is offline would otherwise
        // spend the head batch's coalescing eligibility on every poll, for a pass that was never
        // going to reach the network — and coalescing exists to collapse edits made *while
        // offline*, so that is the whole feature (`src/transport.rs`).
        //
        // Ahead of `sweep_corrupt` as well as the read, so an offline pass writes nothing at all.
        // A corrupt row stays invisible for one more pass, which is the window `sweep_corrupt`'s
        // contract already describes; it is not made worse by declining to run while offline, and
        // the next connected pass closes it.
        if self.transport.offline_now().await? {
            return Ok(SyncReport::ended(SyncPass::Offline));
        }

        // Corrupt rows are found before the batch is built, so an undecodable record becomes
        // visible in quarantine rather than being silently skipped on every pass forever.
        let quarantined = self.store.sweep_corrupt().await?;

        // Not `pending_batch`: this marks every record it returns as transport-started, in its own
        // transaction, before the request below exists. Deriving that fact afterwards from how the
        // send failed cannot be done — `Error::Offline` covers a browser `fetch` that fails for
        // lack of connectivity, which is indistinguishable from one the server answered into a lost
        // response (`wiki/proposals/queued-write-coalescing.proposal.md`).
        let records = self.store.read_for_send(self.batch_limit).await?;
        if records.is_empty() {
            let mut report = SyncReport::ended(SyncPass::Idle);
            report.quarantined = quarantined;
            return Ok(report);
        }

        let sent = records.len();
        let request =
            MutationBatchRequest::new(records.iter().map(|record| record.to_intent()).collect());

        let response = match self.transport.send_batch(request).await {
            Ok(response) => response,
            Err(error) if error.is_offline() => {
                let mut report = SyncReport::ended(SyncPass::Offline);
                report.sent = sent;
                report.retained = sent;
                report.quarantined = quarantined;
                return Ok(report);
            }
            // A request was attempted and failed. Pending work is untouched, and the caller sees a
            // failed attempt rather than a quiet no-op.
            Err(error) => return Err(error),
        };

        let known: HashSet<MutationId> = records.iter().map(|record| record.mutation_id).collect();

        // Repeated ids are found before any verdict is applied, rather than while walking the list.
        // Rejecting only the *second* of a contradictory pair would silently make arrival order the
        // tie-break — which is precisely the preference this crate says it has no basis for. So a
        // repeated id gets no outcome at all and its record stays queued, to be resent and ruled on
        // again; `mutation_id` is the idempotency key, so a resend costs a round trip and nothing
        // else.
        //
        // Verdicts that merely agree with each other are not exempted. Deciding that two are "the
        // same" would mean core comparing rejection payloads and ranking statuses, which is the
        // judgment it is declining to make.
        let mut seen: HashSet<MutationId> = HashSet::with_capacity(response.results.len());
        let repeated: HashSet<MutationId> = response
            .results
            .iter()
            .filter(|result| !seen.insert(result.mutation_id))
            .map(|result| result.mutation_id)
            .collect();

        let mut counts = SyncOutcomeCounts::default();
        let mut outcomes = Vec::with_capacity(response.results.len());
        let mut anomalies = Vec::new();
        let mut applied_ids: HashSet<MutationId> = HashSet::new();

        for result in response.results {
            if !known.contains(&result.mutation_id) {
                anomalies.push(Anomaly::new(
                    result.mutation_id,
                    AnomalyKind::UnknownMutation,
                ));
                continue;
            }
            if repeated.contains(&result.mutation_id) {
                anomalies.push(Anomaly::new(
                    result.mutation_id,
                    AnomalyKind::RepeatedVerdict,
                ));
                continue;
            }

            let disposition = match result.status {
                MutationStatus::Applied => {
                    counts.applied += 1;
                    // Both `Applied` and `Duplicate` map to `Delete`, so the disposition alone
                    // cannot tell the report which one happened. Recorded here, where the status
                    // is still in hand.
                    applied_ids.insert(result.mutation_id);
                    Disposition::Delete
                }
                MutationStatus::Duplicate => {
                    counts.duplicate += 1;
                    Disposition::Delete
                }
                MutationStatus::Rejected => {
                    counts.dead_lettered += 1;
                    Disposition::DeadLetter {
                        reason: DeadLetterReason::Rejected {
                            error: result.error,
                        },
                    }
                }
                // Never evaluated on its own merits, so it keeps its place in the queue. The
                // source dead-letters this alongside `Rejected`, which discards valid work for the
                // sole reason that it followed a failed record in the same batch.
                MutationStatus::Blocked => {
                    counts.blocked += 1;
                    Disposition::Retain {
                        reason: Some("blocked".to_owned()),
                    }
                }
                MutationStatus::Pending => {
                    counts.pending += 1;
                    Disposition::Retain {
                        reason: Some("pending".to_owned()),
                    }
                }
                // Sent, ruled on, and unactionable. It gets an outcome *and* an anomaly: the
                // outcome so the backend accounts for a record it handed out, the anomaly so the
                // caller learns which word the server used. Mapping it onto a known status would
                // mean assuming acceptance or refusal, and this crate does not guess.
                MutationStatus::Unknown(raw) => {
                    counts.unknown_status += 1;
                    anomalies.push(Anomaly::new(
                        result.mutation_id,
                        AnomalyKind::UnknownStatus(raw.clone()),
                    ));
                    // The one retain whose reason is the server's own word rather than this
                    // crate's. It is also the only status core cannot name, which is exactly why
                    // decision 033 stores the string instead of an enum.
                    Disposition::Retain { reason: Some(raw) }
                }
            };
            outcomes.push(Outcome::new(result.mutation_id, disposition));
        }

        // Every record that was sent and is still queued gets a `Retain`, including the ones the
        // server said nothing about and the ones whose verdicts contradicted each other. Silence is
        // still not a verdict — the disposition is `Retain`, which changes no state and invents no
        // refusal — but it *is* an attempt, because the record was sent and came back queued.
        //
        // Leaving them out would have been the smaller change and it defeats the point: a server
        // that permanently omits one verdict would never advance that record's count, so at
        // `batch_limit = 1` it would freeze the whole queue forever, which is the failure
        // `wiki/decisions/017-bounded-retention.decision.md` exists to prevent.
        //
        // Every id here is distinct and was sent in this batch, which is what `apply_outcomes`
        // requires: `known` filters out ids the server invented, and `repeated` filters out every
        // verdict for an id the server answered more than once.
        // The set is what guarantees distinctness, which `apply_outcomes` requires and which the
        // verdict loop got for free from `repeated`. A store holding two rows under one id is a
        // degenerate state this runner does not create, but emitting the id twice would turn it
        // into a protocol error blamed on the server.
        let mut emitted: HashSet<MutationId> = outcomes.iter().map(|outcome| outcome.id).collect();
        for record in &records {
            if emitted.insert(record.mutation_id) {
                // Silence is not a verdict, so there is no server word to record — but it is an
                // attempt, and `last_error` should say which kind of nothing came back.
                outcomes.push(Outcome::new(
                    record.mutation_id,
                    Disposition::Retain {
                        reason: Some("no verdict returned".to_owned()),
                    },
                ));
            }
        }

        // A record whose recorded attempts have reached the bound is terminated rather than sent
        // again. `attempts` is what previous passes recorded, so the count on the dead letter is
        // exactly the number of attempts that were made — no off-by-one to explain to whoever
        // reads it later. The dead letter carries no `RemoteRejection`, because no server refused
        // it.
        if let Some(bound) = self.retention_bound {
            let at_bound: HashSet<MutationId> = records
                .iter()
                .filter(|record| record.attempts >= bound)
                .map(|record| record.mutation_id)
                .collect();
            for outcome in &mut outcomes {
                if matches!(outcome.disposition, Disposition::Retain { .. })
                    && at_bound.contains(&outcome.id)
                {
                    outcome.disposition = Disposition::DeadLetter {
                        reason: DeadLetterReason::RetentionBound,
                    };
                    counts.dead_lettered += 1;
                }
            }
        }

        // Built before `apply_outcomes` consumes nothing but read after it succeeds: if the
        // apply fails, no record left the queue and reporting that one did would be a lie the
        // caller could act on. `records` is what carries the row binding — an `Outcome` names only
        // an id, by design (`wiki/decisions/035-reports-name-what-drained.decision.md`).
        let bound_rows: HashMap<MutationId, RowRef> = records
            .iter()
            .filter_map(|record| record.row.clone().map(|row| (record.mutation_id, row)))
            .collect();
        let drained: Vec<Drained> = outcomes
            .iter()
            .filter_map(|outcome| {
                let outcome_kind = match &outcome.disposition {
                    Disposition::Delete => {
                        // `Delete` covers both `Applied` and `Duplicate`, and the report must tell
                        // them apart. The status is not on the outcome, so it is recovered from
                        // the verdict list rather than inferred from the disposition.
                        match applied_ids.contains(&outcome.id) {
                            true => DrainedAs::Applied,
                            false => DrainedAs::Duplicate,
                        }
                    }
                    Disposition::DeadLetter { .. } => DrainedAs::DeadLettered,
                    // Retained and quarantined records have not left the queue.
                    _ => return None,
                };
                Some(Drained::new(
                    outcome.id,
                    outcome_kind,
                    bound_rows.get(&outcome.id).cloned(),
                ))
            })
            .collect();

        self.store.apply_outcomes(&outcomes).await?;

        Ok(SyncReport {
            pass: SyncPass::Completed,
            sent,
            counts,
            retained: sent - (counts.applied + counts.duplicate + counts.dead_lettered),
            quarantined,
            anomalies,
            drained,
        })
    }
}

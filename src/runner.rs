//! One sync pass.
//!
//! The periodic loop that calls this is an adapter concern. What lives here is a single pass:
//! sweep corrupt rows, read a bounded batch, send it, and apply the server's verdicts atomically.

use std::cell::Cell;
use std::collections::HashSet;

use crate::error::Error;
use crate::id::MutationId;
use crate::protocol::{MutationBatchRequest, MutationStatus};
use crate::store::{Disposition, OutboxStore, Outcome};
use crate::transport::SyncTransport;

/// How many records one pass sends by default.
pub const DEFAULT_BATCH_LIMIT: usize = 100;

/// How a sync pass ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SyncPass {
    /// A batch was sent and the server's verdicts were applied.
    Completed,
    /// Nothing was pending.
    Idle,
    /// No request could be attempted. Work is untouched, and this is not a failed attempt.
    Offline,
    /// A pass was already in flight, so this call did nothing.
    AlreadyRunning,
}

/// How many records each verdict accounted for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct SyncOutcomeCounts {
    /// Server applied them; removed from the queue.
    pub applied: usize,
    /// Server had already seen them; removed from the queue.
    pub duplicate: usize,
    /// Server terminally refused them; moved to dead letters.
    pub dead_lettered: usize,
    /// Server skipped them behind an earlier terminal failure; left queued.
    pub blocked: usize,
    /// Server has not finished with them; left queued.
    pub pending: usize,
    /// Server answered with a status this crate cannot act on; left queued.
    ///
    /// These also appear in [`anomalies`](SyncReport::anomalies) with the offending string, which
    /// is where a caller finds out *which* status it was. The count is here so that
    /// `blocked + pending + unknown` still reconciles against
    /// [`retained`](SyncReport::retained) without walking the anomaly list.
    pub unknown_status: usize,
}

/// A verdict that changed nothing, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Anomaly {
    /// The mutation the verdict named.
    pub mutation_id: MutationId,
    /// Why it could not be acted on.
    pub kind: AnomalyKind,
}

impl Anomaly {
    /// Build an anomaly.
    pub fn new(mutation_id: MutationId, kind: AnomalyKind) -> Self {
        Self { mutation_id, kind }
    }
}

/// Why a verdict changed nothing.
///
/// These are three different problems and they call for different responses, which is why the
/// report names them rather than handing back a bare list of identifiers. A server ruling on work
/// the client never sent is a routing or identity fault; a repeated id is a server that contradicted
/// itself; an unrecognised status means the two ends are speaking different versions of the
/// protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AnomalyKind {
    /// A verdict naming a record this pass did not send.
    UnknownMutation,
    /// One of two or more verdicts for the same id in one response. None of them was applied.
    RepeatedVerdict,
    /// A status this crate cannot act on, as the server spelled it.
    ///
    /// The record is retained. See
    /// [`MutationStatus::Unknown`](crate::protocol::MutationStatus::Unknown).
    UnknownStatus(String),
}

/// What one pass did.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SyncReport {
    /// How the pass ended.
    pub pass: SyncPass,
    /// How many records were sent.
    pub sent: usize,
    /// How many records each verdict accounted for.
    pub counts: SyncOutcomeCounts,
    /// How many sent records are still queued afterwards.
    ///
    /// Counts records the server explicitly left queued (`Blocked`, `Pending`) *and* records it
    /// returned no verdict for at all.
    pub retained: usize,
    /// How many undecodable rows the pass moved to quarantine before sending.
    pub quarantined: usize,
    /// Verdicts that changed nothing, each with the reason it could not be acted on.
    ///
    /// Reported rather than acted on, for the same reason in all three cases: the crate will not
    /// guess. A server ruling on a mutation the client did not send is a protocol violation, and
    /// acting on it would risk mutating an unrelated record. A repeated id means the verdicts may
    /// disagree with no basis for preferring either, so **none of them is applied**. An unrecognised
    /// status cannot be mapped onto a known one without assuming either acceptance or refusal, and
    /// both lose data when wrong.
    ///
    /// Every verdict that changed nothing appears here, in the order the server sent it, so a
    /// response carrying one id twice contributes two entries. All of them leave the record queued
    /// and counted in [`retained`](SyncReport::retained).
    ///
    /// An [`UnknownStatus`](AnomalyKind::UnknownStatus) entry is the one kind that *also* produces
    /// an outcome — a [`Retain`](crate::store::Disposition::Retain), because unlike the other two
    /// the record was genuinely sent and genuinely ruled on. The anomaly is the diagnosis, not the
    /// disposition.
    pub anomalies: Vec<Anomaly>,
}

impl SyncReport {
    fn ended(pass: SyncPass) -> Self {
        Self {
            pass,
            sent: 0,
            counts: SyncOutcomeCounts::default(),
            retained: 0,
            quarantined: 0,
            anomalies: Vec::new(),
        }
    }

    /// Whether the queue actually got shorter.
    ///
    /// True when at least one record was applied, deduplicated, or dead-lettered.
    pub fn made_progress(&self) -> bool {
        self.counts.applied + self.counts.duplicate + self.counts.dead_lettered > 0
    }

    /// Whether a pass sent work and drained none of it.
    ///
    /// This is the signal that distinguishes a stalled queue from an idle one, and it exists
    /// because retention has no bound. `Blocked` clears on its own — the record that caused it was
    /// terminally rejected, so it is dead-lettered and gone before the next batch is built. But a
    /// `Pending` prefix as long as the batch limit starves everything behind it for as long as the
    /// server takes to settle those jobs.
    ///
    /// That is head-of-line blocking, not deadlock. It resolves when the server does. This crate
    /// makes it observable rather than silent; it does not age, count, or escalate it.
    pub fn is_stalled(&self) -> bool {
        self.sent > 0 && !self.made_progress()
    }
}

/// Releases the in-flight flag however the pass ends, including cancellation.
struct InFlightGuard<'a>(&'a Cell<bool>);

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// Runs sync passes against a store and a transport.
///
/// Both are generic parameters rather than trait objects: `async fn` in trait is not
/// dyn-compatible, and this crate does not add the adapter layer that would make it so.
pub struct SyncRunner<S, T> {
    store: S,
    transport: T,
    batch_limit: usize,
    in_flight: Cell<bool>,
}

impl<S, T> SyncRunner<S, T>
where
    S: OutboxStore,
    T: SyncTransport,
{
    /// Build a runner with the default batch limit.
    pub fn new(store: S, transport: T) -> Self {
        Self {
            store,
            transport,
            batch_limit: DEFAULT_BATCH_LIMIT,
            in_flight: Cell::new(false),
        }
    }

    /// Set how many records one pass sends.
    ///
    /// A limit of zero would send nothing forever, so it is clamped to one.
    #[must_use]
    pub fn with_batch_limit(mut self, limit: usize) -> Self {
        self.batch_limit = limit.max(1);
        self
    }

    /// The store this runner drives.
    pub fn store(&self) -> &S {
        &self.store
    }

    /// The transport this runner sends through.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Run one pass.
    ///
    /// Re-entering while a pass is in flight returns [`SyncPass::AlreadyRunning`] without sending
    /// anything, so a periodic loop overlapping a manual trigger cannot double-send a batch.
    ///
    /// # Cancellation
    ///
    /// Dropping the returned future is safe: the in-flight flag is released by a guard, so a
    /// cancelled pass does not wedge the runner. This matters because cancellation is routine on
    /// the target runtimes — a Dioxus `use_future` is dropped whenever its component re-renders —
    /// and a flag released only on the success path would leave every later call returning
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
    /// offline is not an error: it returns [`SyncPass::Offline`] with the queue untouched.
    pub async fn sync_once(&self) -> Result<SyncReport, Error> {
        if self.in_flight.get() {
            return Ok(SyncReport::ended(SyncPass::AlreadyRunning));
        }
        self.in_flight.set(true);
        let _guard = InFlightGuard(&self.in_flight);
        self.run().await
    }

    async fn run(&self) -> Result<SyncReport, Error> {
        // Corrupt rows are found before the batch is built, so an undecodable record becomes
        // visible in quarantine rather than being silently skipped on every pass forever.
        let quarantined = self.store.sweep_corrupt().await?;

        let records = self.store.pending_batch(self.batch_limit).await?;
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
                    Disposition::Delete
                }
                MutationStatus::Duplicate => {
                    counts.duplicate += 1;
                    Disposition::Delete
                }
                MutationStatus::Rejected => {
                    counts.dead_lettered += 1;
                    Disposition::DeadLetter {
                        error: result.error,
                    }
                }
                // Never evaluated on its own merits, so it keeps its place in the queue. The
                // source dead-letters this alongside `Rejected`, which discards valid work for the
                // sole reason that it followed a failed record in the same batch.
                MutationStatus::Blocked => {
                    counts.blocked += 1;
                    Disposition::Retain
                }
                MutationStatus::Pending => {
                    counts.pending += 1;
                    Disposition::Retain
                }
                // Sent, ruled on, and unactionable. It gets an outcome *and* an anomaly: the
                // outcome so the backend accounts for a record it handed out, the anomaly so the
                // caller learns which word the server used. Mapping it onto a known status would
                // mean assuming acceptance or refusal, and this crate does not guess.
                MutationStatus::Unknown(raw) => {
                    counts.unknown_status += 1;
                    anomalies.push(Anomaly::new(
                        result.mutation_id,
                        AnomalyKind::UnknownStatus(raw),
                    ));
                    Disposition::Retain
                }
            };
            outcomes.push(Outcome::new(result.mutation_id, disposition));
        }

        // Records the server did not rule on get no outcome at all, and so stay queued. That is
        // deliberate: a server is not obliged to answer every mutation it was sent, and treating
        // silence as any verdict would either drop work or invent a refusal. They are counted in
        // `retained` alongside the explicitly `Blocked` and `Pending` ones.
        //
        // Retain outcomes for the records that *were* ruled on are included deliberately: a backend
        // can then check that every record it handed out was accounted for, rather than inferring
        // silence to mean "leave it alone".
        //
        // Every id here is distinct and was sent in this batch, which is what `apply_outcomes`
        // requires: `known` filters out ids the server invented, and `repeated` filters out every
        // verdict for an id the server answered more than once.
        self.store.apply_outcomes(&outcomes).await?;

        Ok(SyncReport {
            pass: SyncPass::Completed,
            sent,
            counts,
            retained: sent - (counts.applied + counts.duplicate + counts.dead_lettered),
            quarantined,
            anomalies,
        })
    }
}

//! One sync pass.
//!
//! The periodic loop that calls this is an adapter concern. What lives here is a single pass:
//! sweep corrupt rows, read a bounded batch, send it, and apply the server's verdicts atomically.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};

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
    pub retained: usize,
    /// How many undecodable rows the pass moved to quarantine before sending.
    pub quarantined: usize,
    /// Verdicts that named a record this pass did not send, or named one twice.
    ///
    /// Reported rather than acted on. A server ruling on a mutation the client did not send is a
    /// protocol violation, and guessing what it meant would risk mutating an unrelated record.
    pub anomalies: Vec<MutationId>,
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
    /// # Errors
    ///
    /// A storage failure, or a transport failure where a request was actually attempted. Being
    /// offline is not an error: it returns [`SyncPass::Offline`] with the queue untouched.
    pub async fn sync_once(&self) -> Result<SyncReport, Error> {
        if self.in_flight.get() {
            return Ok(SyncReport::ended(SyncPass::AlreadyRunning));
        }
        self.in_flight.set(true);
        let result = self.run().await;
        self.in_flight.set(false);
        result
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
        let request = MutationBatchRequest {
            mutations: records.iter().map(|record| record.to_intent()).collect(),
        };

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

        let known: HashMap<MutationId, ()> = records
            .iter()
            .map(|record| (record.mutation_id, ()))
            .collect();

        let mut counts = SyncOutcomeCounts::default();
        let mut outcomes = Vec::with_capacity(response.results.len());
        let mut anomalies = Vec::new();
        let mut ruled: HashSet<MutationId> = HashSet::with_capacity(response.results.len());

        for result in response.results {
            if !known.contains_key(&result.mutation_id) || !ruled.insert(result.mutation_id) {
                anomalies.push(result.mutation_id);
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
            };
            outcomes.push(Outcome::new(result.mutation_id, disposition));
        }

        // Retain outcomes are included deliberately: a backend can then check that every record it
        // handed out was accounted for, rather than inferring silence to mean "leave it alone".
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

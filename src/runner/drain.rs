//! Drain until the queue stops draining.
//!
//! One [`sync_once`](SyncRunner::sync_once) sends one batch. The loop that keeps calling it is what
//! turns a backlog into a drained queue, and *how often* it calls decides whether that takes seconds
//! or hours: at `batch_limit = 1` the source's five-second poll interval turns a 200-record backlog
//! into roughly seventeen minutes, against the twenty seconds a back-to-back drain costs
//! (`wiki/decisions/018-single-flight-drain-mode.decision.md`).
//!
//! This is that loop. It lives in core rather than in a framework adapter because it needs nothing
//! a framework supplies — no timer, no executor, no signal — and because it is where a caller can
//! aggregate (`wiki/decisions/028-drain-loop-boundary.decision.md`).

use serde::Serialize;

use crate::error::Error;
use crate::store::OutboxStore;
use crate::transport::SyncTransport;

use super::report::{Anomaly, Drained, SyncOutcomeCounts, SyncPass, SyncReport};
use super::SyncRunner;

/// Why a drain stopped.
///
/// Every variant is a reason to stop trying *now*, not a verdict on the queue. A caller resumes by
/// calling [`drain`](SyncRunner::drain) again on whatever cadence it keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum DrainEnd {
    /// Nothing was left to send.
    Drained,
    /// A pass sent work and drained none of it, so sending again would send the same records.
    ///
    /// The queue is not empty and the drain stopped anyway. [`drain`](SyncRunner::drain) explains
    /// under *Why it stops here* why that is the right answer rather than a reason to try harder.
    Stalled,
    /// No request could be attempted. Work is untouched, and this is not a failed attempt.
    Offline,
    /// A pass was already in flight on this scope, so this drain did nothing.
    AlreadyRunning,
}

/// What a drain did, summed over the passes it ran.
///
/// # Two of these fields count sends, not records
///
/// [`sent`](DrainReport::sent) and [`retained`](DrainReport::retained) are totals over passes: a
/// record the server left queued in three passes contributes three to each. That is the honest
/// reading of "how much work did this drain do", and it is **not** an answer to "how much is left".
///
/// The counts inside [`counts`](DrainReport::counts) are per-record for
/// [`applied`](SyncOutcomeCounts::applied), [`duplicate`](SyncOutcomeCounts::duplicate) and
/// [`dead_lettered`](SyncOutcomeCounts::dead_lettered), because each of those removes the record
/// from the queue and so can happen to it once. The other three —
/// [`blocked`](SyncOutcomeCounts::blocked), [`pending`](SyncOutcomeCounts::pending),
/// [`unknown_status`](SyncOutcomeCounts::unknown_status) — leave the record queued and are counted
/// per send, exactly like `retained`.
///
/// For what is still queued, ask the store: [`OutboxStore::pending_count`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[non_exhaustive]
pub struct DrainReport {
    /// How many passes ran, including the one that ended the drain.
    ///
    /// Never zero: a drain always runs at least one pass, even against an empty queue.
    pub passes: usize,
    /// Why the drain stopped.
    pub ended: DrainEnd,
    /// How many records were sent, summed over passes.
    pub sent: usize,
    /// What the server's verdicts accounted for, summed over passes.
    pub counts: SyncOutcomeCounts,
    /// How many sent records were still queued afterwards, summed over passes.
    pub retained: usize,
    /// How many undecodable rows moved to quarantine, summed over passes.
    ///
    /// A sweep runs at the start of every pass, so in practice a drain finds them all in its first
    /// one and this equals that pass's count.
    pub quarantined: usize,
    /// Every verdict that changed nothing, in the order the passes reported them.
    pub anomalies: Vec<Anomaly>,
    /// Every record that left the queue, in pass order.
    ///
    /// Concatenated across passes, which is the same aggregation rule the counts follow. **Per
    /// record**, so unlike [`sent`](DrainReport::sent) and [`retained`](DrainReport::retained) it
    /// answers "how much is gone" rather than "how many sends happened"
    /// (`wiki/decisions/035-reports-name-what-drained.decision.md`).
    ///
    /// Its length equals `counts.applied + counts.duplicate + counts.dead_lettered`, and a
    /// conformance case holds the two to that agreement.
    pub drained: Vec<Drained>,
}

impl DrainReport {
    /// An empty report, before any pass has run.
    ///
    /// `ended` is provisional: every exit path in [`drain`](SyncRunner::drain) overwrites it, and
    /// `Drained` is what a drain against an empty queue genuinely reports.
    fn started() -> Self {
        Self {
            passes: 0,
            ended: DrainEnd::Drained,
            sent: 0,
            counts: SyncOutcomeCounts::default(),
            retained: 0,
            quarantined: 0,
            anomalies: Vec::new(),
            drained: Vec::new(),
        }
    }

    /// Fold one pass in, and hand back what the loop needs to decide whether to continue.
    ///
    /// The pass is consumed because its anomalies are moved rather than cloned, so the two values
    /// the caller still needs come back out rather than being read off a borrow that no longer
    /// exists.
    fn absorb(&mut self, pass: SyncReport) -> (SyncPass, bool) {
        let ended = pass.pass;
        let progressed = pass.made_progress();

        self.passes += 1;
        self.sent += pass.sent;
        self.retained += pass.retained;
        self.quarantined += pass.quarantined;
        self.counts.applied += pass.counts.applied;
        self.counts.duplicate += pass.counts.duplicate;
        self.counts.dead_lettered += pass.counts.dead_lettered;
        self.counts.blocked += pass.counts.blocked;
        self.counts.pending += pass.counts.pending;
        self.counts.unknown_status += pass.counts.unknown_status;
        self.anomalies.extend(pass.anomalies);
        self.drained.extend(pass.drained);

        (ended, progressed)
    }

    /// Whether the queue actually got shorter.
    ///
    /// True when at least one record was applied, deduplicated, or dead-lettered across the whole
    /// drain. The counterpart of [`SyncReport::made_progress`], and false for every
    /// [`DrainEnd`] except [`Drained`](DrainEnd::Drained) — where it is still false if the queue was
    /// already empty.
    pub fn made_progress(&self) -> bool {
        self.counts.applied + self.counts.duplicate + self.counts.dead_lettered > 0
    }
}

impl<S, T> SyncRunner<S, T>
where
    S: OutboxStore,
    T: SyncTransport,
{
    /// Run passes back to back until the queue stops draining.
    ///
    /// The loop [`sync_once`](SyncRunner::sync_once) always needed and never had. It sleeps for
    /// nothing, needs no executor and no timer, and returns one [`DrainReport`] covering every pass
    /// it ran.
    ///
    /// # Why it stops here
    ///
    /// A pass continues the drain only if it **made progress** — at least one record applied,
    /// deduplicated, or dead-lettered. Anything else stops it:
    ///
    /// | Pass | `DrainEnd` |
    /// | --- | --- |
    /// | [`Idle`](SyncPass::Idle) | [`Drained`](DrainEnd::Drained) |
    /// | [`Completed`](SyncPass::Completed) draining nothing | [`Stalled`](DrainEnd::Stalled) |
    /// | [`Offline`](SyncPass::Offline) | [`Offline`](DrainEnd::Offline) |
    /// | [`AlreadyRunning`](SyncPass::AlreadyRunning) | [`AlreadyRunning`](DrainEnd::AlreadyRunning) |
    ///
    /// Stopping on `Idle` alone would not be enough, and the reason is the interesting one. A pass
    /// that drained nothing leaves the queue exactly as it found it, so
    /// [`pending_batch`](OutboxStore::pending_batch) returns the same records and the next pass
    /// sends the same request. Going around again cannot help; it can only hurt.
    ///
    /// It hurts in a specific way. A [`retention bound`](SyncRunner::with_retention_bound) counts
    /// verdicts *received*, and a back-to-back loop receives them as fast as the network allows — so
    /// a bound of eight against a wedged head would become eight requests in a few hundred
    /// milliseconds and then a dead letter. The bound means "give the server eight chances", and a
    /// loop that spends all eight inside one drain destroys that meaning. The bound is a mechanism
    /// across a *caller's cadence*, not something a drain burns through
    /// (`wiki/decisions/029-drain-termination.decision.md`).
    ///
    /// # Why it terminates
    ///
    /// Not by a pass cap. Every pass that continues the loop removed at least one record from the
    /// outbox — [`Delete`](crate::store::Disposition::Delete) and
    /// [`DeadLetter`](crate::store::Disposition::DeadLetter) both do, and those are the only three
    /// counts [`made_progress`](SyncReport::made_progress) sums — so the queue is strictly shorter
    /// every time round. A store where that is not true is a store already violating
    /// [`apply_outcomes`](OutboxStore::apply_outcomes).
    ///
    /// Work enqueued *during* a drain is drained by it, which is the useful behaviour rather than a
    /// hazard: the loop is bounded by the queue, and the queue is finite whenever enqueueing stops.
    ///
    /// # Concurrency
    ///
    /// A drain holds no claim of its own; each pass takes and releases the same per-scope claim
    /// `sync_once` does. Two overlapping drains therefore interleave passes rather than double-send
    /// a batch, and each returns the aggregate of the passes it personally ran. A drain that starts
    /// while a pass is in flight *on that scope* ends immediately as
    /// [`AlreadyRunning`](DrainEnd::AlreadyRunning) with `passes: 1` — including a drain driven by
    /// a different runner over a second handle on the same queue.
    ///
    /// # Errors
    ///
    /// A failing pass aborts the drain and its error is returned, so the aggregate of the passes
    /// before it is lost. Those passes committed their outcomes atomically and the store still
    /// reflects them — the same trade [`sync_once`](SyncRunner::sync_once) makes for a cancelled
    /// pass, where the work survives and only the return value does not.
    pub async fn drain(&self) -> Result<DrainReport, Error> {
        let mut report = DrainReport::started();
        loop {
            let (pass, progressed) = report.absorb(self.sync_once().await?);
            report.ended = match pass {
                // The only arm that does not end the drain.
                SyncPass::Completed if progressed => continue,
                SyncPass::Completed => DrainEnd::Stalled,
                SyncPass::Idle => DrainEnd::Drained,
                SyncPass::Offline => DrainEnd::Offline,
                SyncPass::AlreadyRunning => DrainEnd::AlreadyRunning,
            };
            return Ok(report);
        }
    }
}

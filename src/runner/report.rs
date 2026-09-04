//! What one sync pass reports back.
//!
//! Split from the runner because these are the types a *caller* holds, not machinery the pass
//! needs. [`DrainReport`](super::DrainReport) folds a sequence of these into one; what is here is
//! the single pass's account of itself.

use serde::Serialize;

use crate::id::MutationId;
use crate::record::RowRef;

/// How a record left the queue.
///
/// Exactly the three terminal dispositions — the ones
/// [`made_progress`](SyncReport::made_progress) counts. A retained record has not left and does not
/// appear (`wiki/decisions/035-reports-name-what-drained.decision.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum DrainedAs {
    /// The server applied it.
    Applied,
    /// The server had already seen it.
    Duplicate,
    /// The server terminally refused it, or it reached the retention bound.
    ///
    /// **It left the queue; its write did not happen.** A caller clearing a "saving…" marker on
    /// this without saying so is telling the user their edit landed.
    DeadLettered,
}

/// One record that left the queue, and what became of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Drained {
    /// Which mutation.
    pub mutation_id: MutationId,
    /// How it left.
    pub outcome: DrainedAs,
    /// Which read-model row it was about, if the caller bound one at enqueue.
    ///
    /// `None` for an unbound mutation, which is the entire behaviour change for a caller that does
    /// not use [`MutationIntent::with_row`](crate::record::MutationIntent::with_row).
    pub row: Option<RowRef>,
}

impl Drained {
    /// Record that one mutation left the queue.
    pub fn new(mutation_id: MutationId, outcome: DrainedAs, row: Option<RowRef>) -> Self {
        Self {
            mutation_id,
            outcome,
            row,
        }
    }
}

/// How a sync pass ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum SyncPass {
    /// A batch was sent and the server's verdicts were applied.
    Completed,
    /// Nothing was pending.
    Idle,
    /// No request could be attempted. Work is untouched, and this is not a failed attempt.
    Offline,
    /// A pass was already in flight on this scope, so this call did nothing.
    ///
    /// Not only "this runner is busy": the claim is on the scope, so a second handle opened under
    /// the same key reports this rather than draining the same queue alongside the first
    /// (`wiki/decisions/031-cross-realm-single-flight.decision.md`).
    AlreadyRunning,
}

/// How many records each verdict accounted for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
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
    /// `blocked + pending + unknown` can be read against [`retained`](SyncReport::retained) without
    /// walking the anomaly list.
    ///
    /// **The two agree only when no record hit its retention bound in the same pass.** A verdict
    /// that would have retained a record is counted here by the status the server sent, and then
    /// decision 017's bound converts that retain into a dead letter — so the record is counted in
    /// [`blocked`](SyncOutcomeCounts::blocked) (or `pending`, or `unknown_status`) *and* in
    /// [`dead_lettered`](SyncOutcomeCounts::dead_lettered), while
    /// [`SyncReport::retained`] is one lower than the sum.
    ///
    /// That is the honest reading rather than a defect: these three say what the *server*
    /// answered, and `retained` says what the *client* did about it. They are different questions,
    /// and the retention bound is the one place the answers legitimately differ.
    pub unknown_status: usize,
}

/// A verdict that changed nothing, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize)]
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
    /// Every record that left the queue this pass, named rather than counted.
    ///
    /// The counterpart to [`counts`](SyncReport::counts): those answer "how many", this answers
    /// "which". Before it existed the reports named a mutation only when something went *wrong*,
    /// so a caller tracking per-row progress had no success event to act on and rebuilt its whole
    /// index after every drain — `wiki/plans/d4a-offline-todo-trial.plan.md`, Finding 1.
    ///
    /// **Per record, unlike [`sent`](SyncReport::sent) and [`retained`](SyncReport::retained),
    /// which are sums over sends.** A record retained twice and applied on the third pass
    /// contributes 3 to `sent` and exactly one entry here.
    pub drained: Vec<Drained>,
}

impl SyncReport {
    pub(super) fn ended(pass: SyncPass) -> Self {
        Self {
            pass,
            sent: 0,
            counts: SyncOutcomeCounts::default(),
            retained: 0,
            quarantined: 0,
            anomalies: Vec::new(),
            drained: Vec::new(),
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

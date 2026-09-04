//! The background loop, and the cadence that is this crate's actual job.

mod cadence;
mod steps;
mod wait;

pub use cadence::SyncCadence;
pub use steps::{CountsStep, SyncStep};
pub use wait::{Sleeper, Wake};

use dioxus_hooks::{use_future, use_signal};
use dioxus_signals::{Signal, UnsyncStorage, WritableExt};
use frontbox::DrainReport;

use crate::counts::OutboxCounts;

/// What the loop publishes to the component tree.
///
/// `Copy`, because every field is a [`Signal`] and signals are handles rather than values.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct SyncState {
    /// The most recent drain, or `None` before the first one finishes.
    pub last: Signal<Option<DrainReport>, UnsyncStorage>,
    /// The three store counts, refreshed after every drain.
    pub counts: Signal<OutboxCounts, UnsyncStorage>,
    /// The most recent failure, cleared by the next drain that succeeds.
    ///
    /// A string rather than [`frontbox::Error`], which is not `Clone` and so cannot be read out of
    /// a signal by value. Nothing is lost that a UI would use: being offline is not an error and
    /// arrives as [`DrainEnd::Offline`](frontbox::DrainEnd::Offline) on `last`, so what reaches
    /// here is a storage or transport failure that a person has to read.
    pub error: Signal<Option<String>, UnsyncStorage>,
}

/// Sync forever, on a cadence, publishing what happened.
///
/// The task is owned by the calling component and dropped with it. Dropping mid-drain is safe: a
/// cancelled pass releases its per-scope claim through a guard, so the scope does not wedge
/// (`SyncRunner::sync_once`).
///
/// # Why this takes closures
///
/// It used to take a shared-runner handle and call `drain` on the runner inside it, and **the only
/// application in this repository could not use it**. Two reasons, either fatal on its own: the
/// handle wanted a runner by value, and an application that owns its runtime has nothing to hand
/// over; and "sync" is rarely just `drain` — the D4a trial rebuilds a row-to-pending index
/// afterwards, so a loop that drains behind its back leaves every row marked "saving…" forever.
/// Both capabilities are closures, so the hook asks for closures
/// (`wiki/references/open-decisions.reference.md`, entry 14).
///
/// The handle itself is gone. It survived entry 14 as a convenience for "applications that let
/// Dioxus own the runner", a class that never acquired a member, and its one obvious use was a
/// correctness trap the same entry describes: two loops over one queue, each believing itself
/// alone.
///
pub fn use_sync_loop(
    sync: SyncStep,
    counts: CountsStep,
    cadence: SyncCadence,
    sleeper: Sleeper,
) -> SyncState {
    let state = SyncState {
        last: use_signal(|| None),
        counts: use_signal(OutboxCounts::default),
        error: use_signal(|| None),
    };

    use_future(move || {
        let (sync, counts, sleeper) = (sync.clone(), counts.clone(), sleeper.clone());
        let SyncState {
            last: mut report_signal,
            counts: mut counts_signal,
            error: mut error_signal,
        } = state;

        async move {
            let mut wait = cadence.first_ms;
            loop {
                sleeper.sleep(wait).await;

                wait = match sync.run().await {
                    Ok(report) => {
                        let next = cadence.after(report.ended);
                        report_signal.set(Some(report));
                        error_signal.set(None);
                        next
                    }
                    Err(failure) => {
                        error_signal.set(Some(failure.to_string()));
                        cadence.error_ms
                    }
                };

                // Refreshed even when the sync failed. A drain runs several passes and each
                // commits its own outcomes atomically, so a failure in the third pass leaves the
                // first two applied and the counts genuinely changed.
                match counts.run().await {
                    Ok(next) => counts_signal.set(next),
                    Err(failure) => error_signal.set(Some(failure.to_string())),
                }
            }
        }
    });

    state
}

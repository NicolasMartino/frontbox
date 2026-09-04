//! The two things the loop needs from an application, as closures.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use frontbox::{DrainReport, Error};

use crate::counts::OutboxCounts;

type Step<T> = Rc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<T, Error>>>>>;

/// One round of syncing, as the application defines it.
///
/// **Not necessarily `SyncRunner::drain`.** An application usually wraps a drain in something
/// larger — the D4a trial rebuilds a row-to-pending index afterwards, and routing the loop past
/// that leaves every row's "saving…" marker set on enqueue and never cleared. A hook that called
/// `drain` itself would be draining behind the application's back, which is what
/// `wiki/references/open-decisions.reference.md` entry 14 records.
///
/// `Rc`-boxed for the same reason [`Sleeper`](crate::Sleeper) is: the hook's signature stays
/// readable and the closure clones per render.
#[derive(Clone)]
pub struct SyncStep(Step<DrainReport>);

impl SyncStep {
    /// Build one from whatever "sync" means to this application.
    pub fn new<F, Fut>(step: F) -> Self
    where
        F: Fn() -> Fut + 'static,
        Fut: Future<Output = Result<DrainReport, Error>> + 'static,
    {
        Self(Rc::new(move || {
            Box::pin(step()) as Pin<Box<dyn Future<Output = _>>>
        }))
    }

    pub(crate) async fn run(&self) -> Result<DrainReport, Error> {
        (self.0)().await
    }
}

impl std::fmt::Debug for SyncStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SyncStep(..)")
    }
}

/// How the loop reads the three counts back.
///
/// Separate from [`SyncStep`] because it runs on a different schedule: the counts are refreshed
/// even when the sync *failed*, since a drain runs several passes and each commits its own outcomes
/// atomically, so a failure in the third leaves the first two applied and the numbers genuinely
/// changed.
#[derive(Clone)]
pub struct CountsStep(Step<OutboxCounts>);

impl CountsStep {
    /// Build one from whatever the application reads counts out of.
    pub fn new<F, Fut>(step: F) -> Self
    where
        F: Fn() -> Fut + 'static,
        Fut: Future<Output = Result<OutboxCounts, Error>> + 'static,
    {
        Self(Rc::new(move || {
            Box::pin(step()) as Pin<Box<dyn Future<Output = _>>>
        }))
    }

    pub(crate) async fn run(&self) -> Result<OutboxCounts, Error> {
        (self.0)().await
    }
}

impl std::fmt::Debug for CountsStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CountsStep(..)")
    }
}

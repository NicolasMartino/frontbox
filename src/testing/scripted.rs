//! A scripted [`SyncTransport`] for the conformance cases.
//!
//! Kept out of `super` because the suite's server-side scaffolding is a self-contained
//! concern: what a server may answer, including the answers a real server should never give.

use std::cell::RefCell;
use std::collections::VecDeque;

use crate::error::Error;
use crate::protocol::{
    MutationBatchRequest, MutationBatchResponse, MutationResult, MutationStatus,
};
use crate::transport::SyncTransport;

/// What a [`ScriptedTransport`] answers with.
#[derive(Debug, Clone)]
pub enum Reply {
    /// Give every mutation in the batch the same status.
    All(MutationStatus),
    /// Give the mutation at each position the matching status.
    ///
    /// Positions beyond the end of the list get no result at all, which is a server declining to
    /// rule on them.
    ByPosition(Vec<MutationStatus>),
    /// Answer with exactly these results, whatever the batch contained.
    ///
    /// The only way to produce a verdict for a mutation that was never sent.
    Exact(Vec<MutationResult>),
    /// Answer a *called* `send_batch` with [`Error::Offline`].
    ///
    /// The batch has already been read and handed over by the time this is produced, so the records
    /// in it are transport-started. For an offline that is known before any read, use
    /// [`ScriptedTransport::reporting_offline`].
    Offline,
    /// Answer a called `send_batch` with [`Error::Transport`]: a request was attempted and failed.
    TransportFailure,
}

/// A transport that answers from a script and records what it was asked.
pub struct ScriptedTransport {
    default: Reply,
    scripted: RefCell<VecDeque<Reply>>,
    sent: RefCell<Vec<MutationBatchRequest>>,
    yielding: std::cell::Cell<bool>,
    offline_now: std::cell::Cell<bool>,
    probes: std::cell::Cell<usize>,
}

impl ScriptedTransport {
    /// Build a transport that answers every batch with `default`.
    pub fn new(default: Reply) -> Self {
        Self {
            default,
            scripted: RefCell::new(VecDeque::new()),
            sent: RefCell::new(Vec::new()),
            yielding: std::cell::Cell::new(false),
            offline_now: std::cell::Cell::new(false),
            probes: std::cell::Cell::new(0),
        }
    }

    /// Answer `SyncTransport::offline_now` with `true`.
    ///
    /// Distinct from [`Reply::Offline`], and the distinction is the point: this one is known
    /// *before* a batch is read, so a pass that gets it never reads and never marks. `Reply::Offline`
    /// is a request that was already handed over.
    #[must_use]
    pub fn reporting_offline(self) -> Self {
        self.offline_now.set(true);
        self
    }

    /// How many times the runner asked whether the application was offline.
    pub fn probe_count(&self) -> usize {
        self.probes.get()
    }

    /// Suspend once before answering.
    ///
    /// A sync pass that never yields cannot be observed mid-flight, so the re-entrancy case needs a
    /// transport that actually gives the executor a chance to poll something else.
    #[must_use]
    pub fn yielding(self) -> Self {
        self.yielding.set(true);
        self
    }

    /// Queue a reply for the next batch, ahead of the default.
    #[must_use]
    pub fn then(self, reply: Reply) -> Self {
        self.scripted.borrow_mut().push_back(reply);
        self
    }

    /// Every batch this transport was asked to send, in order.
    pub fn sent(&self) -> Vec<MutationBatchRequest> {
        self.sent.borrow().clone()
    }

    /// How many batches this transport was asked to send.
    pub fn send_count(&self) -> usize {
        self.sent.borrow().len()
    }
}

impl SyncTransport for ScriptedTransport {
    async fn offline_now(&self) -> Result<bool, Error> {
        self.probes.set(self.probes.get() + 1);
        Ok(self.offline_now.get())
    }

    async fn send_batch(
        &self,
        request: MutationBatchRequest,
    ) -> Result<MutationBatchResponse, Error> {
        self.sent.borrow_mut().push(request.clone());

        if self.yielding.get() {
            YieldOnce(false).await;
        }

        let reply = self
            .scripted
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| self.default.clone());

        let results = match reply {
            Reply::Offline => return Err(Error::Offline),
            Reply::TransportFailure => return Err(Error::transport_opaque()),
            Reply::Exact(results) => results,
            Reply::All(status) => request
                .mutations
                .iter()
                .map(|m| MutationResult::new(m.mutation_id, status.clone()))
                .collect(),
            Reply::ByPosition(statuses) => request
                .mutations
                .iter()
                .zip(statuses)
                .map(|(m, status)| MutationResult::new(m.mutation_id, status))
                .collect(),
        };
        Ok(MutationBatchResponse::new(results))
    }
}

/// A future that is pending exactly once.
struct YieldOnce(bool);

impl std::future::Future for YieldOnce {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if self.0 {
            std::task::Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    }
}

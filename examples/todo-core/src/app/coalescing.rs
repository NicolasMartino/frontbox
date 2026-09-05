//! The trial's use of `enqueue_coalescing`, kept beside the plain path rather than replacing it.
//!
//! [`TodoApp::rename`](super::TodoApp::rename) still appends. This module adds a second rename that
//! coalesces, and **both stay** — a trial that only had the coalescing one could show the feature
//! working but not show what it changes, and the difference between one queued write and two is the
//! whole claim (`wiki/decisions/044-transport-started-before-the-request.decision.md`).
//!
//! # What this trial can and cannot demonstrate
//!
//! It demonstrates the user-visible effect: rename a todo twice with no connectivity, and the server
//! eventually sees one `PUT` carrying the second title rather than two `PUT`s carrying both.
//!
//! It does **not** demonstrate the motivating case for
//! [`CoalescingPolicy::RequireExisting`](frontbox::CoalescingPolicy::RequireExisting), because this
//! trial sends no preconditions at all. RepForge's difficulty is that a profile row's hash covers a
//! server-assigned `updated_at`, so the state write 1 will produce is unknowable until write 1
//! drains and a second write has no honest precondition to queue behind it. Nothing here has that
//! shape, and inventing one would be a fixture arranged to flatter the feature rather than an
//! application that needed it.
//!
//! So the trial uses `AppendIfMissing`, which is the policy an application with a valid precondition
//! — or none — should reach for: it collapses when it safely can and queues normally when it cannot,
//! and it can never answer `NotQueued`.

use frontbox::{
    CoalescingEnqueue, CoalescingPolicy, Error, MutationId, MutationIntent, OperationMeta,
    OutboxStore, RowRef,
};

use super::{TodoApp, TODO};
use crate::store::Change;
use crate::trace;

impl TodoApp {
    /// Rename a todo, replacing an unsent rename of the same row instead of queueing behind it.
    ///
    /// The same user-visible action as [`rename`](TodoApp::rename), and the same projection. What
    /// differs is the queue: two of these while offline leave one pending write carrying the later
    /// title, so the server is told what the user settled on rather than replayed the path they took
    /// to get there.
    ///
    /// # When the collapse does not happen
    ///
    /// Whenever frontbox cannot prove the queued write is still unsent — most often because a drain
    /// has already read it for sending. Then this appends, exactly as `rename` would, and the server
    /// sees both writes. Nothing is lost either way; the second title still wins, because it is
    /// still the later write on the same row.
    ///
    /// This is why the application asks the transport whether it is offline before draining
    /// (`examples/todo-core/src/transport/mod.rs`'s `offline_now`): a poll while the plug is out would otherwise
    /// mark the queued rename as read-for-sending and every later edit would append.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn rename_coalescing(
        &self,
        id: &str,
        title: &str,
    ) -> Result<CoalescingEnqueue, Error> {
        trace::log(format!(
            "action rename_todo_coalescing id={id} title={title:?}"
        ));
        self.store.project(Change::Rename {
            id: id.to_owned(),
            title: title.to_owned(),
        });
        self.persist_row(id).await?;

        let created_at = self.clock.now_ms();
        let outcome = self
            .runner
            .store()
            .enqueue_coalescing(
                MutationIntent::new(
                    MutationId::new(),
                    "PUT",
                    format!("/api/v1/todos/{id}"),
                    serde_json::json!({ "title": title }),
                    created_at,
                )
                .with_op(OperationMeta::new("rename_todo").with_version("1"))
                // The row binding is what coalescing matches on, so this is not optional here the
                // way it is on an ordinary enqueue: without it there is nothing to say which queued
                // write this is a newer version of, and `AppendIfMissing` would simply append.
                .with_row(RowRef::new(TODO, id)),
                CoalescingPolicy::AppendIfMissing,
            )
            .await?;

        trace::log(match &outcome {
            CoalescingEnqueue::Replaced { kept, .. } => {
                format!("coalesced rename id={id} into={kept}")
            }
            CoalescingEnqueue::Appended { mutation_id } => {
                format!("appended rename id={id} as={mutation_id}")
            }
            // `AppendIfMissing` cannot produce this, and the trial says so here rather than
            // pretending the branch is reachable.
            other => format!("unexpected coalescing outcome id={id} outcome={other:?}"),
        });

        self.store.mark_pending(id);
        Ok(outcome)
    }
}

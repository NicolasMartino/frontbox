//! Posting one destination's share of a batch, and the one HTTP status this transport reads.
//!
//! Split out of [`super`] when it passed the four-hundred-line cap `AGENTS.md` sets. The seam is
//! the one the module header already draws: next door decides *which* destination each mutation
//! belongs to and what a batch of them means, and this is the single request that follows.

use frontbox::{
    Error, MutationBatchRequest, MutationBatchResponse, MutationResult, MutationStatus,
};

use super::error::map_send_error;
use super::HttpTransport;
use crate::trace;

impl HttpTransport {
    /// Post one destination's share of a batch.
    pub(super) async fn send_one(
        &self,
        index: usize,
        mutations: Vec<frontbox::MutationIntent>,
    ) -> Result<Vec<MutationResult>, Error> {
        self.request_attempts.set(self.request_attempts.get() + 1);
        let url = format!("{}/api/v1/sync", self.routes[index].base_url);
        trace::log(format!(
            "sync send destination={index} url={url} mutations={}",
            mutations.len()
        ));

        // Fresh per send. Real credentials would be resolved here and nowhere else, because a token
        // captured at enqueue is a token that has expired by the time an offline queue drains
        // (`wiki/decisions/004-transport-auth-and-offline.decision.md`).
        let response = match self
            .client
            .post(&url)
            .json(&MutationBatchRequest::new(mutations.clone()))
            .send()
            .await
        {
            Ok(response) => {
                trace::log(format!(
                    "sync response destination={index} url={url} status={}",
                    response.status()
                ));
                response
            }
            Err(source) => {
                let mapped = map_send_error(source);
                trace::log(format!(
                    "sync error destination={index} url={url} error={mapped}"
                ));
                return Err(mapped);
            }
        };

        // **The one HTTP status this transport does classify, and decision 019 is why it may.**
        //
        // 019 opens by saying core does not classify HTTP responses and will not. What it places
        // on a *transport* is the obligation that a missing prerequisite is transient and must
        // never become `Rejected`. A 404 from the batch endpoint means a mutation in this group
        // references a record another service has not accepted yet — which is the condition 019
        // describes for `Blocked` in as many words: "this write failed only because a predecessor
        // had not landed".
        //
        // `Blocked` rather than `Unknown`: `Unknown` is reserved for a response the transport
        // *cannot* confidently classify, and this one it can. The consequence is what a test
        // should assert on — the record is retained with `reason: Some("blocked")` and counted in
        // `counts.blocked`, and it does **not** reach the anomaly surface, which only `Unknown`
        // raises.
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            trace::log(format!(
                "sync blocked destination={index} mutations={}",
                mutations.len()
            ));
            return Ok(mutations
                .iter()
                .map(|m| MutationResult::new(m.mutation_id, MutationStatus::Blocked))
                .collect());
        }

        if !response.status().is_success() {
            // Every other status stays unclassified, deliberately. A 401 means "retry with a fresh
            // token", not "terminally refused"; failing this destination leaves its records queued,
            // which is the only answer that assumes nothing.
            return Err(Error::protocol(format!(
                "sync endpoint answered {}",
                response.status()
            )));
        }

        response
            .json::<MutationBatchResponse>()
            .await
            .map(|response| response.results)
            .map_err(Error::transport)
    }
}

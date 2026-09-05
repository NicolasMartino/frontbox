//! A routing `SyncTransport` over real HTTP, and a switch for pulling the plug.
//!
//! # Where a multi-service client puts its routing
//!
//! Here, and nowhere else. Decision 037 settles it: **one outbox under one scope, and routing
//! inside `SyncTransport`.** Core gains no origin column, no per-service queue, and no routing
//! concept, which is the join of decision 034 ("a service is a transport concern, not a record
//! one") and decision 036 (which refused to partition the outbox precisely to keep enqueue order
//! across types).
//!
//! The cost lands here too. `send_batch` takes one batch and returns one response, so a routing
//! transport has to split by destination, issue a call each, and merge. The awkward case is
//! partial — one server answers, another is unreachable — and **core already models it**: a record
//! the response does not name is retained with `reason: Some("no verdict returned")` and its
//! `attempts` incremented. So this transport returns the verdicts it obtained and stays silent
//! about the rest, and needs no protocol change to do it.

use std::cell::{Cell, RefCell};

use frontbox::{Error, MutationBatchRequest, MutationBatchResponse, MutationId, SyncTransport};

use crate::trace;

mod error;
mod send;
mod shared;

use error::map_send_error;
pub use shared::SharedTransport;

/// One destination, and the path prefix that selects it.
struct Route {
    prefix: String,
    base_url: String,
}

/// Posts batches to the trial's servers, one call per destination.
pub struct HttpTransport {
    client: reqwest::Client,
    /// Checked in order; the first prefix that matches a path wins, and the last route is the
    /// fallback. Order is explicit rather than longest-match so a reader can see which wins.
    routes: Vec<Route>,
    /// Pretend the network is gone.
    ///
    /// A real client learns this from the platform — `navigator.onLine`, a connection error, a
    /// captive portal. The trial needs it deterministic, and the distinction it exercises is real:
    /// `Error::Offline` leaves the queue untouched and is *not* a failed attempt
    /// (`wiki/decisions/004-transport-auth-and-offline.decision.md`), where any other transport
    /// error is.
    offline: Cell<bool>,
    /// Every HTTP request this transport attempted, for the observations to count.
    request_attempts: Cell<usize>,
    /// Why the last batch reached no destination, when that was a failure rather than silence.
    ///
    /// # Why this exists at all
    ///
    /// A batch whose destinations all answered with a status returns `Ok` with no verdicts, for
    /// the reasons written at the end of `send_batch` — the records are retained and the attempt is
    /// counted, which is the bounded treatment. The cost is that such a pass is indistinguishable
    /// from a healthy one that had nothing to say: same `Ok`, same empty verdict list, same report.
    /// A worktree review read that as records being silently retained forever, which it is not, and
    /// the reason it could be read that way is that nothing said otherwise.
    ///
    /// So the fact is recorded rather than the control flow changed. It is a *condition* — this
    /// endpoint is failing, and still is — which is the status bar's half of decision 042 and not a
    /// toast's.
    last_send_failure: RefCell<Option<String>>,
}

impl HttpTransport {
    /// Build a transport pointing at one server, which answers for every path.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            routes: vec![Route {
                prefix: String::new(),
                base_url: base_url.into(),
            }],
            offline: Cell::new(false),
            request_attempts: Cell::new(0),
            last_send_failure: RefCell::new(None),
        }
    }

    /// Send paths starting with `prefix` to `base_url` instead of the default.
    ///
    /// Inserted ahead of the existing routes, so the fallback stays last however many are added.
    #[must_use]
    pub fn routing(mut self, prefix: impl Into<String>, base_url: impl Into<String>) -> Self {
        self.routes.insert(
            0,
            Route {
                prefix: prefix.into(),
                base_url: base_url.into(),
            },
        );
        self
    }

    /// Which destination a path belongs to, as an index into `routes`.
    ///
    /// Always answers: the fallback route has an empty prefix, which every path starts with. A
    /// transport that could fail to route would need a verdict for "I do not know where this goes",
    /// and the protocol has no honest one — so the fallback is a constructor invariant instead.
    fn destination(&self, path: &str) -> usize {
        self.routes
            .iter()
            .position(|route| path.starts_with(&route.prefix))
            .unwrap_or(self.routes.len() - 1)
    }

    /// Pull the plug, or plug it back in.
    pub fn set_offline(&self, offline: bool) {
        trace::log(format!("transport offline={offline}"));
        self.offline.set(offline);
    }

    /// How many non-offline HTTP requests this transport attempted.
    pub fn request_attempts(&self) -> usize {
        self.request_attempts.get()
    }

    /// Why the last batch reached no destination, if it failed rather than being offline.
    ///
    /// `None` after any pass that reached a destination, including a partial one. A pass that
    /// attempted **nothing** — every destination offline — leaves it as it was: nothing was sent,
    /// so nothing was learned about the endpoint, and clearing it there would say the destination
    /// had recovered when it had not been asked.
    ///
    /// A *mixed* pass is not that. One destination unreachable and another answering with a status
    /// did learn something about the one that answered, and it is recorded even though the pass
    /// reports offline.
    #[must_use]
    pub fn last_send_failure(&self) -> Option<String> {
        self.last_send_failure.borrow().clone()
    }

    /// Send a direct JSON write through the same client and offline switch as batch sync.
    ///
    /// # Errors
    ///
    /// Offline before any request was attempted, or a transport failure while sending.
    pub async fn put_json(
        &self,
        path: &str,
        mutation_id: MutationId,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response, Error> {
        self.request(reqwest::Method::PUT, path, Some(mutation_id), Some(body))
            .await
    }

    /// Fetch JSON through the same client and offline switch as batch sync.
    ///
    /// # Errors
    ///
    /// Offline, a non-success HTTP response, or a transport failure while sending.
    pub async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        let response = self.request(reqwest::Method::GET, path, None, None).await?;
        if !response.status().is_success() {
            return Err(Error::protocol(format!(
                "read endpoint answered {}",
                response.status()
            )));
        }
        response.json().await.map_err(Error::transport)
    }

    /// Fetch JSON from an explicitly named service, bypassing the routing table.
    ///
    /// # Why a source names its host
    ///
    /// Both services answer `GET /api/v1/versions`, so a path cannot say which is meant. The
    /// routing table places *mutations*, whose paths are domain-specific by construction; a
    /// versions endpoint is per-service instead. An invalidation source therefore carries its base
    /// URL, which is the honest shape — a source speaks for one origin — and it still goes through
    /// this transport so that the offline switch covers polling as well as writing.
    ///
    /// # Errors
    ///
    /// Offline, a non-success HTTP response, or a transport failure while sending.
    pub async fn get_json_at<T: serde::de::DeserializeOwned>(
        &self,
        base_url: &str,
        path: &str,
    ) -> Result<T, Error> {
        if self.offline.get() {
            trace::log(format!("http offline method=GET url={base_url}{path}"));
            return Err(Error::Offline);
        }
        self.request_attempts.set(self.request_attempts.get() + 1);
        let url = format!("{base_url}{path}");
        trace::log(format!("http request method=GET url={url}"));
        let response = match self.client.get(&url).send().await {
            Ok(response) => {
                trace::log(format!(
                    "http response method=GET url={url} status={}",
                    response.status()
                ));
                response
            }
            Err(source) => {
                let mapped = map_send_error(source);
                trace::log(format!("http error method=GET url={url} error={mapped}"));
                return Err(mapped);
            }
        };
        if !response.status().is_success() {
            return Err(Error::protocol(format!(
                "versions endpoint answered {}",
                response.status()
            )));
        }
        response.json().await.map_err(Error::transport)
    }

    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        mutation_id: Option<MutationId>,
        body: Option<&serde_json::Value>,
    ) -> Result<reqwest::Response, Error> {
        if self.offline.get() {
            trace::log(format!("http offline method={method} path={path}"));
            return Err(Error::Offline);
        }
        self.request_attempts.set(self.request_attempts.get() + 1);

        let url = self.url(path);
        trace::log(format!("http request method={method} url={url}"));
        let mut builder = self.client.request(method.clone(), &url);
        if let Some(mutation_id) = mutation_id {
            builder = builder.header("x-mutation-id", mutation_id.to_string());
        }
        if let Some(body) = body {
            builder = builder.json(body);
        }
        match builder.send().await {
            Ok(response) => {
                trace::log(format!(
                    "http response method={method} url={url} status={}",
                    response.status()
                ));
                Ok(response)
            }
            Err(source) => {
                let mapped = map_send_error(source);
                trace::log(format!(
                    "http error method={method} url={url} error={mapped}"
                ));
                Err(mapped)
            }
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.routes[self.destination(path)].base_url, path)
    }
}

impl SyncTransport for HttpTransport {
    /// The switch, asked before the runner reads a batch rather than after it has one.
    ///
    /// This transport is the easy case for the probe and shows why it exists: the plug is *known*
    /// to be out — a test pulled it, or the UI did — so there is nothing to infer and no request to
    /// attempt. Answering here means `read_for_send` is never called, so nothing is marked
    /// transport-started and the queue stays coalescible.
    ///
    /// Without it, `send_batch` below would still report offline correctly and the queue would
    /// still be safe, but every poll while offline would spend the head batch's eligibility for
    /// `enqueue_coalescing` — which is the eligibility the feature exists to use
    /// (`wiki/decisions/044-transport-started-before-the-request.decision.md`).
    ///
    /// A real transport with no such switch returns the default `false` and loses only the
    /// coalescing, never correctness.
    async fn offline_now(&self) -> Result<bool, Error> {
        Ok(self.offline.get())
    }

    async fn send_batch(
        &self,
        request: MutationBatchRequest,
    ) -> Result<MutationBatchResponse, Error> {
        if self.offline.get() {
            trace::log("sync batch offline");
            return Err(Error::Offline);
        }

        // Split by destination, preserving order inside each group. The *cross-group* order is
        // already guaranteed by the one thing decision 036 paid for: a single `seq` on a single
        // queue means the batch arrives here in enqueue order, so a per-destination group is a
        // subsequence of it and the user record is drained before the todo that references it.
        let mut groups: Vec<Vec<frontbox::MutationIntent>> =
            (0..self.routes.len()).map(|_| Vec::new()).collect();
        for mutation in request.mutations {
            groups[self.destination(&mutation.path)].push(mutation);
        }
        trace::log(format!(
            "sync batch grouped destinations={}",
            groups.iter().filter(|group| !group.is_empty()).count()
        ));

        let mut results = Vec::new();
        let mut reached_any = false;
        let mut offline_any = false;
        // The first destination that answered with a status rather than silence, recorded so the
        // condition can be shown. See the comment on the return below for why it is not raised.
        let mut attempted_failure: Option<String> = None;

        for (index, mutations) in groups.into_iter().enumerate() {
            if mutations.is_empty() {
                continue;
            }
            match self.send_one(index, mutations).await {
                Ok(mut verdicts) => {
                    reached_any = true;
                    results.append(&mut verdicts);
                }
                // Silence for this destination, verdicts for the others. Core retains every record
                // the response does not name, with `reason: Some("no verdict returned")` and
                // `attempts` incremented, which is exactly the treatment an unreachable service
                // deserves — see this module's header.
                Err(Error::Offline) => offline_any = true,
                Err(failure) => {
                    trace::log(format!(
                        "sync destination index={index} failed error={failure}"
                    ));
                    if attempted_failure.is_none() {
                        attempted_failure = Some(failure.to_string());
                    }
                }
            }
        }

        // **Every destination offline and none reached is reported as offline; every destination
        // *failing* is not reported at all, and the asymmetry is deliberate.**
        //
        // `SyncPass::Offline` leaves the queue untouched and does not count as an attempt, so an
        // aeroplane does not burn decision 017's retention bound
        // (`wiki/decisions/004-transport-auth-and-offline.decision.md`). Reporting a partial reach
        // as offline would hide the verdicts that did arrive, hence the guard on nothing having
        // been reached.
        //
        // A batch whose destinations all answered with a status instead returns `Ok` carrying no
        // verdicts, and that reads like a bug until the alternative is followed through. Core
        // retains every record the response does not name with `reason: Some("no verdict
        // returned")` **and increments `attempts`** (`src/runner/mod.rs`), so a permanently broken
        // endpoint walks the retention bound and dead-letters like anything else. Returning `Err`
        // here would abort the pass *before* outcomes are applied — `Err(error) => return Err(error)`,
        // pending work untouched — so the attempt would never be counted and the same endpoint
        // would be retried forever. The obvious fix is the unbounded one.
        //
        // It also has to stay `Ok` for a server that refuses a batch it will accept later:
        // `observation_13_a_failed_cascade_leaves_the_user_alone` is exactly that, and a cascade
        // the user service could not run is transient by design, never terminal.
        //
        // What was genuinely missing is that none of this was *visible*: the pass looked identical
        // to a healthy one that had nothing to say. `last_send_failure` is that, and it is a
        // condition rather than an event, so it belongs on the status bar
        // (`wiki/decisions/042-events-are-toasts-conditions-are-the-status-bar.decision.md`).
        // Cleared on a clean pass rather than left to accumulate: this says whether the last batch
        // hit a failing destination, not whether one ever failed.
        //
        // **Recorded before the offline return below, which used to discard it.** A mixed batch —
        // one destination unreachable, another answering 503 — reaches nothing, so it returns
        // `Error::Offline`, and the failure that had actually been observed went unnamed. That is
        // the worst case for this field to be empty in: the pass presents as an ordinary aeroplane
        // while a genuinely broken endpoint is the thing holding the queue up.
        //
        // A pass that attempted nothing at all leaves the field alone rather than clearing it —
        // silence is not evidence of recovery.
        if reached_any {
            self.last_send_failure.replace(None);
        } else if attempted_failure.is_some() {
            self.last_send_failure.replace(attempted_failure);
        }

        if offline_any && !reached_any {
            return Err(Error::Offline);
        }

        Ok(MutationBatchResponse::new(results))
    }
}

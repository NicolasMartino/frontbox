# frontbox Wiki

**frontbox** is an offline-first cache and mutation outbox for Rust frontends.

The name is the backend outbox pattern moved to the client: a durable local queue of writes that
survives restarts, replays when connectivity returns, and routes server refusals into dead letters
instead of losing them. Alongside the queue sit local read models and version-based cache
invalidation.

This wiki is the project knowledge base for extracting that runtime out of RepForge's Dioxus
application and into a reusable library.

## Project Shape

- **Current stage:** D1 through D5 are built — D1 2026-08-26, D2 2026-08-27, D3a/D3b/D4a
  2026-08-29, D4b/D4c/D5 2026-08-30, **D4d 2026-08-31** — and **D5's last owed proof line was met
  the same day**: the two-realm drain is observed by a browser fixture. Every *deliverable* with a
  plan is built and none is carrying an unwitnessed promise. **D6** has no plan.
  `plans/queued-write-coalescing.plan.md` sits outside the D0-D6 sequence — it answers an external
  request rather than a roadmap step — and was **built 2026-09-05**. The crate is the root package of
  a workspace whose other members are `crates/frontbox-dioxus`, `crates/frontbox-sqlite`,
  `crates/frontbox-indexeddb` and the four trial crates under `examples/`; design continues to live
  in this wiki.
- **Blueprint:** library-sdk
- **Source system:** `/Users/nicolasmartino/Documents/workout/cqrs-fullstack` (RepForge)
- **Extraction target:** `/Users/nicolasmartino/Documents/rust/frontbox`
- **Primary goal:** Separate the generic client-side cache/runtime from RepForge's domain-specific
  Dioxus app.
- **Naming:** formerly `dioxus-cache`, renamed 2026-08-25. The core is framework-neutral, so
  Dioxus belongs in an adapter.

## Current Thesis

The current cache is not just a local map or request cache. It is an offline-first CQRS client
runtime with a persistent mutation outbox, local read models, dead-letter handling, server version
invalidation, SSE reconnect checks, and optimistic UI flows.

The durable outbox persists a raw HTTP envelope: `method`, `path`, and JSON `body`. That makes the
queue naturally domain-neutral. The extraction risk is not the envelope shape; it is accidentally
porting source defects such as `Blocked` dead-lettering, unbounded batches, non-atomic outcome
application, corrupt-record loss, and closed RepForge entity enums.

Correcting a source defect can introduce a new one. Retaining `Blocked` work instead of
dead-lettering it removes a data-loss bug but makes the queue's liveness a property that has to be
argued and tested rather than assumed. Divergences from the source carry that obligation.

## Catalog

| Path | Status | Summary |
| --- | --- | --- |
| [specs/source-frontend-cache-architecture.spec.md](specs/source-frontend-cache-architecture.spec.md) | Active | Source-backed architecture spec for RepForge's frontend cache: what the source system does, verified against the copied corpus. |
| [specs/frontbox-runtime.spec.md](specs/frontbox-runtime.spec.md) | Active | What the extracted library does, as built and tested: divergences from the source, behaviours the source has no position on, and the constraints these place on D5. |
| [proposals/extraction-boundary.proposal.md](proposals/extraction-boundary.proposal.md) | Accepted | Proposed split between core, storage backends, Dioxus adapter, examples, and RepForge-owned app code. |
| [proposals/single-flight-drain.proposal.md](proposals/single-flight-drain.proposal.md) | **Accepted**; 016-019 built | Response to RepForge's single-flight proposal: the ordering ask is a bug fix rather than a price, `batch_limit = 1` freezes the queue without a retention bound, and the default stays 100. |
| [proposals/repforge-read-model-convergence.proposal.md](proposals/repforge-read-model-convergence.proposal.md) | **Accepted**; 021-023 built | Answers RepForge's 2026-08-28 revision: the read model, trace context, the sink boundary, and all eight questions. |
| [proposals/dead-letter-report-and-preconditions.proposal.md](proposals/dead-letter-report-and-preconditions.proposal.md) | **Accepted**; 026-027 built | Answers RepForge's dead-letter report invitation at field level, and asks the one question that decides whether the outbox gains a precondition column. |
| [proposals/offline-todo-trial.proposal.md](proposals/offline-todo-trial.proposal.md) | **Accepted**; D4a built | The requested Dioxus todo plus axum/sqlx server, split into an API proof that runs today and a durability proof that follows D5 — because an in-memory queue cannot demonstrate the one adjective the demo exists to prove. |
| [proposals/invalidation-delivery.proposal.md](proposals/invalidation-delivery.proposal.md) | Proposed | Why "SSE is out of scope" defended the wrong boundary, the outbound/inbound asymmetry, level-triggered polling before edge-triggered streams, and who owns the resume cursor. |
| [proposals/browser-background-services.proposal.md](proposals/browser-background-services.proposal.md) | Proposed | Whether frontbox uses Background Sync, Background Fetch, or the bfcache. Fetch is the wrong tool, Sync is downstream of D5, and the question surfaced that single-flight holds per runner rather than per scope. |
| [proposals/queued-write-coalescing.proposal.md](proposals/queued-write-coalescing.proposal.md) | **Accepted**; 044 built | RepForge's opt-in same-row queued-write coalescing request, accepted in direction but corrected three times over: `attempts == 0` is not "never sent", `Error::Offline` cannot prove a request never left, and replacement must discard the queued body loudly rather than drop an unbound write in silence. |
| [roadmaps/extraction.roadmap.md](roadmaps/extraction.roadmap.md) | Active | Deliverable sequence D0, D0a, D1-D6 with proof gates, keeping the prior-art survey before implementation and the migration trial before backend ports. |
| [plans/d1-core-cache-runtime.plan.md](plans/d1-core-cache-runtime.plan.md) | Completed | The first core outbox runtime slice: shapes, conformance cases, and what implementation changed. Built 2026-08-26. |
| [plans/d2-cache-invalidation.plan.md](plans/d2-cache-invalidation.plan.md) | Completed | The second slice: generic entity keys, version reconciliation, invalidation handling, and the pending-write conflict signal. Built 2026-08-27. |
| [plans/d3-drain-and-dioxus-adapter.plan.md](plans/d3-drain-and-dioxus-adapter.plan.md) | Completed | The third slice: a drain loop in core, `Serialize` on the reports, the workspace, and the Dioxus adapter. Built 2026-08-29. |
| [plans/d4a-offline-todo-trial.plan.md](plans/d4a-offline-todo-trial.plan.md) | Completed | The migration trial's API half: a Dioxus todo over an axum/sqlx server, ten original observations, post-review regressions, and trial findings. No line of core changed. Built 2026-08-29. |
| [plans/d4c-multi-platform-trial.plan.md](plans/d4c-multi-platform-trial.plan.md) | Completed | The trial application on web, macOS desktop, iOS and Android from one component tree: what the platform seam turned out to contain, the four CSS changes mobile actually needed, and the drain loop dying on Dioxus desktop. Neither library crate was touched. Built 2026-08-30. |
| [plans/d4d-multi-domain-trial.plan.md](plans/d4d-multi-domain-trial.plan.md) | **Built**; amended 2026-09-02 | A second domain server so cross-service ordering and multi-source invalidation stop being untested guarantees, plus the first application code that ever runs the invalidation runtime. Fourteen observations; the fourteenth came out of a review and unlocked a branch the fixture's own outage switch had made unreachable. |
| [plans/d5-persistence-backends.plan.md](plans/d5-persistence-backends.plan.md) | **Completed**; last proof line met 2026-08-31 | The execution plan for durable SQLite and IndexedDB storage: both backends together, the obligations core already placed on them, the application's own read-model store, and the four decisions owed before the first durable row. |
| [plans/queued-write-coalescing.plan.md](plans/queued-write-coalescing.plan.md) | Completed | The safe implementation plan for RepForge's same-row queued-write coalescing request: one durable `transport_started` fact written before the send, an offline probe on `SyncTransport`, SQLite's first schema-versioning mechanism, and cross-backend conformance. Outside the D0-D6 sequence. |
| [plans/prior-art-survey.plan.md](plans/prior-art-survey.plan.md) | Completed | Executed D0a: compared frontbox against offline/local-first prior art before implementation is authorized. |
| [references/prior-art-survey.reference.md](references/prior-art-survey.reference.md) | Sourced | D0a prior-art comparison across two cohorts: state-replication engines (Replicache/Zero, PowerSync, Electric, RxDB, WatermelonDB, PouchDB/CouchDB, Automerge, Yjs) and the HTTP command-queue peers frontbox actually belongs to (Workbox, Redux Offline, TanStack Query, Amplify DataStore). URLs verified 2026-08-26. |
| [references/repforge-cache-source-corpus.reference.md](references/repforge-cache-source-corpus.reference.md) | Sourced | Inventory of copied RepForge source files, line counts, scope notes, and extraction value. |
| [references/source-test-inventory.reference.md](references/source-test-inventory.reference.md) | Sourced | Inventory of 130 source tests and how they map to extraction milestones. |
| [references/repforge-single-flight-proposal.reference.md](references/repforge-single-flight-proposal.reference.md) | Recorded (not in `raw/`) | The proposal RepForge architecture sent on 2026-08-27, recorded so the response and its decisions have something to cite. Received in conversation, not in `raw/`. |
| [references/open-decisions.reference.md](references/open-decisions.reference.md) | Active | Every decision not yet made: origin, options, consequences, and which of the three clocks each one is on. |
| [references/open-questions.reference.md](references/open-questions.reference.md) | Active; 8 of 10 answered | Ten questions for RepForge and kafkaman: what produced each, what it changes, and what happens while it stays open. Q1-Q8 answered 2026-08-30; **Q9 and Q10 have never been asked**, which is a fact about this project rather than about kafkaman. |
| [references/repforge-section-c-answers.reference.md](references/repforge-section-c-answers.reference.md) | Recorded (not in `raw/`) | RepForge's 2026-08-29 answers to the register's section C: the accumulator seeds at zero, a `retry` terminality field now exists, and there is no production queue-depth data. |
| [references/repforge-questions-from-frontbox.reference.md](references/repforge-questions-from-frontbox.reference.md) | Sent 2026-08-29 | The eight questions sent to RepForge in reply to their section C answers. Lived untyped at the repository root until 2026-08-31, cited by nothing — including the page that checks the replies. |
| [references/repforge-eight-answers.reference.md](references/repforge-eight-answers.reference.md) | Recorded (not in `raw/`) | RepForge's 2026-08-30 reply, their claims about this repository checked one by one, and the two findings the check produced. |
| [references/repforge-queued-write-coalescing-request.reference.md](references/repforge-queued-write-coalescing-request.reference.md) | Recorded (not in `raw/`) | RepForge's 2026-09-05 request to collapse repeated offline writes to one row, plus the checked frontbox-side claims and the unsafe `attempts == 0` finding. |
| [decisions/001-single-threaded-core.decision.md](decisions/001-single-threaded-core.decision.md) | Accepted | Core uses single-threaded frontend-friendly traits with no `Send` bounds. |
| [decisions/002-error-model.decision.md](decisions/002-error-model.decision.md) | Accepted | Core uses one non-exhaustive structured error type instead of per-trait associated errors. |
| [decisions/003-atomic-outcome-application.decision.md](decisions/003-atomic-outcome-application.decision.md) | Accepted | Outbox outcome application is one atomic transition, not separate delete and dead-letter writes. |
| [decisions/004-transport-auth-and-offline.decision.md](decisions/004-transport-auth-and-offline.decision.md) | Accepted | Auth is evaluated freshly per send; offline is distinct from attempted transport failure. |
| [decisions/005-mutation-outcome-policy.decision.md](decisions/005-mutation-outcome-policy.decision.md) | Accepted | `Blocked` and `Pending` stay queued; only `Rejected` becomes a dead letter. |
| [decisions/006-corrupt-record-policy.decision.md](decisions/006-corrupt-record-policy.decision.md) | Accepted | Corrupt local records are quarantined or surfaced, not silently dropped from reads. |
| [decisions/007-generic-entity-key-registry.decision.md](decisions/007-generic-entity-key-registry.decision.md) | Accepted | Generic cache versioning needs caller-owned entity keys and a caller-supplied registry. |
| [decisions/008-mutation-envelope-extensibility.decision.md](decisions/008-mutation-envelope-extensibility.decision.md) | Accepted | Records are non-exhaustive and constructor-built, the body is parsed JSON, and optional structured operation metadata rides along uninterpreted. |
| [decisions/009-local-scope-identity.decision.md](decisions/009-local-scope-identity.decision.md) | Accepted | Every store takes a required opaque scope key; records are stamped, reads verify, and a mismatch retains rather than discards. |
| [decisions/010-batch-wire-format.decision.md](decisions/010-batch-wire-format.decision.md) | Accepted (amended) | The batch payload stays byte-compatible with the source server. Amended by decision 011, which kept the bytes and dropped the date dependency. |
| [decisions/011-owned-rfc3339-rendering.decision.md](decisions/011-owned-rfc3339-rendering.decision.md) | Accepted | `client_datetime` is rendered by this crate and compared against `chrono` by a dev-dependency oracle, so byte compatibility costs no public dependency. |
| [decisions/012-unknown-mutation-status.decision.md](decisions/012-unknown-mutation-status.decision.md) | Accepted | An unrecognised server status is retained with its original spelling and reported as an anomaly, never mapped onto a known status. |
| [decisions/013-unknown-entity-name.decision.md](decisions/013-unknown-entity-name.decision.md) | Accepted | An invalidation naming an unregistered entity is ignored for cache purposes but reported to the caller, in one event shape. |
| [decisions/014-pull-gating.decision.md](decisions/014-pull-gating.decision.md) | Accepted | Core reports that a refetch would discard unsent local work; it does not gate a pull it does not perform. |
| [decisions/015-cache-version-persistence.decision.md](decisions/015-cache-version-persistence.decision.md) | Accepted | Cache version and staleness are durable by default and persist as one unit — storing the version alone is a correctness bug. |
| [decisions/016-monotonic-enqueue-sequence.decision.md](decisions/016-monotonic-enqueue-sequence.decision.md) | **Implemented** 2026-08-29 | Enqueue assigns a durable, globally monotonic sequence and `pending_batch` orders by it. `(created_at, mutation_id)` was never causal, and batching never hid that. |
| [decisions/017-bounded-retention.decision.md](decisions/017-bounded-retention.decision.md) | **Implemented** 2026-08-29 | A record retained to a caller-set attempt bound is dead-lettered, never discarded and never skipped past. Without it, `batch_limit = 1` freezes the queue on one stuck record. |
| [decisions/018-single-flight-drain-mode.decision.md](decisions/018-single-flight-drain-mode.decision.md) | **Implemented** 2026-08-29 | `batch_limit = 1` gains a conformance profile and does not become the default. |
| [decisions/019-verdict-synthesis.decision.md](decisions/019-verdict-synthesis.decision.md) | **Documented** on `SyncTransport` 2026-08-29 | With no server producing a batch response, the transport synthesizes verdicts — and must not read terminality off an HTTP status class. |
| [decisions/020-observability-surface.decision.md](decisions/020-observability-surface.decision.md) | Accepted (amended 2026-08-29) | Core emits nothing: diagnostics are returned, correlation is `mutation_id`, and the no-clock and `!Send` gates are why in-library emission is not available to a convergence. |
| [decisions/021-cache-version-identity.decision.md](decisions/021-cache-version-identity.decision.md) | **Implemented** 2026-08-28 | A cache version is an opaque identity compared by equality, not an ordered counter. `VersionUpdate::NeedsReset` loses its producer and zero stops being a safe sentinel. |
| [decisions/022-durable-trace-context.decision.md](decisions/022-durable-trace-context.decision.md) | **Implemented** 2026-08-29 | The record carries W3C trace context, stamped at enqueue and caller-supplied — core has no randomness, and a span cannot model enqueue-to-send. |
| [decisions/023-read-model-boundary.decision.md](decisions/023-read-model-boundary.decision.md) | Accepted; **superseded in part** by 032 | Originally: frontbox stores row-level staleness markers, never the rows. Decision 032 reversed that one sentence — frontbox now stores the rows, as opaque blobs — because only something seeing both the rows and the queue can decide the hydration merge. The rest of 023 stands. |
| [decisions/024-scope-storage-encoding.decision.md](decisions/024-scope-storage-encoding.decision.md) | Accepted; case 49 landed 2026-08-29 | Core supplies no encoder and states one obligation — injectivity — proven by a conformance case that does not yet exist. |
| [decisions/025-quarantine-storage-shape.decision.md](decisions/025-quarantine-storage-shape.decision.md) | Accepted; case 50 landed 2026-08-29 | The backend picks table-or-state; core requires four properties, three already proven and one not. |
| [decisions/026-replayable-preconditions.decision.md](decisions/026-replayable-preconditions.decision.md) | **Implemented** 2026-08-30 | A write precondition is a fourth durable field, captured at enqueue. Refuses the general header map, because it would repeal decision 004. |
| [decisions/027-dead-letter-reason.decision.md](decisions/027-dead-letter-reason.decision.md) | **Implemented** 2026-08-30 | A dead letter carries a `DeadLetterReason`, not a nullable rejection. The old encoding answered two questions with one field. |
| [decisions/028-drain-loop-boundary.decision.md](decisions/028-drain-loop-boundary.decision.md) | **Implemented** 2026-08-29 | The drain loop is core's; only the cadence is the adapter's. An adapter cannot be an aggregation boundary, and the conformance suite cannot reach one. |
| [decisions/029-drain-termination.decision.md](decisions/029-drain-termination.decision.md) | **Implemented** 2026-08-29 | A drain stops when it stops draining, not when the queue empties. Decision 018 credited 017 with preventing a spin that 017 does not prevent. |
| [decisions/030-serializable-reports.decision.md](decisions/030-serializable-reports.decision.md) | **Implemented** 2026-08-29 | The seven report types `Serialize` and never `Deserialize`: nothing writes a report, so the reverse derive is semver surface with no caller. |
| [decisions/031-cross-realm-single-flight.decision.md](decisions/031-cross-realm-single-flight.decision.md) | **Accepted** 2026-08-30; in-realm half built | At most one drain per scope, not per `SyncRunner`. Core claims the scope and case 61 proves it; extending the claim across realms is an obligation on D5's backends, since two tabs share a database and nothing else. |
| [decisions/032-opaque-row-store.decision.md](decisions/032-opaque-row-store.decision.md) | **Accepted** 2026-08-30; builds in D5 | frontbox stores read-model rows as opaque blobs it never parses, mutations bind to rows at enqueue, and the hydration merge becomes a library rule. Supersedes 023's one load-bearing sentence; the rest of 023 stands. |
| [decisions/033-last-error-on-the-record.decision.md](decisions/033-last-error-on-the-record.decision.md) | **Implemented** 2026-08-30 | A retained record carries the server's own words for why, bounded at 512 bytes and never parsed. Decision 017 made a record give up at a bound; this says what it was up against. |
| [decisions/034-no-service-origin-in-core.decision.md](decisions/034-no-service-origin-in-core.decision.md) | **Accepted** 2026-08-30; no code | No origin column and no core concept. A service is a transport concern, and durable routing would be stale the day the topology changed. |
| [decisions/035-reports-name-what-drained.decision.md](decisions/035-reports-name-what-drained.decision.md) | **Implemented** 2026-08-30 | `drained: Vec<Drained>` names every record that left the queue, how, and the row it was bound to. The reports named a mutation only when something went wrong; now success has an event too. |
| [decisions/036-no-sub-scope-partitions.decision.md](decisions/036-no-sub-scope-partitions.decision.md) | **Accepted** 2026-08-30; closes the key | One queue per scope, primary key `(seq)`. A mutation has no type to partition on, and splitting would trade away the cross-type ordering decision 016 exists to protect. |
| [decisions/037-multi-service-routing.decision.md](decisions/037-multi-service-routing.decision.md) | **Accepted** 2026-08-31; D4d is planned to prove it | One outbox, one scope, routing inside `SyncTransport`. The join of 034 and 036, which had never been put together: 036 paid for cross-service ordering by refusing to partition, and nothing has ever tested it. |
| [decisions/038-invalidation-delivery-in-the-trial.decision.md](decisions/038-invalidation-delivery-in-the-trial.decision.md) | **Accepted** 2026-08-31 | The inbound seam is built in `todo-core` first and promoted against a written bar. Invalidation has had zero consumers ever, and a seam with one implementation is a wrapper. |
| [decisions/039-user-delete-cascades-server-side.decision.md](decisions/039-user-delete-cascades-server-side.decision.md) | Accepted | Deleting a user empties them of todos first, and the user service does it. The client enqueues one mutation; a cascade that cannot finish applies nothing. Records the dependency cycle this creates between the two example services. |
| [decisions/040-one-wake-releases-every-waiter.decision.md](decisions/040-one-wake-releases-every-waiter.decision.md) | **Accepted** 2026-09-01; amended 2026-09-02 | A `Wake` is a broadcast, not a handoff: every sleeper waiting on one is released by a single fire, and the latch becomes per waiter. The one-slot version released exactly one loop, measured at 5 of 6 fires, and which one was a race. The amendment keys the pending-waker table by waiter, so "replaced rather than appended" is what the code does. |
| [decisions/041-the-poll-stops-when-nobody-is-looking.decision.md](decisions/041-the-poll-stops-when-nobody-is-looking.decision.md) | **Accepted** 2026-09-01 | The invalidation poll pauses outright when the window is not in front and resumes on the wake; the drain loop does not, because those writes are already the user's. `focus` as well as `visibilitychange`, because two side-by-side windows are both visible. |
| [decisions/042-events-are-toasts-conditions-are-the-status-bar.decision.md](decisions/042-events-are-toasts-conditions-are-the-status-bar.decision.md) | **Accepted** 2026-09-01 | One rule by duration replaces three message surfaces with no rule between them. What happened is a toast; what is still true is on the status bar — including a service that has been unreachable for an hour, which a toast would let expire into silence. |
| [decisions/043-in-front-on-every-platform.decision.md](decisions/043-in-front-on-every-platform.decision.md) | **Accepted** 2026-09-01; re-verified and amended 2026-09-02 | The adapter gains a `native` feature and `use_lifecycle_wake`: one wry event handler serves desktop, iOS and Android. Verified on real OS transitions on both phones. Desktop is wired and inert — `dioxus-desktop` filters the window event out before a handler sees it, measured. The amendment records the web arm defaulting "cannot tell" the wrong way, against its own comment stated three times. |
| [decisions/044-transport-started-before-the-request.decision.md](decisions/044-transport-started-before-the-request.decision.md) | Accepted; amended twice after review | A queued body may be replaced only while a durable `transport_started` is false, and that fact is written before the request exists rather than inferred from how the request failed. Two writers set it — `read_for_send`, and `apply_outcomes` on a `Retain` — because a verdict cannot exist without a request. |
| [compatibility/cache-versions-are-optional.compat.md](compatibility/cache-versions-are-optional.compat.md) | Accepted | Cache versions are optional and always were; when a plain periodic re-read is the better answer, and the one thing skipping them costs — the pending-write conflict report that stops a refetch clobbering unsent edits. |
| [compatibility/dioxus-adapter.compat.md](compatibility/dioxus-adapter.compat.md) | Active | What `frontbox-dioxus` commits to, and the first `0.x` crate in any frontbox public surface. Core's dependency claim survives, and a gate now proves it. |
| [compatibility/indexeddb-adapter.compat.md](compatibility/indexeddb-adapter.compat.md) | Active | What the IndexedDB adapter can say about a stored object that does not match its row type, per store. The outbox quarantines it; the terminal stores drop it, which is a trait shape and register entry 25 rather than an oversight. |
| [compatibility/public-dependencies.compat.md](compatibility/public-dependencies.compat.md) | Active | Which crates appear in the public API and therefore in the semver contract — `serde_json`, `uuid`, `serde` — and which server payload the wire format targets. |

## Open Work

- D1 through D5 are implemented and **all 30 gates pass** (`scripts/verify.sh`): 68 conformance
  cases inside 128 tests on `-p frontbox --all-features`, the same 68 green on **three backends** —
  in-memory, SQLite, and IndexedDB in headless Chrome — plus the trial's observation and regression
  checks over real HTTP, the wasm and desktop builds, clippy everywhere, rustdoc, and coverage of
  89.17% regions / 95.79% lines against an 80% floor. Figures as of 2026-08-31, after the worktree
  audit; `scripts/verify.sh` prints the current ones. Two of the thirty gates are new that day and
  exist because prose rotted where nothing measured it: **cited paths resolve** and **no source file
  over 400 lines**.
- **A whole-worktree review on 2026-08-31 found two cross-backend divergences the conformance suite
  could not see**, and both are now cases. SQLite's `pending_batch` could return nothing while
  records were queued, because it filtered corrupt rows out of a fixed `limit * 4` window (case 69);
  and `DeadLetterStore::list` stated no order, so three backends chose three and a truncating read
  returned a different *set* per backend (case 68). Both fixes were confirmed to fail against the
  previous behaviour first. The same review found the entry-point documents describing a project two
  deliverables behind the code — which is why `cited paths resolve` and `no source file over 400
  lines` are now gates rather than habits. See `log.md`, 2026-08-31.
- **The application runs on four platforms** (D4c): web, macOS desktop, iOS and Android, from one
  component tree with nothing below `main.rs` conditionally compiled, and with no change to
  `frontbox` or `frontbox-dioxus`. One finding is open and not fixed — the drain loop stops being
  polled on Dioxus desktop; see register entry 20 and
  `plans/d4c-multi-platform-trial.plan.md`. Web, iOS and Android are unaffected.
- ~~**RepForge raised same-row queued-write coalescing on 2026-09-05.**~~ **Built the same day** on
  all three backends, adopted by the trial, and closed by decision 044. The product need was accepted
  as stated — two offline profile edits should leave one queued send carrying the latest body and the
  original precondition — and RepForge's safety rule was not: `attempts == 0` is not "never sent",
  because `attempts` counts verdicts received. What shipped is a store-level coalescing API gated on
  one durable `transport_started` fact **written before the request is built**, plus
  `SyncTransport::offline_now` so an offline application does not burn its own coalescibility.
  Deriving that fact from how a send *failed* was tried and dropped: `Error::Offline` covers a browser
  `fetch` that fails for lack of connectivity, indistinguishable from a request the server received
  and answered into a lost response.

  Two reviews followed. The first found a real bypass — `apply_outcomes` applying a `Retain` reaches
  the same state without ever calling `read_for_send` — closed by case 80 and a second writer of the
  mark. The second found no defect but a contract still describing the pre-fix rule, plus two runner
  paths the plan had asked for and nobody had written: cases 81 and 82. Conformance covers 70-82; the
  plan sat outside the D0-D6 sequence throughout.
- The public-dependency note now exists at `compatibility/public-dependencies.compat.md`, which
  discharges two of decision 010's three release obligations. Outstanding: a changelog entry the
  first time a public major moves, and naming a *checkable* server-contract version in place of
  "RepForge as of 2026-08-25" — a D4 question, since D4 is when a second consumer first exists.
- The public dependencies are `serde_json`, `uuid`, and `serde`, all major `1`. `chrono` was on
  that list for one day: writing the note exposed it as public surface *by behaviour* rather than
  by type, and decision 011 removed it by owning the rendering. No 0.x crate is in **core's**
  semver contract, which is what makes its remaining release obligations small — and
  `scripts/verify.sh` now proves it against `cargo tree -p frontbox` rather than against the
  manifest. `frontbox-dioxus` is where that stops being true; see
  `compatibility/dioxus-adapter.compat.md`.
- D0a prior-art survey is complete. Both shape decisions that a published D1 API could foreclose
  are settled: decision 008 (envelope extensibility and operation metadata) and decision 009
  (local scope identity). Decision 010 was added during D1.
- **D2 is implemented** (2026-08-27), amended by decision 021 on 2026-08-28.
- **D4a is implemented** (2026-08-29) and **no line of `src/` or `crates/` changed for it**,
  which is the roadmap's stated question answered as a fact about the core API boundary. It opened
  register entries 12 through 15 — the first questions raised by an application holding this API
  rather than by a conformance case that already knows the answer. The sharpest was Finding 1, **no
  report names the mutations that drained**, which left a UI's row-to-pending index with no event to
  decrement on. ~~Open~~ — **closed by decision 035** on 2026-08-30: `DrainReport::drained` names
  every record that left the queue, so the index decrements and the full rebuild is now startup
  reconciliation only. See `plans/d4a-offline-todo-trial.plan.md`.
- **D3 is implemented** (2026-08-29) and split in two while planning it. D3a is
  `SyncRunner::drain` in core; D3b is `crates/frontbox-dioxus`, which owns only the cadence. The
  split is decision 028: an adapter cannot be decision 020's aggregation boundary without leaving
  every native consumer to reinvent it, and the conformance suite cannot reach an adapter at all.
  The repository is now a workspace; `src/` deliberately did not move.
- ~~Decide the policy for a server status this crate has never heard of~~ — settled by decision 012
  and shipped: `MutationStatus::Unknown(String)` retains the record and reports an
  `AnomalyKind::UnknownStatus` carrying the server's spelling. `SyncReport::anomalies` is now
  `Vec<Anomaly>` rather than `Vec<MutationId>`, because three reasons in one bare list of
  identifiers told a caller nothing.
- ~~Decide whether cache version state is persisted by default~~ — settled by decision 015: durable,
  with version and staleness as one unit. Persisting the version alone would let a client believe an
  invalidated entity is fresh.
- **RepForge is removing its BFF**, which expired the premise several D1 decisions were argued
  against. Their proposal is recorded at `references/repforge-single-flight-proposal.reference.md`
  and answered at `proposals/single-flight-drain.proposal.md`. Decisions 016-019 came out of it and
  **all are built** as of 2026-08-29, in the order 017, 016, 022, then 018's profile and 019's
  transport documentation. The profile landed as its own suite macro rather than the outbox list
  re-run: four cases lose their subject entirely at `batch_limit = 1`, and the macro's own
  documentation names which and why. See decision 018's `## Implementation Outcome`.
- ~~Decide whether mutation ordering needs a monotonic sequence number before durable backends~~ —
  settled by decision 016: yes, globally monotonic and assigned by the store at enqueue. The
  deciding evidence turned out to be in this crate rather than in prior art. `runner.rs:263-264`
  builds the batch from `pending_batch` order, so a mis-sorted queue is a mis-ordered batch and the
  hazard exists today at `batch_limit = 100`.
- ~~Decide whether retained work needs aging, attempt tracking, or last-error metadata~~ — partly
  settled by decision 017: attempt tracking is adopted, aging is rejected (a record that aged while
  the user was offline was never evaluated), and dead-lettering at the bound was already promised by
  decision 005's prior-art section. ~~Last-error metadata stays open~~ — **settled by decision 033**
  on 2026-08-30 and built: a retained record carries the server's own words in `last_error`, bounded
  at 512 bytes by `LAST_ERROR_MAX` and never parsed. 017 made a record give up at a bound; 033 says
  what it was up against.
- Decide whether retention bounds should be per-status. `Pending` waits on a server job that will
  probably settle; `Unknown` waits on a client rebuild that will not. One bound either buries
  `Pending` work early or leaves `Unknown` work queued long (decision 017, Revisit If).
  **D4a was expected to supply the evidence and did not**: its server answers `Pending` only
  under an explicit hold, so the trial watched the bound work without learning anything about a
  real `Pending` distribution. Still waiting on a workload rather than on an argument.
- Decide whether D5's ordering should be a total order or a partial one. Decision 016 takes the
  total order because it is the version that can be reasoned about, but with four per-service
  origins it serializes a `billing` write behind an unrelated `workout` write. Per-origin sequences
  or explicit dependency edges are the alternatives, and both are larger designs.
- ~~D3 owes a drain-until-idle loop~~ — built 2026-08-29 as `SyncRunner::drain`, and conformance
  case 60 measures the gap it closes. Building it found decision 018's liveness claim wrong:
  017's bound counts verdicts *received*, so a loop that stops only at `Idle` spends a bound of
  eight inside a second rather than giving the server eight chances. A drain therefore stops on
  the first pass that makes no progress (decision 029).
- Ask RepForge for a structured terminality signal in service error bodies. It is the one thing the
  redesign could add that would collapse decision 019 into a mapping table, and it is cheap while
  the services are still being designed.
- **RepForge revised the proposal on 2026-08-28** and withdrew "not asking D2 to change". Recorded
  at `references/repforge-single-flight-proposal.reference.md` (`## The 2026-08-28 Revision`) and
  answered at `proposals/repforge-read-model-convergence.proposal.md`. Decisions 021-023 came out of
  it. **Decision 021 changes shipped D2 code** — `EntityState`, `InvalidationEvent`, `StaleEntity`,
  `VersionUpdate`, and `compare` all assume an ordered `u64` — which no decision since D1 has done,
  and it is affordable only because `publish = false`.
- ~~Decide what becomes of **`VersionUpdate::NeedsReset`**~~ — removed when decision 021 was
  implemented. The 005 precedent did not transfer: `Blocked` is kept because a *server* can still
  send it, whereas `VersionUpdate` is produced only by `compare`, so keeping the variant would have
  meant an unreachable arm and a permanently empty `InvalidationReport::needs_reset`. The enum is
  `#[non_exhaustive]`, so restoring it is additive.
- ~~Decide whether local read-model persistence belongs in core or app-owned companion traits~~ —
  settled by decision 023: app-owned, with core holding `(entity, row_id, stale)` markers and never
  the rows. Row markers make D5's third durable schema.
- **Observability convergence across RepForge, frontbox, and kafkaman.** The §5b.6 boundary is
  accepted (decision 020, amended) — emit `tracing`, application owns the sink. The aggregation
  boundary it asked for exists as of 2026-08-29: `DrainReport`, in core rather than the adapter
  020 predicted, and `Serialize` on every report type (decision 030) so it can leave the process
  without hand-mapping. Open: whether the
  `tracing` dependency is default-on or feature-gated, and its wasm binary-size cost, which nobody
  has measured. Earlier framing was: frontbox's side is
  documented at `decisions/020-observability-surface.decision.md`; the convergence *specification*
  was not supplied, so four questions there are open — the shared event vocabulary, whether the
  target is OpenTelemetry and what `mutation_id` maps to in it, whether kafkaman pushes or returns,
  and whose clock stamps the shared format. Two frontbox-side items fell out and **both have shipped**:
  ~~derive `Serialize` on the report types~~ (decision 030, 2026-08-29 — and it settled the reverse
  direction too: nothing writes a report, so `Deserialize` would be semver surface with no caller),
  and ~~settle last-error metadata~~ (decision 033, 2026-08-30 — which decision 020 had promoted
  from optional to load-bearing, because it is the only proposed field that outlives the process).
- ~~Decide how durable backends encode a `ScopeKey` into a storage name~~ — settled by decision 024:
  core supplies no encoder and states one obligation, injectivity, enforced by case 49.
  Hash-with-readable-prefix was recommended and not required; **both backends did something simpler
  and better** — a `scope` column with every read filtered on it, which makes injectivity trivial
  and keeps the suite's sharing contract, since two scopes have to live in one physical store or the
  isolation cases prove nothing. Reversibility turned out unnecessary because decision 009 already
  stamps `scope` on every record.
- ~~Decide whether quarantine is a distinct store or a status in a single durable table~~ — settled
  by decision 025: the backend picks, core requires four properties, and case 50 closed the fourth
  (atomicity of the quarantine leg). **Both backends chose a separate table**, which made that leg a
  cross-store write — on IndexedDB, two object stores named in one transaction declared upfront,
  which is the fiddliest place in that backend to get atomicity subtly wrong. Case 50 is what says
  they did not.
- ~~**Two conformance cases are owed and neither can fail today.**~~ **Landed 2026-08-29** as cases
  49 and 50 — colliding scope keys, and a quarantine transition that rolls back with its batch.
  Both pass on the in-memory backend and exist for D5's; decision 009's injectivity obligation is no
  longer unproven prose.
- D5 must store `mutation_id` in one consistent textual form — exactly what `MutationId::to_string`
  returns — with a binary collation. `MutationId: Ord` compares the UUID's bytes; any single
  canonical form sorts identically, but *mixed* forms (some rows uppercase, some unhyphenated)
  silently invert pairs and give that backend a different pending order. Found during D1; see the D1
  plan's Ordering Policy and the unit tests in `src/id.rs`.
- Decide whether a cross-scope diagnostic is needed to surface work retained under a scope no store
  currently opens. D1's no-progress signal cannot see it (decision 009).
- Create `wiki/apis/` entries once public APIs exist. `wiki/compatibility/` now has four.

## Crate

The library lives at the repository root as a single crate, `frontbox`, with modules matching the
eventual crate split. `scripts/verify.sh` runs every gate the roadmap names, including the prose
ones — no Dioxus, no RepForge entity names, no `Send` bound — as `grep` checks rather than
intentions.

The conformance suite is a library module behind a `testing` feature, not a test file, so D5's
SQLite and IndexedDB backends run the identical cases through `StoreFactory` and `FaultInjection`.
It emits in two shapes — synchronous cases driven by a caller-supplied `block_on`, and `async` cases
for a harness that drives them itself, which is what a browser needs — from one shared case list, so
the native and wasm backends cannot drift into running different suites.

The runtime dependency graph is `serde`, `serde_json`, `thiserror`, and `uuid`. There is no date
library in it: `src/rfc3339.rs` renders the wire format's `client_datetime`, and `chrono` is a
dev-dependency whose only job is to prove those bytes are still its own (decision 011).

Source files are kept under ~400 lines, and `scripts/verify.sh` gates total coverage at 80%.

## Source Evidence

Initial source material was copied by `llm-wiki init` into
`raw/initial/2026-08-25T083750Z`.

Prior-art source notes for D0a live under `raw/research/2026-08-25-prior-art-survey`. That
directory's `manifest.md` records per-URL retrieval state, a verification pass, corrections to four
claims, and the systems deliberately not surveyed.

## Maintenance

- Add durable architecture findings as typed pages under `wiki/specs`, `wiki/proposals`,
  `wiki/roadmaps`, `wiki/plans`, `wiki/decisions`, or `wiki/references`.
- Keep this index and `wiki/log.md` updated when wiki knowledge changes.
- Do not rely on Git operations for wiki bookkeeping; Git commits remain user-owned.

# Prior-Art Survey

Document Class: Reference
Status: Sourced
Date: 2026-08-25
Verified: 2026-08-26
Category: Research
Scope: Comparison of established offline-first, local-first, and HTTP command-queue systems against frontbox's planned cache and mutation outbox design before D1 implementation.
Sources: `raw/research/2026-08-25-prior-art-survey/manifest.md`, `raw/research/2026-08-25-prior-art-survey/research-summary.md`, `raw/research/2026-08-25-prior-art-survey/sources/01-replicache-zero.md`, `raw/research/2026-08-25-prior-art-survey/sources/02-powersync-electric.md`, `raw/research/2026-08-25-prior-art-survey/sources/03-rxdb-watermelon.md`, `raw/research/2026-08-25-prior-art-survey/sources/04-pouchdb-crdt.md`, `raw/research/2026-08-25-prior-art-survey/sources/05-http-command-queues.md`
Related: `wiki/plans/prior-art-survey.plan.md`, `wiki/proposals/extraction-boundary.proposal.md`, `wiki/roadmaps/extraction.roadmap.md`, `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/decisions/003-atomic-outcome-application.decision.md`, `wiki/decisions/004-transport-auth-and-offline.decision.md`, `wiki/decisions/005-mutation-outcome-policy.decision.md`, `wiki/decisions/006-corrupt-record-policy.decision.md`

## Purpose

D0a checks the RepForge-derived frontbox design against mature systems before any crate or public
API exists. The survey was run by system family and synthesized here.

## Revision Note

The 2026-08-25 draft surveyed nine systems, eight of which replicate *state*. frontbox replays
*HTTP commands*, so that cohort omitted frontbox's nearest peers and produced a headline finding
that did not survive verification. A 2026-08-26 pass re-fetched every URL, corrected four claims,
replaced two dead citations, and added the HTTP command-queue cohort. Corrections are recorded in
`raw/research/2026-08-25-prior-art-survey/manifest.md` `## Corrections` and `## Verification Pass`.
Claims below are as verified on 2026-08-26.

## Primary Sources

All URLs verified 2026-08-26 unless marked otherwise.

### State-Replication And CRDT Cohort

| System | Primary URLs | Note |
| --- | --- | --- |
| Replicache | `https://doc.replicache.dev/concepts/how-it-works`, `https://doc.replicache.dev/reference/server-push` | Repository archived, last push 2022-05-07. |
| Zero | `https://zero.rocicorp.dev/docs/mutators`, `https://zero.rocicorp.dev/docs/queries`, `https://zero.rocicorp.dev/docs/auth` | |
| PowerSync | `https://docs.powersync.com/client-sdks/writing-data`, `https://docs.powersync.com/handling-writes/writing-client-changes`, `https://docs.powersync.com/handling-writes/handling-write-validation-errors`, `https://docs.powersync.com/sync/overview`, `https://docs.powersync.com/architecture/client-architecture`, `https://docs.powersync.com/configuration/app-backend/client-side-integration` | |
| Electric | `https://github.com/electric-sql/electric`, `https://electric.ax/docs/guides/writes` | Domain moved from `electric-sql.com`. |
| ElectricSQL (legacy) | `http://web.archive.org/web/20250118030309/https://legacy.electric-sql.com/docs/reference/architecture`, `http://web.archive.org/web/20250530113034/https://legacy.electric-sql.com/docs/usage/data-access/shapes` | Live host dead; archived snapshots. |
| RxDB | `https://rxdb.info/replication.html`, `https://rxdb.info/transactions-conflicts-revisions.html`, `https://rxdb.info/migration-schema.html` | |
| WatermelonDB | `https://watermelondb.dev/docs/Implementation/SyncImpl`, `https://watermelondb.dev/docs/Sync/Frontend`, `https://watermelondb.dev/docs/Sync/Backend` | |
| PouchDB/CouchDB | `https://docs.couchdb.org/en/stable/replication/protocol.html`, `https://docs.couchdb.org/en/stable/replication/conflicts.html`, `https://docs.couchdb.org/en/stable/api/database/security.html`, `https://pouchdb.com/api.html` | |
| Automerge | `https://github.com/automerge/automerge`, `https://automerge.org/docs/reference/documents/conflicts/` | |
| Yjs | `https://docs.yjs.dev/`, `https://docs.yjs.dev/getting-started/working-with-shared-types`, `https://docs.yjs.dev/api/document-updates` | |

### HTTP Command-Queue Cohort

| System | Primary URLs |
| --- | --- |
| Workbox Background Sync | `https://developer.chrome.com/docs/workbox/modules/workbox-background-sync` |
| Redux Offline | `https://github.com/redux-offline/redux-offline/blob/master/docs/api/config.md`, `https://github.com/redux-offline/redux-offline/blob/master/docs/recipes/customize-requests.md` |
| TanStack Query | `https://tanstack.com/query/latest/docs/framework/react/guides/mutations` |
| Amplify DataStore | `https://docs.amplify.aws/gen1/react/build-a-backend/more-features/datastore/datastore-events/`, `https://docs.amplify.aws/gen1/javascript/build-a-backend/more-features/datastore/conflict-resolution/` |

## Headline Findings

The survey does not invalidate the frontbox extraction boundary. It sharpens the pre-D1 risk
profile and corrects the draft's assessment of how unusual frontbox is.

**frontbox belongs to the HTTP command-queue cohort, where the envelope is standard.** Systems that
replicate state use named mutators, row CRUD, document revisions, or CRDT updates, because the
operation body exists in code on both ends. Systems that persist a client-side queue cannot do
that. TanStack Query states the constraint: "only the state of mutations is persisted, as functions
cannot be serialized." Workbox therefore stores a serialized `Request`; Redux Offline stores
`effect: {url, method, json, headers}`. frontbox's `method`/`path`/`body` envelope is the
mainstream answer to that constraint, not a RepForge convenience.

**Terminal-versus-transient classification is near-universal, and frontbox's version is the most
recoverable.** Replicache: "If a permanent error is encountered such that the mutation will never be
appliable, ignore that mutation and increment the `lastMutationID`... If a temporary error is
encountered that might be resolved on retry, halt processing mutations and return." Redux Offline
resolves every request to "success and commit, temporary failure and retry, and permanent failure
and rollback," discarding on 4xx by default. Zero skips throwing mutators and reverts. Amplify
returns a `DISCARD` sentinel. All four *discard or roll back* the terminal case; frontbox
dead-letters it. This directly supports decision 005.

**Workbox is the counterexample that validates decision 005.** It has no terminal/transient split:
a failed request "is put back in the same position in the queue," indefinitely, bounded only by
`maxRetentionTime`. That is the head-of-line wedge decision 005 avoids.

**Per-client causal ordering has exactly one precedent.** Replicache states a mutation ID
"describe[s] a causal order to mutations from this client, and that order is respected by the
server." No other surveyed system documents this. CouchDB's `update_seq` is "The current database
Sequence ID" and its checkpoint is an "Intermediate Recorded Sequence ID used for Replication
recovery" — database-scoped resumption, not per-client causality. RxDB checkpoints are resume
tokens. PowerSync documents "a blocking FIFO queue" but publishes no per-client causal operation
ID. The draft cited all four; only Replicache holds, with TanStack Query's `scope.id` serial
execution as weaker corroboration.

**Bounded retention is solved elsewhere and should be borrowed, not invented.** Redux Offline's
`retry()` returns a delay or "`null` if the action should be discarded" over a 1s-to-1h schedule,
"after this point, it will be discarded." Workbox uses `maxRetentionTime`. Amplify emits
`outboxStatus{isEmpty}`. D1's deferred attempt/aging question has shipped reference designs.

**Stalled-queue visibility is real, from three systems — not four.** CouchDB/PouchDB expose
scheduler and replication state, RxDB exposes active/error/conflict/in-sync observables, and
Amplify emits `outboxStatus` plus enqueued/processed/failed events. PowerSync's repeated-front-entry
warning is *not* in this group: its docs frame it as a developer-error diagnostic for a connector
that fails to call `.complete()`. The draft miscounted it as observability.

**Local cache/store scope is a safety property, not a naming detail.** Replicache warns that "each
user of your application uses a different Replicache `name`." Zero uses authenticated context plus
storage identity. PowerSync scopes streams with authenticated parameters and warns against trusting
client-supplied ones. CouchDB security is database-level, so filters and selectors are selection
tools, not tenant isolation. frontbox should treat principal, tenant, schema/version, and query
scope as part of local storage identity or cache validity.

**Corrupt-record quarantine is borrowed, not invented.** No surveyed local-first system exposes a
first-class per-record quarantine, which supports keeping decision 006. But poison-message
quarantine is long-established in message brokers. The defensible framing is "unusual in
local-first, standard in queueing" — not novelty.

**Electric rates its own rejected-write handling as inadequate.** Its through-the-database pattern
keeps "a log of local writes in a `changes` table," and its documented rollback is "very naive...
clearing all local state and writes in the event of any write being rejected by the server," with a
suggestion to clear "only the set of writes that are causally dependent on the rejected operation."
That phrasing is independent support for decision 005's refusal to dead-letter `Blocked` records.

## Comparison Table - State-Replication And CRDT Cohort

| System | Write Model | Conflict/Outcome Model | Invalidation/Scope | Batching/Retry/Stall | Storage | Framework Integration |
| --- | --- | --- | --- | --- | --- | --- |
| Replicache | Named mutator invocation with JSON args and sequential per-client mutation id. | Server-authoritative; pending mutations rebased over pulled canonical state; permanent failures must be marked processed by advancing `lastMutationID`. | Persistent client view keyed by `name`; backend controls auth and pull strategy. | Ordered mutation batches; temporary errors halt processing and retry. | Browser persistent key-value store, usually IndexedDB. | JS library with subscriptions and custom push/pull endpoints. Repository archived 2022. |
| Zero | Named mutators run locally, then on server against Postgres. | Throwing server mutators are skipped and reported structurally; optimistic effects reverted; later mutations proceed. | Server-defined query/mutator registries; authenticated context and user/storage identity. | Retries some server failures, then requires reconnect. | Client store plus `zero-cache` SQLite/Postgres replication path. | TypeScript with React, Solid, and React Native support. |
| PowerSync | Local SQLite writes captured as `PUT`, `PATCH`, `DELETE` entries in `ps_crud`. | Server-authoritative; transient failures keep entries queued; validation/conflict should return 2xx and propagate details separately; optional server-side dead letters with documented out-of-order risk. | Sync Streams/Rules scope data by authenticated parameters; client parameters not for authorization alone. | "Blocking FIFO queue"; upload loop with 5s retry delay; indefinite retry on error. Front-entry warning is a `.complete()` misuse diagnostic, not observability. | SQLite client database with managed sync tables. | Multiple SDKs, including an alpha Rust SDK. |
| Electric/ElectricSQL | Engine is read-path Postgres Shapes; four documented client write patterns, one keeping "a log of local writes in a `changes` table". Legacy ElectricSQL used oplog/Satellite replication. | No engine-level outcome model; documented rollback is self-described as "very naive", clearing all local state on any rejection. | Shapes define partial replication; invalid offsets handled by refetch. | Shape clients resume by offset/handle; write retry left to the application. | Sync to arbitrary consumers; legacy SQLite/PGlite/wa-sqlite. | HTTP/client/React shape integrations. |
| RxDB | Document-state replication with push rows carrying assumed master state plus new fork state. | Client-side conflict handler; default "will always drop the fork state and use the master state"; revision mismatch yields `409 CONFLICT`. | Replication identifiers and optional scoped replications; backend enforces auth/scope. | Batch sizes, retry timer, active/error/conflict/in-sync observables, `reSync()`. Checkpoints are resume tokens, not causal order. | Pluggable: IndexedDB, OPFS, SQLite, memory, workers. Migration strategies required on schema change. | Framework-neutral JS database. |
| WatermelonDB | Change tracking with `_status` and `_changed`; two-phase pull then push. | Server is source of truth: "server version is taken except for any column that was changed locally since last sync"; push aborts transactionally on conflict. | First sync and replacement sync over accessible dataset; backend enforces access. | App schedules sync; retry-once recommended; unsafe per-collection batching breaks transactionality. | SQLite/native and LokiJS web adapters. | React Native and web-oriented adapters. |
| PouchDB/CouchDB | JSON document revisions replicated with changes feed, `_revs_diff`, and `_bulk_docs`. | `409` on stale same-node writes; replicated conflicts create revision trees with a deterministic winner; losing revisions remain accessible. | Database-level security; filters/selectors are selection tools, not tenant isolation. | Batch sizes, live retry/backoff, replication events, scheduler states, error counts. Sequence IDs are database-scoped. | PouchDB IndexedDB/LevelDB/HTTP adapters; CouchDB server databases. | JS library plus CouchDB HTTP protocol. |
| Automerge | CRDT document changes/commits and sync protocol. | Concurrent changes merge; same-property conflicts expose a deterministic winner plus conflict values; no server rejection. | Document URLs and sync provider define reachability; auth is app/provider concern. | Retry/stall policy is provider-specific, not documented in the core. | Pluggable storage; snapshots and incremental chunks. | JS, Rust core, React/repo ecosystem. |
| Yjs | CRDT shared types inside `Y.Doc`; transactions emit binary updates. | Updates are commutative, associative, idempotent; no core rejection/dead-letter model. | Provider naming and server auth scope documents/rooms. | Transactions and update compaction; retry/stall policy is provider-specific. | IndexedDB persistence provider and many provider backends. | Broad editor/provider ecosystem. |

## Comparison Table - HTTP Command-Queue Cohort

This is frontbox's actual peer group.

| System | Durable Unit | Terminal vs Transient | Bounded Retention | Queue Observability | Ordering |
| --- | --- | --- | --- | --- | --- |
| Workbox Background Sync | Serialized `Request` (url, method, headers, body) in IndexedDB. | None. Any failure "is put back in the same position in the queue". | `maxRetentionTime` only. | None. | Strict FIFO. |
| Redux Offline | `effect: {url, method, json, headers}`. | `discard(error, action, retries)`; default "discards only on client errors" (4xx). | `retry()` returns delay or `null` to discard, over a 1s-to-1h schedule. | Redux state. | Pluggable FIFO (`enqueue`/`dequeue`/`peek`). |
| TanStack Query | Persisted mutation state plus `mutationKey`; functions cannot be serialized. | Paused (offline) versus failed. | Retry count. | Mutation cache. | Serial within a `scope.id`. |
| Amplify DataStore | Named "outbox". | Custom conflict handler returns a `DISCARD` sentinel. | Not documented. | `outboxStatus{isEmpty}`, `outboxMutationEnqueued`/`Processed`/`Failed`. | Not documented. |
| **frontbox (planned D1)** | `{method, path, body}` envelope. | `Rejected` -> dead letter; `Blocked`/`Pending` -> retain. | **Open. D1 defers.** | No-progress signal on a sync with no `Delete` and no `DeadLetter`. | `(created_at, mutation_id)`, deterministic but not causal. |

## Systems Not Surveyed

Recorded so the cohort boundary is explicit rather than implied.

- **2024-2026 local-first cohort**: Triplit (ships an explicit offline outbox), LiveStore
  (deterministic mutation log), TanStack DB, InstantDB, Jazz, Evolu, Ditto, Loro. Directly relevant
  and the most valuable extension if this survey is revisited.
- **Message brokers**: SQS redrive, Azure Service Bus dead-letter, Kafka. Excluded as server-side,
  but they are the origin of both the dead-letter pattern (decision 005) and poison-message
  quarantine (decision 006).
- **Mobile job schedulers**: Android WorkManager, `NSURLSession` background transfer. Relevant only
  to retry/backoff scheduling, which D1 does not implement.

## Implications For frontbox

### Keep

- Keep the HTTP envelope as the D1 extraction baseline. It is standard for frontbox's cohort and is
  the mainstream answer to the non-serializability of operation bodies.
- Keep fresh auth per send (decision 004); Redux Offline's async `discard` refreshing a token on
  401 is the closest prior art, and no surveyed system persists credentials inside queued records.
- Keep `Rejected` as the only terminal server outcome that becomes a dead letter (decision 005).
  Four cohort peers reach the same decision point and all discard or roll back; dead-lettering is
  strictly more recoverable.
- Keep `Blocked` and `Pending` retained, with D1's no-progress signal. Electric's "causally
  dependent" phrasing independently supports not dead-lettering `Blocked`.
- Keep atomic outcome application (decision 003); WatermelonDB's rule against marking a record
  synced when it "changed locally since fetch local changes step" is the same hazard.
- Keep corrupt-record quarantine (decision 006), citing message-broker poison-message handling as
  its lineage rather than claiming novelty.

### Add Or Revisit Before Public API Freeze

- Consider optional caller-owned operation metadata such as operation name, schema/version, or
  diagnostic label. Named operations are dominant in the replication cohort and TanStack Query
  shows a serializable key is required to resume persisted work. This supplements the envelope; it
  does not replace `method`/`path`/`body`.
- Make local namespace/scope explicit in cache and storage APIs. Principal, tenant, schema/version,
  and query/read scope should not be left to informal caller discipline.
- Decide attempt/error metadata or aging for retained work. Model on Redux Offline's
  `retry() -> null` and Workbox's `maxRetentionTime`; frontbox should dead-letter rather than
  discard at the bound.
- Revisit monotonic enqueue sequence before durable backends, recording that the supporting
  evidence is Replicache alone.
- Treat durable storage format versioning as public compatibility surface as soon as SQLite or
  IndexedDB backends exist. RxDB's migration of replication metadata is the reference case.

## D0a Outcome

D0a is complete. Every system listed in `wiki/plans/prior-art-survey.plan.md` has at least one
primary source; the HTTP command-queue cohort that plan omitted has been added; the comparison
tables cover write model, conflict model, invalidation/scope, batching/retry/stall, storage, and
framework integration; systems deliberately not surveyed are listed above; and
`wiki/proposals/extraction-boundary.proposal.md` records the required frontbox follow-up.

The survey's own limitation, that its original cohort excluded frontbox's nearest peers, is
recorded here rather than left for a later reader to discover.

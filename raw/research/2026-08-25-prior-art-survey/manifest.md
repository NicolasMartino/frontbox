# Prior-Art Survey Manifest

Research Question: How do established offline-first and local-first systems inform frontbox's public API before D1 implementation?
Goal: Complete roadmap deliverable D0a by comparing frontbox against mature systems before any crate or public API is written.
Date: 2026-08-25
Verified: 2026-08-26. Every URL below was re-fetched and every claim carried into the wiki was re-checked against it. See `## Verification Pass`.
Source Modes: Primary vendor documentation, official repositories, and official project-owned references retrieved over the public web.
Selection Rationale: Every cited source is official project documentation, an official repository, or an official project-owned reference. Secondary commentary was excluded from the synthesis.

Method Note: The first draft was produced by parallel read-only agents. Those agent reports are
workflow artifacts, not evidence. They are not retained and are not citable. Every claim that
reaches `research-summary.md` or `wiki/references/prior-art-survey.reference.md` traces to a
primary URL in `## Primary Source Set` below, with its retrieval state recorded.

## Inventory

| File | Systems | Cohort |
| --- | --- | --- |
| `sources/01-replicache-zero.md` | Replicache, Zero | State replication |
| `sources/02-powersync-electric.md` | PowerSync, Electric/ElectricSQL | State replication |
| `sources/03-rxdb-watermelon.md` | RxDB, WatermelonDB | State replication |
| `sources/04-pouchdb-crdt.md` | PouchDB/CouchDB, Automerge, Yjs | State replication, CRDT |
| `sources/05-http-command-queues.md` | Workbox Background Sync, Redux Offline, TanStack Query, Amplify DataStore | HTTP command outbox |

`sources/05-http-command-queues.md` was added on 2026-08-26. The original four notes surveyed only
systems that replicate *state*. frontbox replays *HTTP commands*, so the original set omitted its
own nearest peers. See `## Cohort Correction`.

## Primary Source Set

Retrieval state as of 2026-08-26.

| System | URL | State |
| --- | --- | --- |
| Replicache | `https://doc.replicache.dev/concepts/how-it-works` | OK |
| Replicache | `https://doc.replicache.dev/reference/server-push` | OK. Added 2026-08-26; carries the permanent-vs-temporary error rule. |
| Replicache | `https://github.com/rocicorp/replicache` | OK. **Archived**, last push 2022-05-07. |
| Zero | `https://zero.rocicorp.dev/docs/mutators`, `https://zero.rocicorp.dev/docs/queries`, `https://zero.rocicorp.dev/docs/auth` | OK |
| PowerSync | `https://docs.powersync.com/client-sdks/writing-data` | OK |
| PowerSync | `https://docs.powersync.com/handling-writes/writing-client-changes` | OK |
| PowerSync | `https://docs.powersync.com/handling-writes/handling-write-validation-errors` | OK |
| PowerSync | `https://docs.powersync.com/sync/overview` | OK |
| PowerSync | `https://docs.powersync.com/architecture/client-architecture` | OK. Added 2026-08-26; `ps_crud` and "blocking FIFO queue". |
| PowerSync | `https://docs.powersync.com/configuration/app-backend/client-side-integration` | OK. Added 2026-08-26; upload loop, retry delay, front-entry warning. |
| Electric | `https://github.com/electric-sql/electric` | OK. Homepage now `https://electric.ax`. |
| Electric | `https://electric.ax/docs/guides/writes` | OK. Added 2026-08-26; four documented client write patterns. |
| ElectricSQL (legacy) | `http://web.archive.org/web/20250118030309/https://legacy.electric-sql.com/docs/reference/architecture` | Archived snapshot. Live host is dead. |
| ElectricSQL (legacy) | `http://web.archive.org/web/20250530113034/https://legacy.electric-sql.com/docs/usage/data-access/shapes` | Archived snapshot. Live host is dead. |
| RxDB | `https://rxdb.info/replication.html`, `https://rxdb.info/transactions-conflicts-revisions.html`, `https://rxdb.info/migration-schema.html` | OK |
| WatermelonDB | `https://watermelondb.dev/docs/Implementation/SyncImpl`, `https://watermelondb.dev/docs/Sync/Frontend`, `https://watermelondb.dev/docs/Sync/Backend` | OK |
| CouchDB | `https://docs.couchdb.org/en/stable/replication/protocol.html`, `https://docs.couchdb.org/en/stable/replication/conflicts.html`, `https://docs.couchdb.org/en/stable/api/database/security.html` | OK |
| PouchDB | `https://pouchdb.com/api.html` | OK. Canonical host; `pouchdb.apache.org` also resolves. |
| Automerge | `https://github.com/automerge/automerge`, `https://automerge.org/docs/reference/documents/conflicts/` | OK |
| Yjs | `https://docs.yjs.dev/`, `https://docs.yjs.dev/getting-started/working-with-shared-types`, `https://docs.yjs.dev/api/document-updates` | OK |
| Workbox | `https://developer.chrome.com/docs/workbox/modules/workbox-background-sync` | OK. Added 2026-08-26. |
| Redux Offline | `https://github.com/redux-offline/redux-offline/blob/master/docs/api/config.md` | OK. Added 2026-08-26. |
| Redux Offline | `https://github.com/redux-offline/redux-offline/blob/master/docs/recipes/customize-requests.md` | OK. Added 2026-08-26. |
| TanStack Query | `https://tanstack.com/query/latest/docs/framework/react/guides/mutations` | OK. Added 2026-08-26. |
| Amplify DataStore | `https://docs.amplify.aws/gen1/react/build-a-backend/more-features/datastore/datastore-events/` | OK. Added 2026-08-26. |

## Verification Pass

On 2026-08-26 all originally cited URLs were re-fetched and each claim re-checked. Results:

- Two cited sources did not exist. `legacy.electric-sql.com` fails TLS with a certificate name
  mismatch over HTTPS and returns 404 over HTTP. Both legacy ElectricSQL citations are now
  archived Wayback snapshots.
- One claim was attributed to a page that does not contain it. The Replicache permanent-failure
  rule is on `reference/server-push`, not `concepts/how-it-works`. Citation added.
- One claim was mischaracterized. PowerSync's repeated-front-entry warning is a developer-error
  diagnostic about a connector that fails to call `.complete()`, not a queue observability feature.
- One claim was unsupported. Neither CouchDB nor PowerSync documents per-client causal operation
  ordering. See `## Corrections`.
- One system's status was unrecorded. `rocicorp/replicache` is archived.
- One system was under-described. Electric documents four client write patterns, including a
  persistent local `changes` table. The original note said Electric had no client write story.

## Corrections

Recorded so the wiki reference and the raw notes do not drift back to the original wording.

1. **Monotonic ordering evidence was overstated.** Only Replicache documents per-client causal
   operation order: a mutation ID "describe[s] a causal order to mutations from this client, and
   that order is respected by the server" (`concepts/how-it-works`). CouchDB is *not* supporting
   evidence: its Sequence ID is "The current database Sequence ID" and a checkpoint is an
   "Intermediate Recorded Sequence ID used for Replication recovery"
   (`replication/protocol.html`) — database-scoped resumption, not per-client causality. RxDB
   checkpoints are likewise replication resume tokens. PowerSync documents "a blocking FIFO queue"
   (`architecture/client-architecture`) but publishes no per-client causal operation ID; the
   original claim that it "carries per-client operation ids" was an inference from schema, not a
   documented contract.

2. **"Mature systems do not replay raw HTTP envelopes" was a cohort artifact.** It is true of
   state-replication engines and false of HTTP command queues. See `## Cohort Correction`.

3. **The quarantine novelty claim was scoped to a set chosen not to contain it.** No surveyed
   local-first system exposes per-record corrupt-data quarantine, which remains true. But
   poison-message quarantine is long-established in message brokers. Decision 006 is borrowed
   prior art that is unusual in local-first, not an invention.

## Cohort Correction

The original survey compared frontbox against nine systems, eight of which replicate *state* —
rows, documents, or CRDT updates. frontbox replays *HTTP commands*. That cohort mismatch produced
the survey's headline finding, which does not survive contact with frontbox's actual peers:

| System | Durable unit | Terminal-vs-transient split |
| --- | --- | --- |
| Workbox Background Sync | Serialized `{url, method, headers, body}` in IndexedDB | None. All failures requeue until `maxRetentionTime`. |
| Redux Offline | `effect: {url, method, json, headers}` | Yes. `discard()` defaults to 4xx; `retry()` returns `null` to discard. |
| TanStack Query | Persisted mutation state keyed by `mutationKey` | Partial. Paused mutations resume; ordering via `scope.id`. |
| Amplify DataStore | Named "outbox" with Hub events | Yes. Conflict handler returns a `DISCARD` sentinel. |

Among these, raw HTTP-envelope replay is the norm, not a divergence.

## Systems Considered And Excluded

- **2024-2026 cohort** (Triplit, LiveStore, TanStack DB, InstantDB, Jazz, Evolu, Ditto, Loro): not
  surveyed. Triplit ships an explicit offline outbox and LiveStore is a deterministic mutation log,
  so both are directly relevant. Recorded as a known gap rather than silently omitted.
- **Message brokers** (SQS redrive, Azure Service Bus dead-letter, Kafka): excluded as
  server-side, but they are the origin of both the dead-letter and poison-message-quarantine
  patterns frontbox uses. Relevant to decisions 005 and 006.
- **Mobile job schedulers** (Android WorkManager, `NSURLSession` background transfer): excluded.
  Relevant only to retry/backoff scheduling, which D1 does not implement.

## Gaps

- No surveyed local-first system exposes an official per-record local durable-data quarantine model
  comparable to frontbox decision 006. The pattern exists in message brokers instead.
- Retry and stalled-queue diagnostics are detailed in CouchDB/PouchDB and RxDB, thinner in
  PowerSync than the first draft claimed, and sparse in CRDT systems.
- Electric changed product shape twice. Current Electric is a read-path Postgres sync engine that
  additionally documents client write patterns; legacy ElectricSQL carried the local-write
  Satellite/oplog model. The synthesis treats these separately.
- Replicache is archived. Its design lessons stand; it should not be weighted as a live peer.

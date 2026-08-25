# Source Note 03 - RxDB And WatermelonDB

Systems: RxDB, WatermelonDB
Cohort: State replication, server-authoritative
Date: 2026-08-25
Verified: 2026-08-26. All claims re-checked against source; quotes and anchors added.

## Primary Sources

- RxDB Replication: `https://rxdb.info/replication.html` (OK)
- RxDB Transactions, Conflicts, and Revisions: `https://rxdb.info/transactions-conflicts-revisions.html` (OK)
- RxDB Schema Migration: `https://rxdb.info/migration-schema.html` (OK)
- WatermelonDB Sync Implementation: `https://watermelondb.dev/docs/Implementation/SyncImpl` (OK)
- WatermelonDB Frontend Sync: `https://watermelondb.dev/docs/Sync/Frontend` (OK)
- WatermelonDB Backend Sync: `https://watermelondb.dev/docs/Sync/Backend` (OK)

## Notes

### RxDB write and conflict model

RxDB replicates document states, not HTTP commands. The push handler receives rows containing an
assumed master state and a new client (fork) state; the pull handler is checkpoint-based and
batched. Conflict detection compares the assumed master state against the actual master state.

Per `transactions-conflicts-revisions.html`, each document carries a revision built from a revision
height — "a number, starting at `1`, incrementing on each write" — and a database instance token,
producing strings like `1-9dcca3b8e1a`. When the recorded previous revision does not match stored
state, RxDB raises a `409 CONFLICT`.

The backend returns conflicting master states and the client-side conflict handler resolves them.
The documented default is unambiguous:

> "The default conflict handler will always drop the fork state and use the master state instead."

That is server-wins by default, the same default posture as WatermelonDB's backend-as-source-of-
truth, and consistent with frontbox treating the server as authoritative.

### RxDB ordering — not causal

RxDB replication checkpoints are deterministic resume tokens for pull batching. They are *not*
per-client causal operation ordering, and the original note's inclusion of RxDB in the
monotonic-ordering finding was an overreach. The revision height increments per document, not per
client. See `manifest.md` `## Corrections`.

### RxDB observability and retry

RxDB exposes batching, retry timing, replication observables, conflict observables, explicit
`reSync()`, and in-sync helpers. Failed backend requests are retried later, and backend
implementations must tolerate duplicate transmissions. The active/error/conflict/in-sync observable
set is genuine stalled-queue visibility prior art and is the correct citation for that finding —
unlike PowerSync's front-entry warning, which is a misuse diagnostic (see note 02).

### RxDB migration as compatibility surface

Per `migration-schema.html`, schema and replication-state migration are explicit compatibility
surface. Migration strategies are required when schemas change, and replication metadata is
migrated so clients do not have to restart replication from scratch. This is the sharpest evidence
in the survey that durable format versioning becomes a public commitment.

### WatermelonDB write and conflict model

WatermelonDB tracks local row changes with a `_status` field (synced/created/updated/deleted) and a
`_changed` field listing columns changed since last sync. Sync is two-phase: pull, then push.

The documented default resolver, quoted precisely rather than paraphrased as "client-wins":

> "in conflict, server version is taken except for any column that was changed locally since last
> sync"

So the server version is the base and only locally-dirtied columns survive. After a successful
push, records are marked synced and `_changed` is reset, but the docs caution: "do not mark record
as synced if it changed locally since fetch local changes step," so a row that changed mid-sync
stays dirty for the next round. That mid-flight-change guard is directly relevant to frontbox's
atomic outcome application (decision 003): an outcome must not clear state the caller has since
modified.

### WatermelonDB backend contract

Per `docs/Sync/Backend`, the contract emphasizes transactionality. A push should abort if any
pushed record changed on the server since `lastPulledAt`, and the push endpoint should fully revert
local changes on server error. Unsafe per-collection batching breaks that transactionality.

## Gaps

- Official docs warn about malformed sync endpoint data and backend validation responsibilities,
  but neither system documents a local durable-row quarantine model.
- Neither documents client-side attempt counts or aging for a repeatedly failing record.

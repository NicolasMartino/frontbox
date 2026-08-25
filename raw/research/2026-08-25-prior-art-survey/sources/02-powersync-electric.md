# Source Note 02 - PowerSync And Electric

Systems: PowerSync, Electric/ElectricSQL
Cohort: State replication, server-authoritative
Date: 2026-08-25
Verified: 2026-08-26. Two dead URLs replaced, one claim recharacterized, Electric write patterns added.

## Primary Sources

- PowerSync Writing Data: `https://docs.powersync.com/client-sdks/writing-data` (OK)
- PowerSync Writing Client Changes: `https://docs.powersync.com/handling-writes/writing-client-changes` (OK)
- PowerSync Handling Write / Validation Errors: `https://docs.powersync.com/handling-writes/handling-write-validation-errors` (OK)
- PowerSync Sync Streams and Sync Rules: `https://docs.powersync.com/sync/overview` (OK)
- PowerSync Client Architecture: `https://docs.powersync.com/architecture/client-architecture` (OK, added 2026-08-26)
- PowerSync Client-Side Integration: `https://docs.powersync.com/configuration/app-backend/client-side-integration` (OK, added 2026-08-26)
- Electric repository: `https://github.com/electric-sql/electric` (OK; homepage now `https://electric.ax`)
- Electric Writes guide: `https://electric.ax/docs/guides/writes` (OK, added 2026-08-26)
- Legacy ElectricSQL Architecture: `http://web.archive.org/web/20250118030309/https://legacy.electric-sql.com/docs/reference/architecture` (archived snapshot)
- Legacy ElectricSQL Shapes: `http://web.archive.org/web/20250530113034/https://legacy.electric-sql.com/docs/usage/data-access/shapes` (archived snapshot)

## Citation Corrections

The original note cited the `legacy.electric-sql.com` host for both legacy Electric sources.
That host is dead: HTTPS fails with a certificate name mismatch and HTTP returns 404. Both are now
Wayback snapshots. Separately, `electric-sql.com` now redirects to `electric.ax`.

## Notes

### PowerSync write model

PowerSync is local SQLite-first. Application SQL writes are queued automatically as row-level
operations: `PUT`, `PATCH`, and `DELETE`, held in the `ps_crud` table. The application implements
`uploadData()` and may choose any backend API shape, but the backend should apply uploaded writes
synchronously to the source database. PowerSync explicitly warns against placing uploads into
another asynchronous backend queue before source-database application.

### PowerSync outcome handling

PowerSync distinguishes transient failure from terminal validation/conflict outcomes:

> "The backend should respond with 'success' (HTTP 2xx) even in the case of write conflicts or
> validation failures, unless developer intervention is desired."

Error responses are reserved for cases where "the change should stay in the client-side queue."
Acknowledged rejected changes are rolled back through server-authoritative state. PowerSync also
documents optional server-side dead letters, warning this "could result in out-of-order updates if
the client continues sending updates, despite earlier updates being persisted in the dead-letter
queue."

Note the divergence from frontbox: PowerSync pushes terminal-refusal handling to the *server* and
keeps the client queue dumb. frontbox keeps it client-side as `Rejected` plus a local dead letter,
which is closer to Replicache's model (note 01) and avoids PowerSync's out-of-order warning.

### PowerSync queue behavior — corrected

`architecture/client-architecture` describes the upload queue as "a blocking FIFO queue."
`configuration/app-backend/client-side-integration` describes the loop: the SDK "calls
`uploadData()` in a **loop** — not just once per trigger," continuing "until the queue is empty,"
and on error it "waits for the retry delay (default: 5 seconds)," retrying "the same upload
indefinitely, effectively blocking the upload queue."

**Correction.** The original note listed PowerSync's repeated-front-entry detection as stalled-queue
observability, alongside CouchDB scheduler state and RxDB observables. That is wrong. The docs
frame it as a developer-error diagnostic: if the same CRUD entry sits at the front of the queue
across consecutive iterations, the SDK logs a warning because this "typically means your
`uploadData()` implementation is not calling `.complete()`." It is a misuse warning, not a
queue-health signal, and should not be cited as evidence for D1's no-progress reporting.

**Correction.** The original note claimed PowerSync "carries per-client operation ids." The docs
confirm FIFO ordering but publish no per-client causal operation ID. That claim was an inference
from schema and has been withdrawn.

### PowerSync scope

PowerSync's partial replica is scoped with Sync Streams or legacy Sync Rules. Authenticated JWT
parameters are trusted for access control; client-provided parameters require care and should not
be used alone for authorization.

### Electric — read path, plus documented client writes

Current Electric is a read-path sync engine for Postgres. It syncs data out of Postgres into
clients or services through Shapes over HTTP; the engine itself is not a client mutation outbox.

**Correction.** The original note stopped there, which understated Electric. Electric officially
documents four client write patterns: online writes, optimistic state, shared persistent optimistic
state, and through-the-database sync. The fourth keeps "a log of local writes in a `changes` table"
— a persistent local write-ahead log synced through a write API, structurally close to frontbox.

Electric rates its own rejected-write handling as inadequate. The documented rollback strategy is
"very naive... clearing all local state and writes in the event of any write being rejected by the
server," with the suggestion that implementers may want "only clearing the set of writes that are
causally dependent on the rejected operation."

This is independent support for frontbox: a serious sync vendor documents per-record rejected-write
handling as an unsolved problem it leaves to application authors. frontbox's `Rejected` dead letter
and `Blocked` retention are the more careful answer, and Electric's "causally dependent" phrasing
is direct support for decision 005's refusal to dead-letter `Blocked` records.

Legacy ElectricSQL did include local writes through an oplog and Satellite replication, but that
model belongs to the legacy product line and its documentation now exists only in archive.

## Gaps

- No PowerSync or Electric source exposes a first-class quarantine policy for corrupt local durable
  upload/oplog rows.
- Neither documents client-side attempt counts or aging; PowerSync's answer is an indefinite retry
  with a fixed delay.

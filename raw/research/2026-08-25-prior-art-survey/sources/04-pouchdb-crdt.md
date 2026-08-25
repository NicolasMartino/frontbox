# Source Note 04 - PouchDB/CouchDB, Automerge, And Yjs

Systems: PouchDB/CouchDB, Automerge, Yjs
Cohort: State replication (CouchDB/PouchDB), CRDT (Automerge, Yjs)
Date: 2026-08-25
Verified: 2026-08-26. Section anchors added; ordering claim withdrawn; provider caveat added.

## Primary Sources

- CouchDB Replication Protocol: `https://docs.couchdb.org/en/stable/replication/protocol.html` (OK)
- CouchDB Replication and Conflict Model: `https://docs.couchdb.org/en/stable/replication/conflicts.html` (OK)
- CouchDB Database Security: `https://docs.couchdb.org/en/stable/api/database/security.html` (OK)
- PouchDB API: `https://pouchdb.com/api.html` (OK; `pouchdb.apache.org/api.html` also resolves)
- Automerge README: `https://github.com/automerge/automerge` (OK)
- Automerge Conflicts: `https://automerge.org/docs/reference/documents/conflicts/` (OK)
- Yjs Introduction: `https://docs.yjs.dev/` (OK)
- Yjs Shared Types: `https://docs.yjs.dev/getting-started/working-with-shared-types` (OK)
- Yjs Document Updates: `https://docs.yjs.dev/api/document-updates` (OK)

## Notes

### CouchDB/PouchDB write and conflict model

Per `replication/protocol.html` and `replication/conflicts.html`, CouchDB and PouchDB replicate
JSON documents identified by `_id` and `_rev`, using changes feeds, revision difference checks
(`_revs_diff`), and bulk document writes (`_bulk_docs`). A stale write against the same node
returns `409`. Concurrent replicated writes create revision-tree conflicts: a deterministic winner
is shown by default, but losing revisions remain accessible for application-level merge and
cleanup. Nothing is silently discarded — the closest thing in this cohort to frontbox's refusal to
drop data.

### CouchDB ordering — withdrawn as ordering evidence

**Correction.** The original note supported the monotonic-ordering finding with CouchDB revision
ancestry and checkpoints. Per `replication/protocol.html`, that is wrong on both counts:

- A checkpoint is an "Intermediate Recorded Sequence ID used for Replication recovery" — a
  resumption marker, so replication restarts mid-stream rather than from zero.
- `update_seq` is "The current database Sequence ID," and a Sequence ID is "An ID provided by the
  Changes Feed. It MUST be incremental, but MAY NOT always be an integer."

Both are **database-scoped**, not per-client. Revision ancestry is per-document causal history, not
per-client operation order. CouchDB is not evidence for a frontbox per-client enqueue sequence. See
`manifest.md` `## Corrections`.

### CouchDB/PouchDB observability

CouchDB and PouchDB expose batching, retry, replication events, scheduler states, and error counts.
This is genuine prior art for frontbox observability: a queue should not only retain work, it
should let callers see whether it is active, paused, denied, failed, or stalled. Along with RxDB's
observables (note 03) and Amplify's `outboxStatus` (note 05), this is the correct evidence base for
D1's no-progress signal.

### CouchDB scope and security

Per `api/database/security.html`, CouchDB security is database-level. Replication filters and
selectors are data-selection tools and should not be treated as tenant isolation by themselves.
This directly supports frontbox's cache/store namespace concern: a filter that narrows what you
read is not a boundary that prevents what you can read.

### Automerge and Yjs

Automerge and Yjs are CRDT systems. They persist local changes and merge concurrent updates without
server-authoritative rejection. Automerge exposes deterministic conflict winners plus conflict
inspection for same-property concurrent writes. Yjs shared types exchange binary updates that are
commutative, associative, and idempotent.

CRDT systems do not validate frontbox's dead-letter design, because they remove the server-refusal
problem from the core model. They remain useful evidence that local persistence, storage adapters,
binary formats, and provider protocols become public compatibility commitments.

**Provider caveat.** Retry, reconnect, and stall behavior in both systems is a property of the
*provider* (y-websocket, y-indexeddb, Automerge repo adapters, and third-party services), not of
the CRDT core. Neither core documents a retry or stall policy, so neither should be cited for or
against frontbox's retention semantics. Their relevance is confined to storage-format
compatibility.

## Gaps

- No official first-class corrupt local IndexedDB/update quarantine behavior for PouchDB, Automerge,
  or Yjs. Note that this pattern does exist outside local-first, in message-broker poison-message
  handling; see `manifest.md` `## Systems Considered And Excluded`.
- CRDT retry/stall behavior is undocumented at the core level by design.

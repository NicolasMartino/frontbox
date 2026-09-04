# What The IndexedDB Adapter Can And Cannot Say About A Row It Cannot Read

Document Class: Compatibility
Status: Accepted 2026-09-02
Date: 2026-09-02
Category: Storage Adapter
Scope: How `frontbox-indexeddb` behaves when a stored object does not match the row type it is read as, per store, and which of those behaviours is a limit rather than a choice.
Sources: `crates/frontbox-indexeddb/src/scan.rs`, `crates/frontbox-indexeddb/src/store.rs`, `crates/frontbox-indexeddb/src/rows.rs`, `crates/frontbox-indexeddb/src/versions.rs`, `crates/frontbox-indexeddb/tests/schema_mismatch.rs`
Related: `wiki/decisions/006-corrupt-record-policy.decision.md`, `wiki/decisions/025-quarantine-storage-shape.decision.md`, `wiki/decisions/032-opaque-row-store.decision.md`

## The Failure This Page Is About

Every other backend reads a row field by field into a raw struct: SQLite names its columns, so a row
is always *readable* and can only fail the later semantic `decode` — an identifier that will not
parse, a body that is not JSON, a timestamp outside the representable range. Those three are what
`CorruptKind` injects and what the conformance suite proves.

IndexedDB stores whole objects. A stored value can therefore fail one step earlier: it does not
match the row type **at all**, which is what an older schema version, a partially-completed
migration, or another writer in the same origin leaves behind. `serde` returns nothing, and there is
no raw struct to fall back to.

## What Happens Now, Per Store

| Store | A value that does not match its row type | Recoverable |
| --- | --- | --- |
| Outbox | Excluded from reads, then **quarantined by `sweep_corrupt`** with the stored text kept as `raw_body` | Yes — it is visible and inspectable |
| Read-model rows | Dropped from the projection for as long as it does not match | Yes — nothing deletes it; a later schema-compatible read returns it |
| Cache versions | Read as "no version known" | Yes — costs one refetch; the server can always restate a version |
| Dead letters | **Dropped from the listing** | **No** — see below |
| Quarantine | **Dropped from the listing** | **No** — see below |

## The Outbox Case Was A Defect And Is Fixed

`all_in_scope` filtered decode failures out with `filter_map`. For the outbox that was not a lossy
read, it was an **unreachable row**: absent from `pending_batch` and `pending_count`, which is the
documented contract, and absent from `sweep_corrupt`'s scan, which is the *one* path that exists to
make a corrupt row visible. It stayed in storage indefinitely — counted by nothing, named by
nothing, and deletable by nothing. That is precisely the outcome decision 006 rejects.

The scan now returns what failed alongside what decoded, with the primary key fetched from
`getAllKeys` because a value that will not parse cannot be asked for its own key. `sweep_corrupt`
quarantines both kinds in one pass. The quarantine entry has no `mutation_id` — the identifier lived
in the object that would not parse — which is the id-less path `QuarantinedRecord` already models
and only a sweep can reach.

The keys travel with the rows that *decoded* as well, which is what let the second copy of this
scan go away: `purge_older_than` had its own `getAll` + `getAllKeys` + zip, and `apply_outcomes`
rebuilt a delete key from the row's `seq` with `unwrap_or_default()` — a row missing that field
would have resolved to key `0`, a real key belonging to a real and possibly healthy record. Both now
delete by the key the scan read.

`crates/frontbox-indexeddb/tests/schema_mismatch.rs` is the evidence, and it is an adapter suite
rather than a conformance case on purpose: no other backend can reach this state through the same
door, so a fourth `CorruptKind` would be a variant every other factory had to imitate rather than
one it could produce.

## The Terminal Stores Are A Limit, Not A Choice

A dead-letter or quarantine row that does not match its type is **dropped from the listing**, and
that is a real loss: a rejection nothing can now show.

It is dropped rather than raised because the alternative is worse in the direction that matters. The
trait returns `Vec<DeadLetterRecord>`, so the only way to report the bad row is to fail the whole
call — and that listing is what a person reads to find out what the server refused. One unreadable
entry would hide every readable one beside it, which converts a lost record into a lost *view*.

It is not swept for the reason there is no sweep to write: a terminal store is where rows go to
stop. Quarantining a quarantine entry names nothing new, and a dead letter has no next state.

**What would fix it properly** is a listing that can carry per-row failures — `Vec<Result<…>>`, or a
report alongside the rows the way `PendingIndexGap` accompanies the pending index. That is a core
trait change affecting every backend, so it is recorded here and left to whoever needs it rather
than done as a side effect of an adapter fix. Nothing in this project's trials reaches it: the
terminal stores are written by this same adapter version and never by anything else.

## What This Does Not Claim

That an unreadable row is *understood*. The stored text is preserved verbatim so it can be
inspected, and the reason says only that the shape did not match. Deciding what an old schema meant
is a migration's job and this adapter does not attempt it.

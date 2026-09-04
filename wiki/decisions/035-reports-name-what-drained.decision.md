# A Report Names What Left The Queue

Document Class: Decision
Status: Accepted 2026-08-30; implemented 2026-08-30; prose swept 2026-08-31
Date: 2026-08-30
Category: Observability
Scope: Whether a sync report names the mutations that drained successfully, in what form, and what that lets a caller stop doing.
Sources: `src/runner/report.rs`, `src/runner/drain.rs`, `src/runner/mod.rs`, `examples/todo-core/src/app/index.rs`, `examples/todo-core/src/store.rs`
Related: `wiki/decisions/032-opaque-row-store.decision.md`, `wiki/decisions/020-observability-surface.decision.md`, `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/plans/d4a-offline-todo-trial.plan.md`, `wiki/references/open-decisions.reference.md`

## Decision

**`SyncReport` and `DrainReport` gain `drained: Vec<Drained>`** — one entry per record that left the
queue, naming the mutation, how it left, and the row it was bound to.

```
Drained { mutation_id, outcome: DrainedAs, row: Option<RowRef> }
DrainedAs = Applied | Duplicate | DeadLettered
```

The three variants are exactly the three terminal dispositions — the ones `made_progress` already
counts. A retained record is not drained and does not appear.

`row` is populated from decision 032's enqueue-time binding when the caller supplied one, and is
`None` otherwise, so a caller who does not bind sees the ids and nothing else.

## Why

**Finding 1 said the reports name ids only when something goes wrong.** `Anomaly` carries a
`mutation_id` for `UnknownMutation`, `RepeatedVerdict` and `UnknownStatus`; success is a count.
So a UI showing "saving…" per row has no event to decrement on, and `examples/todo-core` rebuilds
its whole pending index from `pending_batch` after every drain — affordable per drain, and the only
reason the index exists is that it must not be per render.

**Under decision 032 this is nearly free.** The runner already holds every id it is about to
dispose of; the binding already maps an id to a row. The report is naming facts the pass computed
anyway rather than gathering new ones.

**It is also RepForge's ask.** Their read-model convergence needs to know which rows a drain
settled — the same fact, from the other end. Answering it in the report is what lets both consumers
stop deriving it: one from a full index rebuild, the other by inference.

**Counts stay.** Decision 020 made aggregation the drain's job and the counts are what a status bar
reads. `drained` is the itemization beside them, not a replacement — a caller that only wants
"how many" must not have to walk a vector.

## Alternatives Rejected

- **Ids only, no disposition.** Cheaper and ambiguous: a dead-lettered record left the queue but
  its write did not happen, and a UI clearing "saving…" for it without saying so would be lying.
- **A separate `drained_ids()` query on the store.** A second round trip for something the pass
  already knew, and it would race the next pass.
- **Emit per-record events.** A callback surface where the crate currently has none, and decision
  020 already established that this crate reports rather than notifies.

## Consequences

- **`row_id_of` retires** in `examples/todo-core` — the path-parsing existed only to recover what
  the binding now records, and Finding 1's full rebuild becomes an incremental decrement.
- **`DrainReport::drained` concatenates its passes' vectors**, in pass order, which is the same
  aggregation rule the counts already follow.
- **`retained` and `sent` stay sums over sends, not records** — unchanged, and worth restating
  because `drained` *is* per record, so the two must not be read as parallel.
- **Conformance cases owed**: a drained record is named with its disposition; a dead letter appears
  as `DeadLettered`; a retained record does not appear; an unbound mutation reports `row: None`.

## Pages This Changed

Adding `drained` turned a full index rebuild into an incremental decrement, and four places went on
describing the world before it — two of them contradicting a doc comment a few lines below in their
own file. Listed because the pattern is the point: **a decision that makes something unnecessary
leaves prose behind more reliably than one that makes something possible.** Nothing fails when the
old explanation survives.

- `examples/todo-core/src/store.rs` — the `TodoStore` type doc said the index *cannot* be kept up to
  date from a drain's result, twenty lines above `unmark_pending`, which does exactly that.
- `examples/todo-app/src/sync.rs` — described `TodoApp::sync` as `drain()` plus `refresh_pending()`,
  and said a `DrainReport` names no successfully drained ids.
- `examples/todo-core/tests/observations/main.rs` — observation 3 said it asserted the negative and would
  invert when D5 landed. It had already inverted.
- `wiki/index.md` — listed "no report names the mutations that drained" as open work while its own
  catalog marked this decision implemented.

All four corrected 2026-08-31.

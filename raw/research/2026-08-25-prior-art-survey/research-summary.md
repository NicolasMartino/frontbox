# Prior-Art Survey Research Summary

Question: How should prior art affect frontbox before D1 implementation?
Scope: Replicache/Zero, PowerSync, Electric/ElectricSQL, RxDB, WatermelonDB, PouchDB/CouchDB,
Automerge, Yjs, and the HTTP command-queue cohort (Workbox Background Sync, Redux Offline,
TanStack Query, Amplify DataStore).
Date: 2026-08-25
Verified: 2026-08-26. See `manifest.md` `## Verification Pass` and `## Corrections`.

Each finding below names the source note it traces to.

## Key Findings

- **Two cohorts, not one.** State-replication engines (Replicache, Zero, PowerSync, Electric, RxDB,
  WatermelonDB, CouchDB/PouchDB) and CRDT systems (Automerge, Yjs) replicate rows, documents, or
  merge updates, and generally do not replay HTTP envelopes. HTTP command queues (Workbox
  Background Sync, Redux Offline, TanStack Query, Amplify DataStore) persist and replay serialized
  requests, and are frontbox's actual peer group. The first draft surveyed only the first cohort
  and concluded frontbox was unusual. It is not. [01, 02, 03, 04, 05]

- **The envelope is the serializable form.** TanStack Query documents the constraint directly:
  "only the state of mutations is persisted, as functions cannot be serialized," which is why a
  persisted mutation must carry a `mutationKey` and a registered default. Workbox stores
  `{url, method, headers, body}`; Redux Offline stores `effect: {url, method, json, headers}`.
  frontbox's `method`/`path`/`body` envelope is the mainstream solution to that constraint, not a
  workaround. [05]

- **Terminal versus transient is the load-bearing distinction, and it is near-universal.**
  Replicache: "If a permanent error is encountered such that the mutation will never be appliable,
  ignore that mutation and increment the `lastMutationID`... If a temporary error is encountered
  that might be resolved on retry, halt processing mutations and return." Redux Offline resolves
  every request to "success and commit, temporary failure and retry, and permanent failure and
  rollback." Zero skips throwing mutators and reverts the optimistic effect. PowerSync keeps
  transient failures queued. Amplify returns a `DISCARD` sentinel. [01, 02, 05]

- **Workbox is the counterexample that validates the design.** It has no terminal/transient split:
  a failed request "is put back in the same position in the queue," indefinitely, bounded only by
  `maxRetentionTime`. That is exactly the wedged-queue failure mode decision 005 avoids. [05]

- **Monotonic per-client causal ordering has exactly one documented precedent.** Replicache states
  a mutation ID "describe[s] a causal order to mutations from this client, and that order is
  respected by the server." No other surveyed system documents this. CouchDB's Sequence ID is
  "The current database Sequence ID" and its checkpoint is an "Intermediate Recorded Sequence ID
  used for Replication recovery" — database-scoped resumption, not per-client causality. RxDB
  checkpoints are resume tokens. PowerSync documents "a blocking FIFO queue" but no per-client
  causal operation ID. D1's `(created_at, mutation_id)` ordering is deterministic but not causal;
  the case for changing that rests on Replicache alone, plus TanStack Query's `scope.id` serial
  execution as weaker corroboration. [01, 02, 03, 04, 05]

- **Stalled-queue visibility is real but thinner than first reported.** CouchDB exposes scheduler
  state and error counts, PouchDB emits replication events, RxDB exposes active/error/conflict/
  in-sync observables, and Amplify DataStore emits `outboxStatus{isEmpty}` plus
  `outboxMutationEnqueued`/`Processed`/`Failed`. PowerSync's repeated-front-entry warning is *not*
  in this group: it is a developer-error diagnostic for a connector that fails to call
  `.complete()`. [02, 04, 05]

- **Bounded retention already has shipped answers.** Redux Offline's `retry()` returns a delay or
  `null` to discard, over a documented 1s-to-1h schedule, after which "it will be discarded."
  Workbox uses `maxRetentionTime`. D1's open question about attempt/aging metadata is not
  unexplored territory. [05]

- **Explicit local scope is a recurring safety requirement.** Replicache separates caches by `name`
  and warns that "each user of your application uses a different Replicache `name`." Zero uses
  authenticated context and storage identity. PowerSync scopes streams with authenticated
  parameters and warns against trusting client-supplied ones. CouchDB security is database-level,
  so filters and selectors are selection tools, not tenant isolation. [01, 02, 04]

- **Corrupt-record quarantine is borrowed, not invented.** No surveyed local-first system exposes a
  first-class per-record quarantine, which supports keeping decision 006. But poison-message
  quarantine is long-established in message brokers, so the accurate framing is "unusual in
  local-first, standard in queueing." [01, 02, 03, 04]

- **Durable storage format and migration behavior become public compatibility surface quickly.**
  RxDB requires migration strategies and migrates replication metadata so clients need not restart
  replication; WatermelonDB, PouchDB, Automerge, Electric, and PowerSync all show similar
  format-compatibility pressure. [02, 03, 04]

- **Electric documents client writes, and rates its own rollback as inadequate.** Its
  through-the-database pattern keeps "a log of local writes in a `changes` table" and its stated
  rollback strategy is "very naive... clearing all local state and writes in the event of any write
  being rejected by the server." Independent support for frontbox investing in per-record outcomes.
  [02]

## Frontbox Implications

- Keep the HTTP-envelope outbox. Reframe it as the standard contract for its cohort rather than a
  documented divergence.
- Keep explicit outcomes and dead letters for `Rejected`. They are more inspectable than Workbox's
  indefinite requeue and Electric's clear-everything rollback.
- Keep fresh auth per send and avoid persisted credential snapshots. Redux Offline's async
  `discard` refreshing a token on 401 is the closest prior art.
- Promote cache/store namespace design from an implementation detail to an API concern.
- Keep D1 no-progress reporting. Model any follow-up on Amplify's `outboxStatus{isEmpty}` and on
  Redux Offline's `retry() -> null` discard rather than inventing a scheme.
- Revisit monotonic enqueue sequence before durable backends, on Replicache's evidence alone, and
  record that the supporting evidence is one system rather than four.
- Keep corrupt-record quarantine, citing message-broker poison-message handling as its lineage.

## Ingest Readiness

Ready, with the cohort limitation recorded. The source set covers every system listed in
`wiki/plans/prior-art-survey.plan.md`, plus the HTTP command-queue cohort that plan omitted, with
at least one primary source per system. Systems deliberately not surveyed are listed in
`manifest.md` `## Systems Considered And Excluded` so the omission is visible rather than implied.

# Source Note 05 - HTTP Command Queues

Systems: Workbox Background Sync, Redux Offline, TanStack Query offline mutations, AWS Amplify DataStore
Cohort: HTTP command outbox, server-authoritative
Date: 2026-08-26
Verified: 2026-08-26. Added during the verification pass; not part of the 2026-08-25 draft.

## Why This Note Exists

The original survey compared frontbox against nine systems, eight of which replicate *state* — rows,
documents, or CRDT updates. frontbox replays *HTTP commands*. That cohort mismatch produced the
draft's headline finding, "mature systems generally do not replay raw HTTP envelopes," which is
true of the surveyed cohort and false of frontbox's actual peers.

This note surveys those peers. In this cohort, serialized HTTP-envelope replay is the norm.

## Primary Sources

- Workbox Background Sync: `https://developer.chrome.com/docs/workbox/modules/workbox-background-sync` (OK)
- Redux Offline config API: `https://github.com/redux-offline/redux-offline/blob/master/docs/api/config.md` (OK)
- Redux Offline request customization: `https://github.com/redux-offline/redux-offline/blob/master/docs/recipes/customize-requests.md` (OK)
- TanStack Query mutations guide: `https://tanstack.com/query/latest/docs/framework/react/guides/mutations` (OK)
- Amplify DataStore events: `https://docs.amplify.aws/gen1/react/build-a-backend/more-features/datastore/datastore-events/` (OK)
- Amplify DataStore conflict resolution: `https://docs.amplify.aws/gen1/javascript/build-a-backend/more-features/datastore/conflict-resolution/` (OK)

## Workbox Background Sync

The service-worker-standard answer, and the closest structural match to frontbox's durable unit.

Requests are serialized into IndexedDB via a `StorableRequest` that converts a `Request` into "a
plain object that can be structured cloned or JSON-stringified," capturing URL, method, headers, and
body. That is frontbox's `method`/`path`/`body` envelope, in the browser platform's own idiom.

Replay is strict FIFO with no outcome model:

> "If any request fails to re-fetch, it's put back in the same position in the queue (which
> registers a retry for the next sync event)."

There is no terminal-versus-transient distinction. Every failure requeues at the head, indefinitely,
bounded only by `maxRetentionTime` (in minutes; the documented example is 24 hours). The
`BackgroundSyncPlugin` only enqueues requests that *throw* — it does not queue 4xx or 5xx responses
unless the developer adds a `fetchDidSucceed` callback.

**Relevance to frontbox.** Workbox is the counterexample that validates decision 005. A permanently
refused request in a Workbox queue blocks the head forever, which is precisely the wedge decision
005 avoids by dead-lettering `Rejected` and retaining only `Blocked`/`Pending`. Its
`maxRetentionTime` is also shipped prior art for the aging question D1 currently defers.

## Redux Offline

The closest overall match found in the survey, and the one the original draft most needed.

The durable unit is an HTTP envelope: `effect: { url, method, json | body, headers }`. The queue is
explicitly pluggable FIFO with `enqueue`, `dequeue`, and `peek`.

The outcome model is frontbox's, stated in three terms:

> "`config.effect` and `config.discard` together are responsible for executing a serialized request
> and resolving it to one of three outcomes. These outcomes and their resulting actions are success
> and commit, temporary failure and retry, and permanent failure and rollback."

The terminal/transient boundary defaults to HTTP status class — "The default implementation discards
only on client errors," i.e. 4xx — with the reference implementation:

```js
const discard = (error, _action, _retries) => {
  const { request, response } = error;
  if (!request) throw error;      // error creating the request
  if (!response) return false;    // no response: transient
  return 400 <= response.status && response.status < 500;
};
```

Bounded retention is solved, not deferred. `retry(action, retries)` returns "either the number of
milliseconds to wait before retrying, or `null` if the action should be discarded," over a
documented schedule of 1s, 5s, 15s, 30s, 1m, 3m, 5m, 10m, 30m, 1h — after which "If a request fails
after this point, it will be discarded."

Auth refresh is handled at the same seam: the documented async `discard` example calls
`refreshAccessToken()` on a 401 and returns `false` so the request retries with a fresh token.

**Relevance to frontbox.**
- Validates the envelope as the durable unit (decision: extraction boundary).
- Validates 4xx-as-terminal, which is decision 005's `Rejected`.
- Its `retry() -> null` discard is a shipped answer to D1's open attempt/aging question. Note that
  Redux Offline *discards*, whereas frontbox dead-letters — frontbox is strictly more recoverable.
- Its 401-refresh seam is direct support for decision 004, fresh auth per send rather than a
  credential snapshot persisted inside the queued record.

## TanStack Query Offline Mutations

The widest-deployed instance of the constraint that makes the envelope necessary:

> "only the state of mutations is persisted, as functions cannot be serialized"

Because `mutationFn` cannot be persisted, a resumed mutation must be re-attached by a serializable
key via `setMutationDefaults`. Without it, "the component that triggers the mutation might not be
mounted, so calling `resumePausedMutations` might yield an error: `No mutationFn found`."

Ordering is opt-in and coarse: mutations sharing a `scope.id` "will run in serial."

**Relevance to frontbox.** This reframes the survey's central finding. The named-mutator systems in
notes 01-04 can afford named operations because the operation body lives in code on both ends.
A persisted client-side queue cannot persist a function, so it must persist a serializable
description of the call. frontbox's `{method, path, body}` *is* that description. The envelope is
the mainstream solution to a real constraint, not a RepForge convenience. `scope.id` is also weak
corroboration for per-operation ordering keys, though far weaker than Replicache's causal ordering.

## AWS Amplify DataStore

Uses frontbox's own vocabulary. The local durable queue is called the **outbox**, with Hub events
`outboxMutationEnqueued`, `outboxMutationProcessed`, `outboxMutationFailed`, and `outboxStatus`
carrying an `isEmpty` boolean "to notify if there are mutations in the outbox," fired each time a
mutation is enqueued and each time one finishes processing.

Conflict resolution offers auto-merge (default), optimistic concurrency, and a custom Lambda
handler. A custom conflict handler "runs if a mutation is rejected by AWS AppSync" and returns
either a modified model or the `DISCARD` symbol.

**Relevance to frontbox.** `outboxStatus{isEmpty}` plus enqueued/processed/failed events is a
shipped, minimal observability surface, and a better model for D1's no-progress signal than
inventing one. The `DISCARD` sentinel is a third independent instance of the terminal-refusal
decision point.

## Cohort Summary

| System | Durable unit | Terminal vs transient | Bounded retention | Queue observability |
| --- | --- | --- | --- | --- |
| Workbox Background Sync | Serialized `Request` in IndexedDB | None | `maxRetentionTime` | None |
| Redux Offline | `effect: {url, method, json, headers}` | `discard()`, defaults to 4xx | `retry() -> null` after 1s-1h schedule | Redux state |
| TanStack Query | Persisted mutation state + `mutationKey` | Paused vs failed | Retry count | Mutation cache |
| Amplify DataStore | Named "outbox" | `DISCARD` sentinel | Not documented | `outboxStatus{isEmpty}` + events |
| **frontbox (planned)** | `{method, path, body}` envelope | `Rejected` -> dead letter; `Blocked`/`Pending` -> retain | **Open (D1 defers)** | No-progress signal |

frontbox sits inside this cohort, not outside it. Its distinguishing choice is that a terminal
refusal becomes an inspectable dead letter rather than a discard or a rollback — which is more
recoverable than any of the four, and is the design position worth defending.

## Gaps

- No system in this cohort exposes per-record corrupt-data quarantine either. The pattern's home is
  message brokers, not client queues.
- 2024-2026 local-first systems with explicit outboxes (notably Triplit) remain unsurveyed. See
  `manifest.md` `## Systems Considered And Excluded`.

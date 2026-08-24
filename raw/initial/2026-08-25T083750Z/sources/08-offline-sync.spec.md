# Offline & Sync

- Document Class: Spec
- Status: Active
- Date: 2026-04-15
- Category: Offline behavior
- Scope: Current product behavior

This spec defines the current backend contract for offline-capable clients.

## Client Profiles

RepForge is offline-first on supported UI platforms:
- Mobile apps MUST maintain local state and a queued mutation outbox.
- Desktop apps MUST maintain local state and a queued mutation outbox.
- The web app SHOULD maintain local state and a queued mutation outbox when durable browser storage is available.

Authentication is still online-only. A device must have logged in successfully at least once before it can operate from cached state.

## Core Endpoints

| Endpoint | Purpose | Status |
|---|---|---|
| `POST /api/v1/sync` | Flush queued mutation intents | Current |
| `GET /api/v1/sync/state` | Refresh the full authoritative read model | Current |
| `GET /api/v1/sync/changes` | Incremental pull API | Post-MVP |

## Push Model

Clients replay offline writes through `POST /api/v1/sync` using `MutationBatchRequest`.

Each mutation intent contains:
- `mutation_id`: client-generated idempotency key
- `method`: public HTTP method
- `path`: public BFF write path
- `client_datetime`: client timestamp for diagnostics and expiry checks
- `body`: the same mutation payload the matching public endpoint accepts

Example:

```json
{
  "mutations": [
    {
      "mutation_id": "01912345-6789-7abc-def0-123456789abc",
      "method": "PATCH",
      "path": "/api/v1/exercises/01912345-6789-7abc-def0-123456789def",
      "client_datetime": "2026-04-14T08:12:00Z",
      "body": {
        "name": "Bench Press"
      }
    }
  ]
}
```

## Server Behavior

On `POST /api/v1/sync`, the BFF:
1. authenticates the caller;
2. validates batch shape and duplicate `mutation_id` values;
3. routes each mutation through the same operation-specific validation and authorization rules used by the matching direct endpoint;
4. persists one mutation job per accepted intent;
5. dispatches jobs to the owning service over trusted internal HTTP;
6. waits up to the configured timeout for job completion;
7. returns one result per mutation intent.

The response shape is `MutationBatchResponse`:

```json
{
  "results": [
    {
      "mutation_id": "01912345-6789-7abc-def0-123456789abc",
      "status": "Applied",
      "error": null
    }
  ]
}
```

## Result Statuses

| Status | Meaning |
|---|---|
| `Applied` | Mutation changed state successfully |
| `Duplicate` | Replay converged idempotently |
| `Rejected` | Validation, authorization, or business rule failure |
| `Blocked` | Skipped because an earlier mutation in the same ordered sequence failed terminally |
| `Pending` | Accepted but not confirmed before timeout |

`Pending` is emitted only by the BFF.

## Ordering and Blocking

- Mutations in a sync batch are evaluated in request order.
- The BFF preserves sequence numbers when it persists jobs for one correlated batch.
- If an earlier mutation fails terminally, later mutations in the same correlated batch are returned as `Blocked` instead of being dispatched.
- This allows clients to replay a queue deterministically without inventing cross-service rollback semantics.

## Idempotency

- `mutation_id` is the stable client idempotency key.
- The BFF forwards the mutation id to internal services using the `Idempotency-Key` header.
- Replaying the same mutation intent with the same `mutation_id` must converge safely to `Applied` or `Duplicate`.

## Pull Model

`GET /api/v1/sync/state` is the current authoritative pull endpoint.

Clients should use it when:
- the local outbox is empty; or
- they need a full refresh after reconnect; or
- SSE invalidations indicate their local view is stale.

The current MVP pull strategy is full-state refresh, not cursor-based incremental replay.

`GET /api/v1/sync/changes` remains a future extension for incremental sync and is not part of the current baseline.

## Client Rules

- Clients MUST NOT invent internal service URLs or bypass the BFF for authoritative user writes.
- Clients MUST use the same request-body JSON shape for a direct endpoint and the matching sync item.
- Clients SHOULD preserve queued mutation order.
- Clients SHOULD retry `Pending` mutations with the same `mutation_id`.
- Clients SHOULD surface terminal `Rejected` or `Blocked` results to the user or local conflict-resolution flow.

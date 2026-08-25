# Source Note 01 - Replicache And Zero

Systems: Replicache, Zero
Cohort: State replication, server-authoritative
Date: 2026-08-25
Verified: 2026-08-26. All URLs re-fetched; one citation added, one status correction.

## Primary Sources

- Replicache How It Works: `https://doc.replicache.dev/concepts/how-it-works` (OK)
- Replicache Server Push Reference: `https://doc.replicache.dev/reference/server-push` (OK, added
  2026-08-26 — the original note asserted the permanent-failure rule but cited no page carrying it)
- Replicache repository: `https://github.com/rocicorp/replicache` (OK, **archived**, last push
  2022-05-07)
- Zero Mutators: `https://zero.rocicorp.dev/docs/mutators` (OK)
- Zero Queries: `https://zero.rocicorp.dev/docs/queries` (OK)
- Zero Authentication: `https://zero.rocicorp.dev/docs/auth` (OK)

## Project Status

`rocicorp/replicache` is archived, last pushed 2022-05-07, and Rocicorp's active work is Zero.
Replicache's design lessons stand and its server-push reference is the single most directly
applicable document in this survey, but it should not be weighted as a live peer system.

## Notes

### Write model

Replicache writes are named mutator invocations with JSON args, persisted as
`{id, name, args}` — for example `{id: 1, name: "createTodo", args: {...}}`. A local mutator
updates the persistent client view immediately and queues a persisted mutation. Replicache assigns
a per-client sequential mutation id and later sends pending mutations in batches to a server push
endpoint. Server state is canonical; pull rebases pending local mutations over confirmed server
state, and the pull response carries `lastMutationIDChanges` to confirm what the server has seen.

Zero also writes through named mutators. A mutator first runs against the client-side datastore,
then Zero sends a mutation record to a server mutate endpoint. The server-side mutator runs
transactionally against Postgres, and Postgres logical replication feeds authoritative row changes
back through `zero-cache`.

Both favor named domain mutations over raw HTTP-envelope replay. That is a property of their
cohort, not a verdict on frontbox: see `sources/05-http-command-queues.md`, where serialized HTTP
envelopes are the norm. RepForge already stores `method`, `path`, and JSON `body`, so the envelope
survives — but operation naming and versioning still matter for diagnostics and compatibility.

### Ordering — the survey's only direct evidence

`concepts/how-it-works` states the mutation id "describe[s] a causal order to mutations from this
client, and that order is respected by the server," and defines it as "a sequential integer
uniquely identifying the mutation in this client."

This is the *only* documented per-client causal operation ordering in the entire survey. CouchDB
sequence IDs are database-scoped, RxDB checkpoints are resume tokens, and PowerSync documents FIFO
without a published causal operation ID (see notes 02 and 04). Any frontbox decision to add a
monotonic enqueue sequence rests on this one precedent, and should say so.

### Terminal versus transient outcomes

`reference/server-push` is the closest prior art to frontbox decision 005, and states the rule
directly:

> "If a mutation is invalid or cannot be handled, the server must still mark the mutation as
> processed by updating the `lastMutationID`. Otherwise, the client will keep trying to send the
> mutation and be blocked forever."

> "If a permanent error is encountered such that the mutation will never be appliable, ignore that
> mutation and increment the `lastMutationID`."

> "If a temporary error is encountered that might be resolved on retry, halt processing mutations
> and return."

That is decision 005's split: terminal refusal advances past the record (frontbox dead-letters it
rather than discarding it, which is strictly more recoverable), while transient failure halts and
retains. Zero reaches the same outcome differently: `handleMutateRequest` "skips any mutations that
throw," the optimistic mutation on the client is reverted, structured error information returns to
the caller, and later mutations proceed.

### Local scope

Replicache isolates browser caches through the `name` constructor parameter and warns that "each
user of your application uses a different Replicache `name`. That way, different users will have
separate caches." Zero uses server-shaped query registries, authenticated context, and local
storage identity options. Both support frontbox making user/tenant/query scope explicit in local
cache and store APIs rather than leaving it to caller discipline.

## Gaps

- No official per-record quarantine policy for corrupt browser-side durable mutation records.
- Neither project documents attempt counts or aging on the client queue; Replicache's answer is to
  advance `lastMutationID` server-side instead.

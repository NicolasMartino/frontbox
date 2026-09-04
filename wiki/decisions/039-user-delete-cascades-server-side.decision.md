# A User Delete Cascades In The User Service

Document Class: Decision
Status: Accepted 2026-09-01; built and proven the same day by observations 11-13
Date: 2026-09-01
Category: Protocol
Scope: What happens to a user's todos when the user record is deleted, which service performs it, and the dependency cycle that answer creates.
Sources: `examples/user-server/src/todos.rs`, `examples/todo-server/src/direct.rs`, `examples/todo-core/src/app/identity.rs`
Related: `wiki/decisions/036-no-sub-scope-partitions.decision.md`, `wiki/decisions/037-multi-service-routing.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/019-verdict-synthesis.decision.md`, `wiki/plans/d4d-multi-domain-trial.plan.md`

## Decision

**`DELETE /api/v1/users/{id}` empties the user of todos before removing them, and the user service
does it.** The client enqueues one mutation. `user-server` asks `todo-server` for that user's todos,
deletes each, and only then deletes the record — refusing the whole batch if any step fails.

## What This Reverses

`examples/user-server/src/domain.rs` carried a written argument for having **no delete at all**, and
`wiki/plans/d4d-multi-domain-trial.plan.md` listed one under `## Explicitly Not Built`. Both said the
same true thing: `todo-server` checks a todo's owner when the todo is *written* and never again, so
a bare delete leaves orphans nothing detects.

That exclusion was waiting for an answer rather than refusing one. This is the answer.

## Why The Server And Not The Client

The client could enqueue a delete per todo and then the user delete, and decision 036's single queue
would order them correctly. That works and it is the wrong place, for one reason:

**A second device deleting the same user would not benefit from it.** Ordering enforced in one
client is a property of that client. Ordering enforced in the service is a property of the system,
and the guarantee a user of these APIs actually wants is "this user's todos are gone when this user
is gone", not "…if the client that deleted them was mine".

It also keeps the client honest about what a mutation is. `TodoApp::delete_user` enqueues exactly
one envelope; the cascade is a consequence the server owns, which is what
`observation_12_deleting_a_user_cascades_to_their_todos` asserts by counting the outbox at one.

## The Cost: The Two Services Now Call Each Other

`todo-server` calls `user-server` to check a todo's owner. `user-server` calls `todo-server` to
cascade a delete. **That is a dependency cycle and it is a genuine smell**, recorded here rather
than discovered:

- A real split would break it — with an event, a server-side outbox, or by not having the reference.
- **Compose cannot express it.** `todo-server` already declares `depends_on: user-server`; the
  reverse is circular and Compose refuses to start it. Neither calls the other at *startup*, so
  `user-server` gets the address and no `depends_on`.
- **Neither service can be constructed before the other's address is known.** The in-process test
  harness has to bind both ports, then build both states, then serve — `examples/todo-core/tests/common/mod.rs`
  says so where it does it. In Compose the addresses are static config and the problem is invisible,
  which is exactly why it is worth writing down.

It is here because **a user delete is the first flow in this trial that exercises ordering
destructively**, and doing it anywhere else would not test that.

## Refuse The Batch, Never Half-Cascade

If the cascade cannot complete — the todo service is unreachable, or refuses a delete — the user
service returns `503` and applies **nothing**. The user record survives, the client retains the
mutation, and the retry runs the whole cascade again.

**A cascade that pressed on past a failure would be worse than no cascade**: the record gone, one
todo left behind, and no signal anywhere. `observation_13_a_failed_cascade_leaves_the_user_alone` is
the half of the pair that makes the other half safe.

The retry is free for everything already deleted because each cascaded delete carries
`x-mutation-id: {user_delete_mutation_id}:todo:{todo_id}` — derived from the mutation that caused
it, so `todo-server`'s idempotency table absorbs the repeat. Deterministic rather than random, which
also means `user-server` needs no uuid dependency.

## `1 + N` Calls, Chosen

One `GET` for the list, then one `DELETE` each. A bulk `DELETE /api/v1/todos?user_id=X` would be one
call and is what a real system should want.

The per-todo form is kept because **this is a demo whose subject is order**, and it puts one line
per delete in the server log — the cascade becomes something you watch rather than infer. Confirmed
in containers: deleting a user with two todos produces exactly
`DELETE /api/v1/todos/b1 -> 204` and `DELETE /api/v1/todos/b2 -> 204`.

A trial that hid its own mechanic behind a single call would be a worse trial. A production service
should make the opposite trade, and this paragraph is where it is written down.

## What The Client Does Not Do

**It does not withdraw queued work for the deleted user.** A todo *create* still in the outbox when
its user is deleted will drain, meet `404 unknown_user`, be classified `Blocked` by the routing
transport (decision 019 via 037), and be retained until decision 017's bound dead-letters it.

That is correct and worth showing rather than special-casing: **an enqueued mutation is a durable
fact**, and the dead-letter panel is where a human learns what became of it. A client that reached
into its own outbox to cancel writes would be a client whose queue means something different from
one drain to the next.

Locally the projection and the row store lose both the user and their todos immediately, because the
server is going to — the same optimistic rule every other write follows, applied to a consequence.

## Alternatives Rejected

- **Cascade in the client.** Correct for one device, silent for the next. Above.
- **Cascade in `todo-server`, triggered by an event from `user-server`.** The right shape, and it
  needs a server-side outbox that `wiki/proposals/invalidation-delivery.proposal.md` already defers.
  Revisit with SSE, which is when that outbox becomes necessary anyway.
- **Refuse to delete a user who has todos.** Honest, and it makes the UI's delete button dead most
  of the time. It also tests nothing about ordering, which is the reason the flow was built.
- **Leave orphans.** What the original exclusion was avoiding.

## Revisit If

- A server-side invalidation outbox lands. The cascade should become an event, and this decision
  should shrink to "the user service publishes; the todo service reacts".
- The `1 + N` shape becomes a measurable cost, which for a demo with tens of rows it is not.

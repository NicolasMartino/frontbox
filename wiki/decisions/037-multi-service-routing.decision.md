# One Outbox, One Scope, A Routing Transport

Document Class: Decision
Status: Accepted 2026-08-31; **proven by D4d the same day**, with one interaction it did not anticipate — see `## What D4d Proved, And The One Thing It Found`
Date: 2026-08-31
Category: Protocol
Scope: How a client whose writes span several backend services queues and drains them, which decision 034 explicitly left open.
Sources: `src/runner/mod.rs`, `src/transport.rs`, `src/store/mod.rs`
Related: `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/018-single-flight-drain-mode.decision.md`, `wiki/decisions/019-verdict-synthesis.decision.md`, `wiki/decisions/034-no-service-origin-in-core.decision.md`, `wiki/decisions/036-no-sub-scope-partitions.decision.md`, `wiki/plans/d4d-multi-domain-trial.plan.md`

## Decision

**A multi-service client uses one outbox under one scope, and routes inside `SyncTransport`.** Core
gains nothing: no origin column, no per-service queue, no routing concept.

This is not a new position so much as the join of two existing ones that had never been put
together:

- **Decision 034**: *"A service is a transport concern, not a record one. `SyncTransport` is the
  seam that knows where a batch goes."* It settled that no durable column is needed and left the
  routing question open because nothing durable depended on it.
- **Decision 036**: refused to partition the outbox, and its stated reason was ordering — *"One
  queue replays the caller's enqueue order across types at no cost. Split it and 'create the list,
  then create the todo in it' may drain in either order."*

Read together they already answer the question. This page states the answer, and D4d is what makes
it falsifiable.

## What This Buys, And What Has Never Been Tested

**Cross-service enqueue order is preserved**, because there is one `seq` and one queue. "Create the
user, then create their todo" drains in that order even though the two records go to two different
servers.

That is the guarantee decision 036 paid for by declining to partition, and **nothing has ever
exercised it.** Every test in this project sends to one server. A guarantee whose only evidence is
the absence of a second server is not evidence.

## The Batch Mechanic, Which Is Already Modelled

`SyncTransport::send_batch` takes one `MutationBatchRequest` and returns one
`MutationBatchResponse`. A routing transport therefore has to split a batch by destination, issue
several HTTP calls, and merge what comes back. The awkward case is partial: server A answers,
server B is unreachable.

**Core already handles a response that does not name every record it was sent.** `src/runner/mod.rs`
retains any such record with `reason: Some("no verdict returned")`, `attempts` increments, and
decision 017's bound applies exactly as it does to a record the server declined to rule on. So a
routing transport may return the verdicts it obtained and stay silent about the rest, and the
records for the unreachable service are retained, counted, and retried.

That this needed no new protocol is a real result about decision 019's shape: **synthesis was built
for a server that declines to rule, and a transport that could not reach one is the same fact from a
different cause.**

## What It Costs

**Head-of-line blocking spans services.** At `batch_limit = 1` — decision 018's single-flight
profile — one wedged record for a down service blocks records destined for a healthy one. Decision
017's retention bound terminates it rather than making it permanent, which is the same answer 036
already gave: the blast radius is bounded, and narrowing it is not worth an ordering guarantee.

**The escape hatch is more scopes, and it is expensive.** A client that genuinely needs two services
isolated opens two scopes, which is two queues with two independent `seq` counters. That buys
isolation and **loses cross-service ordering entirely** — the thing this decision exists to keep.
It is available, it is sometimes right, and it should never be reached for casually.

## How D4d Will Prove It

D4d is a draft plan, so nothing below has happened yet; this states the evidence the decision is
waiting on. By sabotage, which is the standard the rest of this project holds itself to and which
036 currently has none of:

- **One scope**: the user record drains before the todos that reference it. Every drain succeeds.
- **Two scopes, one per service**: ordering is gone, and the todo server receives a todo for a user
  it cannot find.

**The classification of that failure is the transport's, and this page must not imply otherwise.**
Decision 019 opens with "core does not classify HTTP responses and will not". What it places on a
*transport* is the obligation that a missing prerequisite is transient and must never be mapped to
`Rejected`. A routing transport discharging that obligation maps the `404` to
`MutationStatus::Blocked` — the status whose condition 019 describes as "this write failed only
because a predecessor had not landed" — rather than to `Unknown`, which 019 reserves for a response
the transport *cannot* confidently classify, and this one it can.

**So the observable surface is the retained one, not the anomaly one.** The record is retained with
`reason: Some("blocked")`, counted in `counts.blocked`, and carries `"blocked"` in decision 033's
`last_error` column. Anomalies are raised only by `Unknown`. Stated precisely because "it shows up
in the anomaly surface" would send whoever writes the test to the wrong assertion.

The second half is what makes the first half evidence. A test that only shows the good path proves
that this drain worked, not that ordering held.

## What D4d Proved, And The One Thing It Found

**Both halves, as this page asked for.** `examples/todo-core/tests/multi_domain/main.rs`, observations 2
and 3:

- **One scope**: the user record drains before the todos referencing it, asserted on the servers'
  state rather than on the client's queue — every todo landed owned by the user record that preceded
  it.
- **Two scopes, one per service**: ordering is gone, the todo server answers `404`, and the routing
  transport maps it to `MutationStatus::Blocked`. Retained, counted in `counts.blocked`, no anomaly,
  no dead letter — and it **heals on the retry** once the user record lands, which is what makes
  `Blocked` the right classification rather than merely the kinder one.

The paragraph above about the observable surface being the retained one, not the anomaly one, was
worth writing: it is exactly where a test would otherwise have been pointed wrong.

**The partial-batch mechanic needed no protocol change**, as predicted. Observation 4 stops the todo
service; the user mutation applies, the todo is retained with `reason: Some("no verdict returned")`,
and decision 017's bound moves.

### The interaction this page did not anticipate

**A partial batch spends the retention bound twice per drain, not once.** Observation 4 expected
`attempts == 1` and found 2.

A drain is a loop that stops on the first pass making **no progress**
(`wiki/decisions/029-drain-termination.decision.md`). The user record draining *is* progress, so the
loop runs a second pass, re-sends the todo to the still-silent service, and spends a second attempt
before stopping.

So **037 and 029 interact**: routing makes partial progress the normal case, and 029's termination
rule turns partial progress into an extra send to whichever service is down. A client whose services
fail independently reaches decision 017's bound in roughly half the drains a single-service client
would.

This is not a defect in either page — the bound exists to terminate a wedged head, and it still
does — but it changes a number a caller setting that bound has to reason about, and neither page
said so before a two-service client existed to make it happen.

## Alternatives Rejected

- **A durable `origin` column.** Decision 034 rejected it and nothing here reopens it: the path
  already carries the destination, and a second blessed way to say the same thing forces core to
  decide which wins. Routing pinned into storage also survives a topology change it should not.
- **One outbox per service, always.** This is decision 036's partition under another name, and it
  trades cross-service ordering away by default rather than on purpose. It remains available per
  scope for clients that want it.
- **`send_batch` per destination in core** — core splitting the batch and calling the transport
  once per service. It puts routing in core, which 034 declined, and it requires core to parse the
  path to learn a destination, which is exactly the interpretation decision 008 keeps it out of.

## Revisit If

- A client appears whose services have genuinely independent ordering *and* whose head-of-line
  blocking is not tolerable. Two scopes already serve it; if that turns out to be the common case
  rather than the exception, the default is wrong.
- A routing transport turns out to need something from core it cannot compose — a way to say "this
  batch was split" in a report, say. That would be evidence routing is not purely a transport
  concern after all, and 034 would owe a re-examination.

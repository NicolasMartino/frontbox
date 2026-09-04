# D4d: Two Domains, One Queue, And The First Invalidation That Ever Runs

Document Class: Plan
Status: **Built 2026-08-31** — see `## Outcome`
Date: 2026-08-31
Category: Extraction
Scope: A second example server, the cross-service ordering guarantee decision 036 paid for and nothing has tested, and the first application code that ever consumes frontbox's invalidation runtime.
Sources: `examples/todo-server/`, `examples/todo-core/`, `examples/todo-app/`, `src/cache/`, `src/runner/mod.rs`
Related: `wiki/proposals/invalidation-delivery.proposal.md`, `wiki/decisions/037-multi-service-routing.decision.md`, `wiki/decisions/038-invalidation-delivery-in-the-trial.decision.md`, `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/decisions/036-no-sub-scope-partitions.decision.md`, `wiki/plans/d4c-multi-platform-trial.plan.md`

## Why A Second Server

Two things are unprovable with one server, and both are guarantees this project has already paid
for.

**Cross-service ordering.** Decision 036 refused to partition the outbox, and paid for that refusal
with head-of-line blocking, explicitly to keep enqueue order across types. Its own example is
"create the list, then create the todo in it." Every test in the repository sends to one server, so
the guarantee's only evidence is that nothing has contradicted it.

**Multiple invalidation sources.** Decision 038 sets the bar for promoting the inbound seam into
core, and one criterion is more than one origin — because per-source reconnect semantics are where
the design is load-bearing. One server cannot exercise them.

A second domain buys both with one crate.

## The Domain: Users, And Why Not Lists

The natural candidate was `lists`, mirroring decision 036's own wording. **Users is better**, and
the reason is not naturalness.

**It exercises decision 009's actual claim.** The scope key is *caller-composed* — a local identity
the client picks, which core compares for equality and nothing else. The consequence has never been
tested: **a queue can exist before the server has ever heard of you.**

So the story the trial tells is offline signup:

1. Sign up offline. The scope is `user:alice@tenant:local`, chosen locally, with no server involved.
2. Create the user *record* — a write to the user server, `seq` 1.
3. Create todos immediately — writes to the todo server, `seq` 2, 3, 4.
4. Close the application, reopen it, still offline, keep working. Everything survives (D4b).
5. Reconnect. The user record must land before any todo that references it.

**"User" now means two things, and that is the lesson rather than a wart.** The scope is the local
queue identity; the user record is the server's row. Same person, different lifetimes, and the whole
point of decision 009 is that the first does not wait for the second. Every page and doc comment
touching this must say which one it means.

## What Gets Built

### 1. `examples/user-server`

A CRUD server mirroring `examples/todo-server`, and **deliberately not depending on `frontbox`**,
for the reason `scripts/verify.sh` already states about `todo-server`: it writes its wire shapes
from the spec, so a disagreement about the format fails the trial's tests and nothing else. That is
what makes those tests a primary gate rather than a demo, and a second server that shared types
would quietly retire the property for the half it covers.

- `POST /api/v1/sync` — the same batch endpoint, for user mutations.
- `GET /api/v1/users` — every user. This is what makes another user's todos visible.
- `GET /api/v1/users/{id}` — one user, for the todo server's reference check.
- `GET /api/v1/versions` — `{ "user": "<opaque>" }`, computed from current state.

No auth. Decision 004 already keeps credentials out of durable rows, and a trial that spent its
budget on plumbing would answer a question nobody asked. Recorded as an exclusion so it reads as a
choice.

**A `docker-compose.yml` service beside the other two**, because the convergence observation needs
two clients against one pair of servers and the existing compose file is how this project already
runs that. Named here so the verification section below is not asking for something no step
produces.

### 2. `todo-server` gains an owner

A todo carries `user_id`, and the server rejects a todo whose user it cannot find by asking the user
server for it.

**This is a fixture, not an architecture recommendation**, and the plan says so rather than letting
someone read it as guidance. A real service split would not make a synchronous cross-service call to
validate a foreign key. It is here because it is the only thing that makes ordering *observable*:
without it, out-of-order delivery succeeds anyway and the test proves routing rather than order.

`GET /api/v1/versions` here too, returning `{ "todo": "<opaque>" }`.

### 3. `todo-core`: a routing transport and a users entity

- `HttpTransport` becomes a **routing** transport per decision 037: split a batch by destination,
  issue one call per service, merge the verdicts. When one service is unreachable, return the
  verdicts obtained and stay silent about the rest — core already retains an unnamed record with
  `reason: Some("no verdict returned")`, so this needs no protocol change.
- A `user` entity beside `todo`, in the registry, in the row store, with its own version state.
- An `InvalidationSource` **local to this crate**, per decision 038. Two implementations: manual and
  polling. The seam is not exported and does not move into core in this deliverable.
- A poll loop per source, so a dropped source marks only *its* entities stale.

### 4. `todo-app`: identity, a refresh button, and focus gating

- A minimal identity affordance — pick or create the local user, which composes the `ScopeKey`.
- Other users' todos, read-only, and a note in the code that **they are cached into your scope's row
  store**. That is correct — the row store is what this client cached, not what this user owns — and
  it will read as a scope leak to anyone who does not find that sentence.
- **A refresh button** — the second of the two sources this deliverable builds, and the simplest
  possible one. (SSE would be a third; it is out of scope here, so nothing in D4d counts to three.)
- **Focus gating**, which needs no adapter change: `Wake::new()` and `Wake::wake()` are already
  public. A `visibilitychange` listener on web calls `wake()`, and the poll cadence answers "hidden"
  with an interval long enough to be a stop. On iOS and Android the equivalent lifecycle callback is
  **untested and out of scope here** — D4c already recorded that mobile background suspension is
  unexplored, and this plan does not pretend otherwise.
- **A per-entity staleness budget**, stated as "this entity may be up to N seconds out of date"
  rather than "poll every N ms". The proposal argues why: one `GET /api/v1/versions` answers for every
  entity a server holds, so a literal schedule is not satisfiable per entity, while a budget is —
  by polling each source at the tightest budget among its entities and over-delivering for the rest.

## The Observations

Numbered in the D4a style, because that is what made the earlier trials evidence rather than demos.

1. **Offline signup survives a restart.** User record and todos queued, application closed and
   reopened offline, everything still queued and the projection intact.
2. **Cross-service order holds.** On reconnect the user record reaches the user server before any
   todo reaches the todo server. Asserted on the servers' received order, not on the client's queue.
3. **Sabotage: two scopes lose it.** The same flow with one scope per service produces a todo for a
   user the todo server cannot find, and a `404`.

   **The classification is the transport's, not core's.** Decision 019 opens by saying core does not
   classify HTTP responses and will not; what it places on a transport is the obligation that *a
   missing prerequisite is transient* and must not become `Rejected`. So this trial's routing
   transport maps that `404` to **`MutationStatus::Blocked`** — the status whose condition decision
   019 describes in exactly these words, "this write failed only because a predecessor had not
   landed". Not `Unknown`, which is reserved for a response the transport *cannot* confidently
   classify, and this one it can.

   The record is therefore retained with `reason: Some("blocked")`, counted in `counts.blocked`, and
   carries `"blocked"` in decision 033's `last_error` column. **It does not reach the anomaly
   surface**, which only `Unknown` does — so the assertion is on the counts and the record, and the
   retry that follows heals it once the user lands.

   **Without this sabotage, observation 2 proves that one drain worked, not that ordering held.**
4. **A down service does not lose the other's verdicts.** With the **todo** server stopped, user
   mutations still apply and todo mutations are retained with `attempts` incrementing.

   **The direction is forced, and that is worth stating rather than hiding.** The reference check in
   §2 makes the todo server depend on the user server, so stopping the *user* server would make
   todo mutations fail too — for the fixture's reason rather than the transport's — and the
   observation would prove nothing about partial batches. Stopping the todo server is the only side
   with no dependency, so it is the only side that isolates the mechanic. A fixture that constrains
   which experiment you may run is a cost of the fixture, and it belongs next to it.
5. **A cached todo is readable offline for a user you have never fetched.** Decision 032's row store
   holding rows from two domains.
6. **Invalidation runs, at last.** A change made on device A becomes visible on device B after a
   poll, without a reload — the first time `InvalidationRunner::apply` is ever called by an
   application.
7. **The refresh button is a source.** The same convergence, on demand, through the same seam.
8. **A hidden tab stops polling and repolls on focus**, with the wake latched if focus returns
   mid-poll.
9. **One source dropping marks only its entities stale.** Stop the user server; `todo` stays fresh.

## Verification

- `cargo test -p todo-core -p todo-server -p user-server`, with the observations as tests over real
  HTTP, in the shape D4a established.
- `bash scripts/verify.sh` reports ALL GATES PASSED, with `user-server` added to the trial gates.
- Coverage on `-p frontbox` unchanged — **no core file should change in this deliverable.** If one
  does, that is the finding and it belongs in the log, not in a quiet diff.
- `docker compose up -d --build` brings up all three services, and two browser windows against them
  converge without a reload — the manual end of observation 6, which its test asserts headlessly.

## Explicitly Out

- **Anything in `frontbox` or `frontbox-dioxus`.** Decision 038 is the reason; if the trial forces a
  change, stop and record it rather than making it.
- **SSE.** Polling first, for the level-triggered reasons in the proposal. SSE is the second stream
  implementation and it arrives with the server-outbox question attached.
- **A server-side invalidation outbox.** It only becomes necessary with SSE. Recorded in the
  proposal so the ordering of the two is not re-argued.
- **Auth**, per above.
- **The duplicate-a-todo operation.** It tests nothing the create does not, since a copy targets
  you and depends only on your own user record. It *would* test one novel thing — a write whose body
  comes out of the cached row store while offline — and that is worth its own observation later.
- **Mobile lifecycle gating.** Named, not built.

## Outcome

**Built 2026-08-31.** `examples/user-server` exists, the routing transport routes, the invalidation
runtime has its first consumer, and **ten observations pass over real HTTP against two servers**
(`examples/todo-core/tests/multi_domain/main.rs`). All 31 gates green.

**Amended 2026-09-02.** The suite now stands at **fourteen** observations against **35** gates.
Observations 11-13 arrived with decision 039's server-side delete cascade, after this plan was
written; observation 14 came out of a review pass and is described under `### Observation 14` below.
The counts above are left as they were on the day rather than quietly restated, because a plan's
Outcome is a record of what shipped under it.

**No `src/` or `crates/*/src/` file changed**, which was this deliverable's stated proof and is a
fact about the diff rather than a judgement: everything is under `examples/`, plus the workspace
member list, `scripts/verify.sh` and `docker-compose.yml`.

### What the two paid-for guarantees turned out to be worth

**Cross-service order holds** (observation 2) and **the sabotage shows it being lost** (observation
3). One scope: the user record reaches the user service before any todo reaches the todo service,
asserted on the servers' state rather than on the client's queue. One scope per service: the todo
arrives at a server that has never heard of its user, and the transport maps the `404` to
`MutationStatus::Blocked` — retained, `counts.blocked`, no anomaly, no dead letter — and it heals
on the retry after the user record lands. Decision 036's refusal to partition now has evidence
rather than an absence of contradiction.

**Per-source invalidation semantics hold** (observation 8): the user service stops answering, its
entity is marked stale, and the todo entity is left alone. That observation is unwritable with one
entity, because "this source's entities" and "every entity" are then the same set.

### Findings

**1. A partial batch burns the retention bound twice per drain.** Observation 4 expected
`attempts == 1` and got 2. A drain is a loop that stops on the first pass making *no progress*
(decision 029); the user record draining **is** progress, so the loop runs a second pass, re-sends
the todo to the still-silent service, and spends a second attempt. So decision 037's routing and
029's termination rule interact: **a client whose services fail independently reaches decision 017's
bound in half the drains a single-service client would.** Neither page anticipated it, and only a
two-service client can produce it. Not a defect — the bound exists to terminate a wedged head — but
it is a number a caller setting that bound needs to know.

**2. The seam cannot be `dyn`, and that is decision 038's problem more than this trial's.**
`Vec<Box<dyn InvalidationSource>>` does not compile: `poll` is an `async fn` in a trait and an
`async fn` in a trait is not dyn-compatible. Core has this constraint everywhere — `SyncTransport`,
`OutboxStore` and `CacheVersionStore` are all `#[allow(async_fn_in_trait)]` and all used statically
— and it has never mattered, because an application has **one** of each. **An invalidation source is
the first seam where one application plainly wants several at once, of different types.** The trial
enumerates them (`invalidation::Source`), which works for two and does not generalise. A core
`InvalidationSource` would have to answer a question no existing core trait has faced: box the
future and take an allocation per poll, or make the set generic and fix it at compile time.

**3. The refresh button is not a source.** This plan called it "the second of the two sources this
deliverable builds". Trying to build it as one is what showed why: **a button has no versions to
deliver.** A source answers *what changed*; the only honest answer a button has is *go and look*,
which is a trigger. So the two implementations are `ManualSource` (push-shaped) and
`VersionPollSource` (pull-shaped) — which is what decision 038's bar actually wanted — and
`TodoApp::refresh_now` drives the same seam from outside. The observation survives intact: pressing
refresh converges through exactly the path the cadence uses, with no second route to keep in step.

**4. "The same URL for both services" is not a valid single-server configuration.** `Config.user_url`
had to become an `Option`. Pointing both at one host makes a one-service deployment look like a
two-service one whose user endpoints happen to `404`, and the only way to keep such a client working
is to treat a `404` from a read as normal — which is precisely the failure
`examples/todo-core/src/transport/mod.rs` documents at length, where a wrong base URL presents as a
healthy client with no data. `None` says the honest thing: there is no user service, so nothing
hydrates user records and no source polls for them.

**5. A versions endpoint cannot be routed by path.** Both services answer `GET /api/v1/versions`, so
the routing table cannot place it. That is not a hole in decision 037: the table exists to place
*mutations*, whose paths are domain-specific by construction, and a versions endpoint is per-service
instead. A source therefore carries its base URL, which is the honest shape — **a source speaks for
one origin**, and that is the same fact per-source reconnect semantics rest on.

**6. A client's first poll marks everything stale.** An unknown version differs from the server's,
and a differing identity means refetch (decision 021). Correct, and a trap: observation 8 passed for
the wrong reason until a baseline poll and two `mark_fresh` calls were put in front of it.

### Decision 038's Promotion Bar, Scored

Two of four closed, and the other two answered rather than left open:

| Criterion | Outcome |
| --- | --- |
| 1. Three sources against one seam | **Not closed, as expected.** Two built; a stream is the third and D4d could not close it alone. Finding 2 is what the attempt turned up instead. |
| 2. More than one origin | **Closed.** Observation 8. |
| 3. The resume-cursor question | **Answered: no cursor needed.** Reconnect-and-reconcile against a versions endpoint is one `GET`, a comparison, and done — `VersionPollSource` keeps its last-seen map in memory and loses nothing useful on restart, because the durable `(version, stale)` pair core already owns is the state that mattered. **This weakens the strongest argument for core owning the inbound path**, which was that a cursor would be durable per-scope state and core holds all of that. |
| 4. A cadence policy that survives contact | **Answered: `SyncCadence` did not generalise.** The invalidation loop is its own `use_future` over a `Sleeper` (`examples/todo-app/src/session.rs`). A drain is one call returning one report; an invalidation round is several sources with per-source failure, and `SyncStep`/`CountsStep` have nowhere to put that. |

**The recommendation this produces is to not promote yet**, and for a sharper reason than "wait for
SSE": criterion 3 removed the argument that made core the natural owner, and finding 2 named a cost
core has never had to pay. Both point the same way — the seam is doing fine where it is.


### Observation 14

**A fixture switch that did not cover the read another service makes.** `user-server`'s `AppState::down`
documents itself as failing *every* read. It failed two of three: `list` and `versions` go through a
shared helper and `GET /api/v1/users/{id}` had its own query, so the one read another **service**
makes was exempt.

That made a branch of this trial's own design unreachable. `todo-server` splits a failed reference
check into `Missing::NoSuchUser` and `Missing::Unavailable` specifically so a user-service outage
cannot present to a client as a durable ordering failure — a `404` becomes `MutationStatus::Blocked`
and heals on retry (observation 3), a `503` leaves the batch alone. With the read exempt, no test
could produce the second condition, and the distinction the error type exists to draw had no
evidence behind it.

The switch is now a function every read passes through, which is what makes "every read" a claim a
reader can check by looking at call sites. Observation 14 is the condition it unlocked: a todo
enqueued while the user service is down is **retained, not dead-lettered**, the outage is **named**
in `last_send_failure` rather than presenting as an ordinary quiet pass, and the todo lands on the
first sync after the service answers again. Removing the one added line makes it fail — the todo
drains straight through to a server whose reference check silently succeeded.

**It also makes observation 4's stated reason true.** That observation explains at length why it
stops the *todo* server and not the user one: "stopping the user server would make todo mutations
fail too — for the fixture's reason rather than the transport's." Under the exempt read it would
not have. The paragraph was describing a dependency the fixture did not actually enforce, which is
a more interesting kind of documentation error than a wrong sentence: it was right about the design
and wrong about the code, and only a test that tried to use the dependency could tell them apart.

## Explicitly Not Built

- **SSE**, per the plan above. Polling first, and the server-outbox question rides with it.
- **Mobile lifecycle gating.** Named in the plan and still named: D4c recorded that background
  suspension on iOS and Android is unexplored, and `session.rs` says so where the web gating is.
- **The duplicate-a-todo operation.** Unchanged from the plan's reasoning.
- **Auth.** Unchanged.
- ~~**A user delete.**~~ **Built 2026-09-01**, once the referential question this list was avoiding
  had an answer: the user service empties a user of todos before removing them, and applies nothing
  if it cannot. See `wiki/decisions/039-user-delete-cascades-server-side.decision.md`. It turned out
  to be the first flow in this trial that exercises decision 036's ordering guarantee
  *destructively*, which is more than the exclusion expected to be giving up.

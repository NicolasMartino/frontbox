# An Unknown Entity Name Is Ignored But Reported

Document Class: Decision
Status: Accepted; implemented 2026-08-27
Date: 2026-08-27
Category: Cache Versioning
Scope: What the invalidation runtime does with an event naming an entity the caller's registry does not know, and how many event shapes frontbox models.
Sources: `raw/initial/2026-08-25T083750Z/sources/listener.rs`, `raw/initial/2026-08-25T083750Z/sources/frontend/sse.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/cache.rs`
Related: `wiki/decisions/007-generic-entity-key-registry.decision.md`, `wiki/decisions/012-unknown-mutation-status.decision.md`, `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/plans/d2-cache-invalidation.plan.md`, `wiki/specs/source-frontend-cache-architecture.spec.md`

## Decision

An invalidation event naming an entity the registry cannot parse is **ignored for cache purposes and
reported to the caller**. It marks nothing stale, resets nothing, and is not an error.

The report is part of the return value, not a log line. The exact shape belongs to the D2 plan, but
the obligation is that applying a batch of invalidation events yields the unknown names alongside
what was marked stale and what was reset.

**One event shape.** frontbox models a single invalidation event — one entity name, one version. A
server speaking a different shape is served by its transport mapping into that, per decision 010.

**No policy knob.** Decision 007 suggested apps should choose whether unknown entities are "ignored,
reported, or treated as protocol errors". This decision declines the choice and is an amendment to
that consequence: there is one behaviour, and it gives the application what it needs to implement
any of those three itself.

## Why

**Ignoring is correct here, which is not true of an unknown status.** Decision 012 retains an
unknown `MutationStatus` because a real record is stuck behind it — refusing to guess costs a
resend, and guessing costs a lost or wrongly buried write. An unknown *entity* blocks nothing. The
registry defines what the application models; a name outside it names data this client does not
hold. There is no local state to mark stale, and nothing to refetch. Ignoring is not a concession,
it is the only meaningful action.

So the two decisions share a principle and diverge on disposition, deliberately: **do not guess, and
do report.** What differs is what "not guessing" costs, and here it costs nothing.

**Erroring would break every client during a rolling deploy.** A server that adds a sixth entity
type starts emitting its name immediately, to clients built before it existed. If that were a
protocol error, every client would fail on an event about data it does not use. This is the same
argument decision 012 makes about failing a whole batch response for one unrecognised status, and it
points the same way.

**Silence is the actual defect in the source, not the ignoring.** The source gets the disposition
right and the signal wrong, twice:

- `listener.rs:465` parses the name and, failing, falls through to `crate::log!("[SSE] Unknown
  entity type: {}", event.entity)` at `listener.rs:505`. A log line is not something an application
  can branch on.
- The reconnect path is worse. `listener.rs:414` is
  `.filter_map(|(k, v)| EntityType::parse(&k).map(|e| (e, v)))` — unknown names vanish from the
  server version map with no log at all, so a client whose registry has drifted out of date
  reconciles against a silently truncated view and believes it succeeded.

The second is the one worth designing against. It is not a missing warning; it is a reconciliation
that reports success while having skipped part of its input.

**The distinction the report enables is real.** "The server invalidated something I do not model"
and "nothing happened" are different facts. The first means the client build is behind the server's
vocabulary — benign during a deploy, a misconfiguration if it persists. An application that wants to
alert on it, count it, or refuse to start can, and one that wants the source's behaviour ignores the
field. That is why no knob is needed: the reporting behaviour subsumes "ignore" and "report", and
"error" is a caller-side branch on the report.

**One event shape, because the second one is dead.** `LegacyInvalidationEvent { entities:
Vec<String>, user_id }` (`frontend/sse.rs:26-34`) is documented as a migration shim — *"Used during
migration period when some code still expects `entities` array"* — and is **declared and never used
anywhere in the copied corpus**. There is no compatibility obligation to a shape nothing sends.
Modelling it would also import a genuine problem: the legacy form carries no version, so fanning it
out would mean marking entities stale with no version to compare, which is exactly the ambiguity
decision 007 flagged around `update_version(entity, None)`.

## Consequences

- **`EntityRegistry::parse` returning `Option` is load-bearing**, and its `None` is a reportable
  outcome rather than a discard. Decision 007's sketch already has the right signature.
- **Reconnect reconciliation must report too**, not only the per-event path. This is the divergence
  from `listener.rs:414` and needs its own conformance case: reconciling against a server map
  containing an unknown name must surface it, and must still reconcile the names it did understand.
- **`mark_all_stale` means "all *registered* entities".** With the registry as the source of "all",
  an unknown entity cannot be marked stale by it, which is consistent: the caller cannot refetch
  what it does not model.
- **Decision 007's "apps decide" consequence is amended**, not discharged. The API specifies the
  behaviour; the application decides what to do with the report.
- ~~**Two conformance cases are owed**~~ **Implemented 2026-08-27** as cases 35 and 36.
- **Implementation refined decision 007's sketch**: `EntityRegistry` uses an associated `Key` type
  rather than a generic parameter, and `parse` returning `None` feeds `InvalidationReport::unknown`
  exactly as this decision requires. A third path appeared that this page did not anticipate —
  reading *stored* state whose name the registry no longer models, which a durable store can hold
  because it outlives the build that wrote it. Same policy applies: skipped, not an error, because
  the application cannot act on data it does not model.

## Revisit If

An application appears that must not proceed at all when its registry is behind the server — a
strict-schema deployment where an unmodelled entity implies the client is too old to be trusted with
any of the data. The report already gives it what it needs to refuse; if that pattern turns out to
be common rather than exotic, the refusal could move into core.

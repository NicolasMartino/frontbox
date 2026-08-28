# Observability Is A Returned Value First; Emission Is Additive And Available

Document Class: Decision
Status: Accepted 2026-08-28; corrected same day — see `## Why Core Emits Nothing Today`
Date: 2026-08-28
Category: Public API Shape
Scope: What frontbox tells a caller about its own operation, through what mechanism, and what a cross-library observability convergence can and cannot ask of it.
Sources: `src/runner.rs`, `src/record.rs`, `Cargo.toml`, `scripts/verify.sh`, `wiki/compatibility/public-dependencies.compat.md`
Related: `wiki/decisions/022-durable-trace-context.decision.md`, `wiki/decisions/001-single-threaded-core.decision.md`, `wiki/decisions/008-mutation-envelope-extensibility.decision.md`, `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/decisions/011-owned-rfc3339-rendering.decision.md`, `wiki/decisions/012-unknown-mutation-status.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/018-single-flight-drain-mode.decision.md`, `wiki/specs/frontbox-runtime.spec.md`

## Decision

**The returned value is the primary surface and stays that way. Emission is additive, is not
blocked by anything in this crate, and RepForge's §5b.6 boundary is accepted.**

Core emits nothing *today*: `Cargo.toml`'s runtime graph is `serde`, `serde_json`, `thiserror`,
`uuid` and nothing else, and no `log` or `tracing` call exists anywhere in `src/`. That is a
statement about what is built, not a prohibition — the first version of this page argued it was a
prohibition and was wrong, which is corrected below.

**RepForge's §5b.6 ask is granted:** a library emits spans and events and never owns the
application's subscriber, formatter, or sink. frontbox has no reason to want the sink and every
reason not to — a wasm client cannot export OTLP without exposing a collector publicly, which
RepForge notes and which makes the application's HTTP path the only sane route out.

Three parts, all of them already built:

- **Diagnostics are returned.** `SyncReport` from `sync_once`, a structured `Error` from a failed
  call, dead letters and quarantine from explicit queries.
- **Correlation is `mutation_id`.** Client-generated, durable on the record, and carried on the wire.
  Under RepForge's `PUT`-with-client-id design it sits *in the request path*, so it is a join key
  across client, gateway, and service for free.
- **Semantic labelling is caller-owned**, through `OperationMeta { name, version }` (decision 008),
  which core stores and never interprets.

## Why Core Emits Nothing Today

**Corrected 2026-08-28.** The first version of this page argued that emission was *structurally*
blocked by two of the crate's own gates. That was wrong on both counts, and the error mattered: it
would have refused RepForge's §5b.6 ask — "emit `tracing` spans and events; let the application
choose the sink" — on grounds that do not exist. Each claim was checked before being withdrawn.

- **The `!Send` gate does not block it.** `scripts/verify.sh`'s `no_send_bound` greps `src/` for
  `+ Send` and `: Send`. A `tracing::info!` call adds neither. `tracing::Subscriber` is
  `Send + Sync + 'static`, but the subscriber belongs to the application and frontbox would never
  name the type.
- **The no-date-library gate does not block it.** Checked against the registry rather than assumed:
  `tracing` depends on `pin-project-lite` and `tracing-core`, and `tracing-core` on `once_cell`.
  None is a date crate, so `date_crate_in_runtime_graph` stays quiet. `tracing-subscriber` is the
  crate that pulls `time`, and a library has no business depending on that.
- **The missing clock does not block it.** A subscriber stamps an event when it is emitted,
  in-process and synchronously, so an event emitted during an offline pass is stamped during the
  offline pass. The earlier claim — that frontbox-emitted events would be dated to the reconnect —
  confused emission with buffering.

What actually survives is a cost, not an impossibility. `tracing` would be a fifth runtime
dependency; it is not public surface, for the same reason `thiserror` is not, so it adds no semver
commitment. Whether it is default-on or feature-gated is a live question, not a settled one.

## The One Limit That Does Survive, And It Is The Interesting One

**A `tracing` span cannot model this crate's most important interval.** The number that matters for
an outbox is enqueue-to-send: how long a write waited before the server saw it. For an offline-first
queue that interval is measured in hours or days and routinely spans process restarts, browser tab
closes, and application upgrades.

A span is an in-memory, in-process object. It cannot stay open across a restart, so the interval
cannot be a span's duration. It has to be *reconstructed* from `created_at` — the caller's injected
`Clock`, written durably on the record — and the wall time at drain.

This is not an argument against emitting. It is an argument that emission alone is insufficient, and
it is the strongest thing in RepForge's document that RepForge does not quite say: their §5.2 asks
for a `traceparent` **stored on the record** rather than generated at drain, on the grounds that
generating it late "would lose the enqueue-to-send interval". That is exactly right, and the reason
is the one above — durable trace context exists precisely because a span is not durable. See
decision 022.

## Why Returned Values Remain The Primary Surface

Emission is additive to this, not a replacement for it.

Push telemetry assumes a live sink, and the mode frontbox exists for is the one where there is no
sink. More concretely: a `SyncReport` is a *value the caller can act on* — branch on
`is_stalled()`, count `retained`, requeue a dead letter. An emitted event is a value the caller can
only observe. The two answer different questions, and the crate's existing answer is richer than
its peers' logging: counts that reconcile against `retained`, an explicit no-progress signal, and
typed anomalies carrying the server's own spelling (decision 012).

## What A Convergence Can Ask For

Written from frontbox's side only. **RepForge's observability requirements were not supplied**, so
the vocabulary questions below are open rather than answered.

### Cheap, and it should be decided now

**Make the report types serializable.** This is the sharpest finding on this page and it is a
one-line change with a real consequence. Today:

| Type | Derives | Serializable |
| --- | --- | --- |
| `OutboxRecord`, `DeadLetterRecord`, `MutationId`, `OperationMeta` | `Serialize, Deserialize` | yes |
| `SyncReport`, `SyncOutcomeCounts`, `SyncPass`, `Anomaly`, `AnomalyKind` | `Debug, Clone, PartialEq` | **no** |

The types a converged pipeline most wants to ship are exactly the ones that cannot leave the process
without being hand-mapped field by field. The durable records serialize because storage required it;
the diagnostics never had a reason until now.

Adding the derive is additive, and free while the crate is `0.0.0` with `publish = false`. It puts
`serde` on those types' semver contract, which the public-dependency note already tracks for every
other public type — so it widens an existing commitment rather than making a new one.

### Already present, at no cost

- **A correlation key** — `mutation_id`, above.
- **A caller-owned operation vocabulary** — `OperationMeta`, which is the existing hook for naming
  what a mutation *is* in whatever taxonomy the three libraries agree on.

### Available, but each costs a decision that is still open

- **Attempt history.** Decision 017's durable counter, accepted and unimplemented. `mutation_id`
  identifies a mutation; it does not identify a *try*. `(mutation_id, attempts)` does.
- **Last-error metadata.** Left open by decision 017 and listed in `wiki/index.md` under Open Work.
  Under a convergence requirement it stops being optional: it is the only proposed field that
  survives a restart and explains *why* a record is still queued. Everything else in the report
  describes one pass and is gone when the process ends.

### Available, and now accepted rather than refused

- **`tracing` emission with a caller-owned sink** (§5b.6). Granted. The open sub-question is whether
  the dependency is default-on or feature-gated, which is a packaging decision and not a large one.
  What emission does *not* remove is the returned report: a caller branches on `is_stalled()` and
  requeues a dead letter, and an emitted event cannot be branched on.

### Not available

- **Enqueue-to-send as a span duration.** See above; it needs durable trace context (decision 022),
  not a span.
- **Cross-scope visibility.** Decision 009 rule 4 means work retained under a scope no store
  currently opens is invisible to every other store. No observability convergence reaches it,
  because there is no live object that can see it. It needs the cross-scope diagnostic that is
  still open, and a converged dashboard would silently under-report without one.

## The Observability Cost Of Single-Flight, Which The Proposal Does Not Price

RepForge's proposal is silent on observability, and `batch_limit = 1` changes it substantially.

At 100, one `SyncReport` describes up to a hundred records and its counts are a genuine aggregate.
At 1, every count is 0 or 1, and the 200-record backlog the proposal's §7 estimates produces **200
reports instead of 2**. Decision 018 records this as a readability note — "a reader of a
single-flight report will find it uninformative in isolation". Under a requirement that three
libraries produce correlatable telemetry it is more than that:

- Event volume rises by the batch-limit factor.
- Per-event content falls to near nothing. `Completed, sent: 1, applied: 1` is not worth shipping.
- **Aggregation moves out of the library and into every caller**, and unless they all do it
  identically the converged dashboards disagree with each other.

The mitigation is not in core, and it lands on a deliverable already on the critical path:
**a drain-until-idle loop is the natural aggregation boundary, and it is where a converged system
should emit — not once per `sync_once`.** Decision 018 already owes D3 that loop for liveness
reasons. This is a second, independent argument for the same thing, which is usually a sign the
loop is in the right place.

## Open Questions This Page Cannot Answer

These need RepForge's actual convergence specification, which was not provided:

- **What the shared event vocabulary is.** `OperationMeta::name` is the hook, but nothing says what
  goes in it.
- **Whether the target is OpenTelemetry semantics**, and if so whether `mutation_id` is a trace id,
  a span attribute, or baggage. It is a *mutation* identifier, not a request identifier — one
  mutation spans many attempts across many sessions — so it maps to none of the three cleanly.
- **Whether kafkaman pushes or returns.** If kafkaman emits and frontbox returns, the adapter
  bridges two shapes, and that asymmetry should be deliberate rather than discovered.
- **Whose clock stamps the shared format.** frontbox has none; the client's `Clock` and the
  collector's receive time give materially different answers for offline work, and the difference is
  the whole point of the queue.

## Consequences

- **If `tracing` is adopted, `scripts/verify.sh` needs no new gate.** The date-library check is a
  pattern match rather than an allowlist, and `tracing`'s transitive set —`pin-project-lite`,
  `tracing-core`, `once_cell` — trips none of it. Verified against the vendored manifests rather
  than assumed, because the first version of this page asserted the opposite from memory.
- **D3 is the observability adapter**, not just the Dioxus binding. Its drain loop is the
  aggregation boundary and its host supplies the clock.
- **D4 is where the convergence is tested**, since it is the first place frontbox, the services, and
  whatever kafkaman does run together.
- **The serialization question should be settled before D5**, not because D5 forces it, but because
  it is free now and every week of delay makes it likelier that a caller has already written the
  hand-mapping it would remove.

## Revisit If

The converged format requires frontbox to own an exporter or a collector endpoint rather than just
emit. That is the boundary §5b.6 draws and this page accepts, and crossing it would put network and
buffering concerns inside a crate whose entire premise is that the network is unreliable and the
buffering is the caller's queue.

Also revisit if `tracing`'s cost turns out to be real rather than nominal on the wasm target —
binary size is the plausible complaint, and it is measurable rather than arguable.

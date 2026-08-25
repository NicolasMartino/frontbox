# Prior-Art Survey Plan

Document Class: Plan
Status: Completed
Date: 2026-08-25
Category: Research
Scope: Gather external prior art before stabilizing frontbox public APIs.
Sources: `wiki/proposals/extraction-boundary.proposal.md`, `wiki/references/prior-art-survey.reference.md`, `wiki/log.md`
Related: `wiki/proposals/extraction-boundary.proposal.md`, `wiki/references/prior-art-survey.reference.md`, `wiki/roadmaps/extraction.roadmap.md`

## Deliverable

This plan executed **D0a** in `wiki/roadmaps/extraction.roadmap.md`. D1 implementation was gated
on it; D1 planning was not. The survey is complete as of 2026-08-25.

## Objective

Before frontbox stabilizes public APIs, compare the RepForge-derived design against established
offline-first and local-first systems. The goal is not to copy another project; it is to catch
known design traps before they become frontbox's public contract.

Every conclusion in this wiki so far derives from one codebase. That is the single largest
unexamined risk in the project: it is still unknown whether server-authoritative HTTP-envelope
replay is a good design or simply what RepForge found convenient to build.

## Systems To Review

- Replicache and Zero
- PowerSync
- ElectricSQL
- RxDB
- WatermelonDB
- PouchDB and CouchDB sync
- Automerge and Yjs

## Questions

1. How do mature systems represent pending local writes?
2. Do they replay HTTP envelopes, domain mutations, database changes, or CRDT operations?
3. How do they distinguish rejected, blocked, conflict, duplicate, and pending outcomes?
4. What happens to malformed local durable data?
5. How do they prevent stale cache fallback from leaking data across users, tenants, languages, or
   query scopes?
6. How do they model entity/key registries, schema evolution, and unknown remote entity names?
7. What retry/backoff and batching policies are considered safe defaults?
8. Which public APIs have caused migration or compatibility pain?
9. How do they stop one unresolvable write from blocking the queue behind it, and how do they make a
   stalled queue visible to the application? This is the open risk in
   `wiki/decisions/005-mutation-outcome-policy.decision.md`: frontbox retains `Pending` and
   `Blocked` work indefinitely with no attempt counter, so prior art should say whether aging,
   skip-past, or operator escalation is the usual answer.

## Deliverables

- A reference page under `wiki/references/` summarizing each system and citing source material.
- A comparison table covering write model, conflict model, cache invalidation, batching, retry,
  storage, and framework integration.
- A short proposal update identifying any frontbox API changes required before D1 implementation.

## Outcome

Completed in `wiki/references/prior-art-survey.reference.md` and promoted into
`wiki/proposals/extraction-boundary.proposal.md`.

The survey did not replace frontbox's extraction boundary, but it raised follow-up questions before
public API freeze:

- optional caller-owned operation metadata,
- explicit local namespace/scope identity,
- monotonic enqueue sequence before durable backends,
- attempt/error aging for retained work,
- and durable storage format versioning.

`wiki/plans/d1-core-cache-runtime.plan.md` `## D0a Follow-Up` splits these by when they must be
settled: the first two can be foreclosed by a public D1 API, the rest can wait for durable backends.

### Limitation Of This Plan's System List

The system list below was drawn from local-first and offline-first sync engines, eight of which
replicate *state*. frontbox replays *HTTP commands*. Verification on 2026-08-26 found that the
omission produced a wrong headline finding, and added a fifth source note covering Workbox
Background Sync, Redux Offline, TanStack Query offline mutations, and Amplify DataStore. A future
survey should choose its cohort from the shape of the artifact under test, not from the topic
label. The 2024-2026 local-first cohort (Triplit, LiveStore, TanStack DB, InstantDB, Jazz) remains
unsurveyed and is recorded as a known gap.

## Out Of Scope

- Benchmarking.
- Building prototypes.
- Choosing a replacement architecture without evidence.
- Changing accepted decisions unless the survey finds direct contradictory evidence.

## Verification

The survey is complete when every listed system has at least one primary source cited, the
comparison table exists, and the extraction boundary proposal has been updated with the outcome.

Checked 2026-08-25, re-checked 2026-08-26: met. The re-check found two cited URLs dead and one
claim attributed to a page that did not contain it; both are fixed and recorded in
`raw/research/2026-08-25-prior-art-survey/manifest.md` `## Verification Pass`. Restating the
criterion rather than asserting its own satisfaction keeps it testable on the next lint pass.

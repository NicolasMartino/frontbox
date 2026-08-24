# Prior-Art Survey Plan

Document Class: Plan
Status: Active
Date: 2026-08-25
Category: Research
Scope: Gather external prior art before stabilizing frontbox public APIs.
Sources: `wiki/proposals/extraction-boundary.proposal.md`, `wiki/log.md`
Related: `wiki/proposals/extraction-boundary.proposal.md`, `wiki/roadmaps/extraction.roadmap.md`

## Deliverable

This plan executes **D0a** in `wiki/roadmaps/extraction.roadmap.md`. D1 implementation depends on
it; D1 planning does not.

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

## Out Of Scope

- Benchmarking.
- Building prototypes.
- Choosing a replacement architecture without evidence.
- Changing accepted decisions unless the survey finds direct contradictory evidence.

## Verification

The survey is complete when every listed system has at least one primary source cited, the
comparison table exists, and the extraction boundary proposal has been updated with the outcome.

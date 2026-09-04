# Core Does Not Learn About Service Origins

Document Class: Decision
Status: Accepted 2026-08-30; no column, no code
Date: 2026-08-30
Category: Protocol
Scope: Whether a mutation records which backend service it targets, settled at the level D5 needs — whether it is a durable column.
Sources: `src/record/mod.rs`, `src/transport.rs`
Related: `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/decisions/004-transport-auth-and-offline.decision.md`, `wiki/references/open-decisions.reference.md`

## Decision

**No column, and no core concept.** A mutation's target service is not something core models.

D5 needed only the *shape* of this answer — whether a durable row grows a field — and the answer is
that it does not. The full question of how a multi-service client routes stays open and costs
nothing to leave open, because nothing durable depends on it.

## Why

**The path already carries it.** `MutationIntent` is `method`, `path`, `body`. An application whose
writes span two services encodes that in the path, or in `OperationMeta`, which exists precisely for
caller-owned labels core does not interpret. A dedicated `origin` field would be a second, blessed
way to say the same thing, and core would then have to decide which wins.

**A service is a transport concern, not a record one.** `SyncTransport` is the seam that knows where
a batch goes. Decision 004 already keeps credentials out of durable rows for a related reason: what
varies per-send belongs to the send, not to the row that outlives it. A durable `origin` would be
routing pinned into storage, and a client whose service topology changed would carry stale routing
on disk.

**One batch goes to one transport.** Nothing in the drain splits a batch by destination, and adding
a field that implies it can would be a promise the runner does not keep.

## Consequences

- **The outbox column set is closed on this question.** D5's backends need no `origin`.
- **If multi-service routing is ever wanted**, it arrives as multiple runners over multiple scopes,
  or as a caller-side dispatch above the transport — both of which work today with no core change.
- **The register entry narrows** from "does core learn about service origins" to a routing question
  with no storage clock on it.

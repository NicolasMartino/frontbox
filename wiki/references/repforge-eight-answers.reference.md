# RepForge's Answers To The Eight Questions

Document Class: Reference
Status: Recorded from conversation; not filed in `raw/`
Date: 2026-08-30
Category: External Input
Scope: RepForge's reply of 2026-08-30, which claims verified against this repository, and the two findings the verification produced.
Sources: RepForge architecture, "Answers To frontbox's Eight Questions", received in conversation 2026-08-30.
Related: `wiki/references/open-questions.reference.md`, `wiki/references/repforge-section-c-answers.reference.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/022-durable-trace-context.decision.md`, `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/references/repforge-questions-from-frontbox.reference.md`

## Provenance

Same as the earlier RepForge pages: received in conversation, not filed in `raw/`. Every claim they
make about **their** repository is recorded as a claim with an address and has not been checked from
here. Every claim they make about **this** repository was checked, and the results are below.

## What They Answered

| # | Answer |
| --- | --- |
| Q1 | Parent-first is now a written invariant. Three of their pages said the sequence *guaranteed* causal order; corrected to *preserves* |
| Q2 | The gate was a six-name denylist that `chrono` passed, half of it aimed at a non-member crate, with no CI. Replaced with a feature-pinned allowlist, a negative test naming `chrono`, a `verify.sh`, and two resolved-graph greps |
| Q3 | Sent — and writing it revealed they had never specified the **success** response. Now `200 { "outcome", "hash" }` for create and update alike |
| Q4 | `CONFLICT` on a `412` from a failed `If-Match`, pinned by a test. Plus `ALREADY_EXISTS` for a failed `If-None-Match: *`, which is a different condition |
| Q5 | Not as written. The discriminator is now `retry`-and-`source` presence, with `source` a new required field |
| Q6 | **8**, derived from a backoff schedule (1 s base, ×2, cap 60 s, full jitter) that did not exist until the question was asked |
| Q7 | Enqueue — no disagreement. But **the ask is re-priced**: decision 023 removed the column it was reusing, so it is now a fourth durable column |
| Q8 | All three requests accepted and built, with a **third** cause kind — `watchdog` — beside `rejected` and `retention_exhausted` |

They report that the eight questions produced six defects in their own material, and that the `401`
finding below came from chasing Q5 rather than from Q5 itself.

## Claims About This Repository, Checked

| Their claim | Verdict |
| --- | --- |
| `MutationStatus` is PascalCase on the wire, safe to pin a test against | **Correct.** `"Applied"`, `"Duplicate"`, `"Rejected"`, `"Blocked"`, `"Pending"` |
| Decision 023 declined the row-contents column, so the base hash has nowhere to live | **Correct**, and the reason they cite is the reason 023 gives |
| Decision 022's shape — caller-supplied, core-stored, never parsed, `#[serde(skip)]` | **Correct**, and it is what was built |
| Decision 017 does not increment on offline or transport-failure passes | **Correct** — `src/runner/mod.rs` returns before `apply_outcomes` on both |
| Decision 018 makes drain cadence part of the contract's documentation | **Correct** |
| Neither repository has a git remote | **Correct** for this one |
| "RepForge D2 is the subject of your D4 migration trial" | **Wrong premise.** D4 targets the *exercise* flow — always-enqueue writes and an optimistic favourite projection — plus reproducing the preferences direct-dispatch pattern in application code. Their conclusion probably survives, because a favourite is also flat, but not for the reason given |
| The column set is `seq`, `attempts`, `trace_context` | **Field name is `traceparent`**, not `trace_context` |

## Their Finding About Decision 017: Correct, And Acted On

> `attempts` counts verdicts received, and both the offline path and the transport-failure path
> deliberately do not increment — correctly, for the reasons the decision gives. But that means a
> head receiving *no* verdicts never advances the count.

Verified against `src/runner/mod.rs`: a transport error returns before `apply_outcomes`, so nothing
increments and the bound never fires. Under their own Q3 transport table a persistent gateway `502`
is a transport error, so the scenario is reachable in their deployment specifically.

Decision 017 now carries `## The Bound's Liveness Guarantee Is Conditional`. Two qualifications went
in with the correction:

- **The behaviour was right and only the claim was too broad.** Incrementing on a pass that produced
  no verdict is exactly what 017 refuses for offline work, and for the identical reason. An outage
  is not a stuck record.
- **The condition is not silent.** `sync_once` returns `Err(Error::Transport)`, not a `SyncReport`,
  so a caller does not even have to inspect `is_stalled()`. A wall-clock watchdog beside the bound
  is a reasonable application answer and is theirs rather than core's, because core has no clock.

## Two Findings Their Reply Produces And Does Not Notice

### 1. Making `401` transient contradicts the derivation of the bound

Their Q6 argues 8 is safe because an outage cannot consume it:

> only a *service verdict* of `retry: transient` produces a retaining status. Everything without an
> envelope — offline, reset, timeout, gateway `5xx` — is surfaced as a transport error, which by
> your 017 does not increment. An outage therefore cannot consume the bound.

Their Q5 then makes `401` **transient**, and says services rather than the gateway author it — so a
`401` carries an envelope. By their own Q3 table, an envelope with `retry: transient` yields a
retaining status, which under 017 as built **does** increment.

So the property that makes 8 safe for outages does not hold for auth. With a five-minute token
lifetime, a 1 s base doubling to a 60 s cap, and a bound of 8 spanning roughly 90 s expected, **a
token refresh that fails for about ninety seconds dead-letters every queued mutation** — for a
condition that clears without the user doing anything, which is the definition they used to
reclassify `401` in the first place.

Both changes are individually right. Together they need either a bound derived with auth in mind, or
`401` treated as a transport-level condition rather than a service verdict.

### 2. Their third cause kind cannot be reconstructed from a frontbox dead letter

`watchdog` and `retention_exhausted` both arrive as `Disposition::DeadLetter { error: None }` and
produce identical `DeadLetterRecord` values. `apply_outcomes` is public, so an application
dead-lettering for any reason of its own is indistinguishable too. `attempts == bound` is a
heuristic, not a discriminator.

This is not a defect in their design — their client knows which path fired, from its own state — but
it is worth stating: **frontbox's record carries the fact of a dead letter without a server verdict,
not the reason for it.** They took our own argument about `error: None` one case further than we
made it and reached the limit of the type while doing so.

Whether `DeadLetterRecord` should carry a reason is a new open decision, and it is the same shape as
the last-error question already in the decision register.

## What They Raised That Blocks Us

**Neither repository has a git remote**, and their layout decision specifies revision-pinned git
dependencies rather than path dependencies. D4 needs this resolved and it is not a code problem.

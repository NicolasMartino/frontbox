# A Retained Record Says Why It Is Still Queued

Document Class: Decision
Status: Accepted 2026-08-30; implemented in core and both durable backends 2026-08-30
Date: 2026-08-30
Category: Storage Contract
Scope: Whether the outbox record carries the last reason a verdict left it queued, in what form, and who bounds it.
Sources: `src/record/mod.rs`, `src/store/mod.rs`, `src/runner/mod.rs`
Related: `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/027-dead-letter-reason.decision.md`, `wiki/decisions/026-replayable-preconditions.decision.md`, `wiki/references/open-decisions.reference.md`

## Decision

**`OutboxRecord` gains `last_error: Option<String>`** — the server's own words for why the most
recent verdict left this record queued. Opaque, bounded, never parsed.

- **`Disposition::Retain` gains `reason: Option<String>`.** The store writes it to `last_error`
  in the same step that increments `attempts`, which is what already makes `Retain` a write rather
  than a no-op (decision 017).
- **Core bounds it at `LAST_ERROR_MAX` (512 bytes)** and the store truncates. Stated as an
  obligation with a conformance case, in the shape decisions 024, 025 and 031 established — a
  pathological server must not be able to grow a durable row without limit.
- **Overwritten, not accumulated.** One slot, latest wins. A history is a second table and nothing
  has asked for one.

## Why

**Decision 017 created the gap.** A record retained eight times and then dead-lettered at the bound
arrives in the dead-letter store with `DeadLetterReason::RetentionBound` — accurate, and it says
nothing about *why* eight attempts failed. Decision 027 fixed the analogous silence for refusals by
making the dead letter say which kind it was; this is the same fix one level earlier. The bound
tells you the record gave up. `last_error` tells you what it was up against.

**It is transport-shaped because that is all core ever has.** `Blocked`, `Pending` and
`UnknownStatus(s)` are what a retained record's verdict can be, and only the last is a string core
did not choose. Storing the server's spelling means an operator reading the row sees what the
server actually said rather than this crate's summary of it.

**Opaque for the same reason `precondition` is** (decision 026): core cannot know whether the string
is a status, a message, or a correlation id, and a field it parsed would be a field it could be
wrong about.

## Alternatives Rejected

- **A structured `RetainReason` enum.** Would force core to enumerate a server's vocabulary, and the
  one case that matters — `UnknownStatus` — is precisely the one core has no variant for.
- **Accumulate a history.** A second table, unbounded by default, for a question nobody has asked.
  If retention forensics turn out to need it, the column is where it starts.
- **Leave it out.** The status quo, and it is what makes a dead letter at the bound unactionable.

## Consequences

- **One more durable column** on the outbox, and D5's backends inherit it before writing a row.
- **`Disposition::Retain` changes from a unit variant to a struct variant.** Breaking, and free
  while `publish = false`. Nine call sites in-tree.
- **A conformance case is owed**: a retained record carries the reason the verdict gave, and an
  over-long reason is truncated rather than stored whole.

## Where The Field Stops

**It is not carried onto the dead letter.** `DeadLetterRecord::from_record` copies `op`,
`traceparent`, `precondition` and `attempts` across the transition and drops `last_error`.

That is currently an omission rather than a position, and it is register entry 23. For a
`RetentionBound` letter it is the one field that says what the record was up against — `attempts`
says how many times it tried, and nothing says what it got back. For a `Rejected` letter it is
redundant, because `DeadLetterReason::Rejected` already carries the server's payload. Which is
exactly the shape that lets a field get quietly dropped: useless for one reason, load-bearing for
another.

Recorded here rather than fixed, because adding a public field is a design change under `AGENTS.md`.

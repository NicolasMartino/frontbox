# Local Store Identity Is A Required Opaque Scope Key

Document Class: Decision
Status: Accepted
Date: 2026-08-26
Category: Public API Shape
Scope: What identifies a frontbox store instance, and what prevents one principal's queued mutations from replaying under another principal's session.
Sources: `raw/initial/2026-08-25T083750Z/sources/persistence/native.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/web.rs`, `raw/research/2026-08-25-prior-art-survey/sources/01-replicache-zero.md`, `raw/research/2026-08-25-prior-art-survey/sources/02-powersync-electric.md`, `raw/research/2026-08-25-prior-art-survey/sources/04-pouchdb-crdt.md`
Related: `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/decisions/004-transport-auth-and-offline.decision.md`, `wiki/decisions/005-mutation-outcome-policy.decision.md`, `wiki/decisions/008-mutation-envelope-extensibility.decision.md`, `wiki/specs/source-frontend-cache-architecture.spec.md`, `wiki/references/prior-art-survey.reference.md`

## Decision

Every store is opened with a required, non-empty, caller-composed scope key. Core compares keys for
equality and nothing else.

```rust
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ScopeKey(String);

impl ScopeKey {
    /// Rejects an empty key with `Error::InvalidScopeKey`.
    pub fn new(key: impl Into<String>) -> Result<Self, Error>;
    pub fn as_str(&self) -> &str;
}
```

Five binding rules:

1. **No unscoped constructor.** There is no `open()` or `init_db()` equivalent with a default or
   shared identity. The scope key is a constructor parameter, not a setter and not an `Option`.
2. **Records are stamped.** Every outbox, dead-letter, and quarantine record carries the scope key
   of the store that wrote it.
3. **Reads verify.** A record whose scope key differs from the opening store's key is never
   returned by `pending_batch`, never included in a `MutationBatchRequest`, and never counted by
   `pending_count`, `DeadLetterStore::count`, or `QuarantineStore::count`.
4. **Mismatch retains.** Cross-scope records are left in place, undeleted and unmodified. Opening
   a store with that key again makes them visible and syncable.
5. **Core does not interpret the key.** Composing principal, tenant, schema/version, and read
   scope into one key is the caller's job. Core never parses it, never orders by it, never derives
   authorization from it.

## Why

**The source has this defect today, and the spec does not record it.** Both backends ship two
constructors — one isolated, one shared:

| Backend | Isolated | Legacy shared |
| --- | --- | --- |
| Native | `open_for_user(user_id)` -> `{user_id}.db` (`persistence/native.rs:86`) | `open()` -> `app.db` (`persistence/native.rs:70`) |
| Web | `init_db_for_user(user_id)` (`persistence/web.rs:89`) | `init_db()` -> `repforge_app` (`persistence/web.rs:81`) |

The isolated path states its own intent: "Each user gets their own database file to ensure data
isolation." The shared path is a cross-principal leak waiting for a call site. Log out with pending
mutations, log in as a different user, and any code path that opened the shared database replays
user A's writes under user B's token. Keeping a scoped constructor alongside an unscoped one means
the safety property holds only where every call site remembered — which is why rule 1 removes the
choice rather than documenting it.

**Transport auth does not cover this.** Decision 004 evaluates credentials freshly per send, which
is correct and insufficient: a fresh, valid token for user B applied to user A's record produces an
authorized request performing the wrong write. Enforcement has to happen where records are read,
not where they are sent.

**Scope is more than a user id.** The source keys on `user_id` alone. The survey found four
dimensions treated as boundaries elsewhere: principal (Replicache's per-user `name`), tenant
(PowerSync's authenticated stream parameters), schema/version (RxDB's mandatory migration
strategies), and query/read scope (CouchDB's database-level security, where filters and selectors
are selection tools rather than isolation). A caller-composed key can carry all four; a
`user_id` parameter cannot.

**Opaque rather than structured, because core is framework-neutral.** A core that hardcodes
`principal` and `tenant` fields will be wrong for the first caller whose identity model differs.
Enforcement — the part that converts a silent leak into a typed error — needs only equality, not
comprehension of the dimensions.

## Consequences

- **Backends must derive physical storage identity injectively.** The source sanitizes by
  character replacement (`persistence/native.rs:65`, `persistence/web.rs:33`), which is safe for
  the UUIDs it was written for and unsafe for arbitrary keys: `tenant/1` and `tenant_1` both
  collapse to `tenant_1`, silently merging two scopes into one database. A caller-composed key is
  arbitrary, so backends must encode reversibly or hash rather than replace characters. This is a
  D5 obligation and a conformance test.
- `pending_count` becomes scope-filtered, matching the existing rule that quarantined rows stay out
  of pending totals. A count is always "pending for this scope".
- **Abandoned scopes are invisible.** Rule 4 means work retained under a scope nobody opens cannot
  be seen through any other store, so D1's no-progress signal will never report it. This is the
  accepted cost of not destroying offline writes on user switch. A cross-scope diagnostic that
  enumerates known scopes may be needed at D5; it is not in D1.
- Records gain a field, which is non-breaking only because decision 008 makes them
  `#[non_exhaustive]` with constructor-based creation. Decision 008 lands first.
- `Error` gains `InvalidScopeKey`, consistent with decision 002's single non-exhaustive core error.
- The D1 test suite needs cases for cross-scope invisibility: a record written under scope A is
  absent from scope B's batch, absent from B's counts, and still present and syncable when A is
  reopened.
- `wiki/specs/source-frontend-cache-architecture.spec.md` must record the source's dual-constructor
  behavior as a known divergence. It is currently unrecorded.

## Revisit If

The D4 migration trial shows callers systematically composing keys wrong — omitting tenant,
omitting schema version, or building keys that collide after backend encoding. The remedy is an
additional structured constructor taking `Scope { principal, tenant, schema_version }` that builds
the key for them. That is additive as long as the opaque key remains the storage-level identity.

## Prior-Art Support

From D0a (`wiki/references/prior-art-survey.reference.md`).

**Every surveyed system treats local store scope as a safety boundary, and every one of them leaves
enforcement to the caller.** Replicache documents that "each user of your application uses a
different Replicache `name`" — a warning, in the docs, because callers get it wrong. Zero combines
authenticated context with storage identity. PowerSync scopes Sync Streams by authenticated
parameters and explicitly warns that client-supplied parameters are not an authorization
mechanism. CouchDB enforces security per database and treats filters and selectors as selection
tools, not tenant isolation — choosing wrong there is the classic CouchDB mistake.

**No surveyed system enforces the boundary inside the client library.** They name it, warn about
it, and rely on discipline. frontbox enforcing equality on read is a deliberate strengthening, the
same posture taken in decision 006: borrow the established pattern, then close the gap where peers
depend on callers remembering.

**The alternative of resetting local state on scope change is documented and rated poorly by its
own authors.** Electric describes clearing all local state and writes on a rejected write as "very
naive." Applied to a user switch it trades a leak for data loss, which contradicts frontbox's
premise that queued writes survive. Rule 4 retains instead, consistent with decision 005's bias
toward retention over discard.

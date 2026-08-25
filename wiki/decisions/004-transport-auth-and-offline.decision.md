# Auth Is Fresh Per Send; Offline Is Not A Transport Failure

Document Class: Decision
Status: Accepted
Date: 2026-08-25
Category: API Shape
Scope: How authentication reaches the sync transport, and how the runner distinguishes being offline from a failed request.
Sources: `raw/initial/2026-08-25T083750Z/sources/persistence/mutations.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/types.rs`
Related: `wiki/decisions/002-error-model.decision.md`, `wiki/specs/source-frontend-cache-architecture.spec.md`, `wiki/references/prior-art-survey.reference.md`

## Decision

Core persists no auth data on outbox records. Each send evaluates auth freshly through the
application-owned transport or auth provider.

The sync transport returns `Error::Offline` when the network is unavailable, and `Error::Transport`
when a request was attempted but failed.

```rust
pub trait SyncTransport {
    async fn send_batch(
        &self,
        request: MutationBatchRequest,
    ) -> Result<MutationBatchResponse, Error>;
}
```

The trait does not expose a bearer-token type, header map, cookie model, or credential snapshot.
Implementors close over whatever auth mechanism their app uses, but they must evaluate it per
send. Capturing one token at construction time is not equivalent.

## Why: Auth

The source signature is `sync(&self, auth_header: &str)` (`persistence/mutations.rs:612`), and the
header goes straight through to the request: `push_mutations(auth_header.to_string(), request)`
(`persistence/mutations.rs:653`). The sync loop obtains it through a closure:
`get_auth_header: impl Fn() -> Option<String> + 'static` (`persistence/mutations.rs:727`).

That means auth is genuinely per sync call. Tokens expire, and a queued mutation may sit in the
outbox for days before it is sent. Persisting credentials on the record would replay stale or
sensitive state.

The correct extraction is an app-owned transport that can consult a token provider immediately
before sending. Core owns durability and status application; the app owns credentials.

## Why: Offline

The source tracks offline separately from generic errors. `SyncStatus::Offline` means keep work
queued and do not enter the error backoff path, while `SyncStatus::Error` drives the longer retry
interval (`persistence/mutations.rs:61,334-345,731-748`).

Offline is an expected operating mode for this library. Collapsing it into a generic transport
failure would produce misleading status and retry behavior.

## Implementor Guidance

Native transports should map OS/client connectivity failures, DNS/network-unreachable failures,
and explicit "no network" probes to `Error::Offline` when no request could reasonably complete.
HTTP responses from the server are not offline; they are either successful batch responses or
transport/protocol failures.

Wasm transports should use browser signals such as `navigator.onLine` only as hints. A failed
`fetch` caused by network unavailability should map to `Error::Offline`; malformed responses,
timeouts after a request was attempted, or server errors map to `Error::Transport` or
`Error::Protocol` depending on the failure.

Missing auth is not the same as offline. An app may choose not to call sync without credentials,
or its transport may return a typed auth-related transport/protocol error, but core should not
invent credentials or treat missing auth as network loss.

## Consequences

- Core records cannot leak credentials because they never contain credentials.
- Token refresh works naturally because auth is evaluated per send.
- The runner can report offline state without counting it as a failed sync attempt.
- Apps with unusual auth, including cookies, signed requests, mTLS, or bearer tokens, do not need
  core API changes.

## Revisit If

A real product needs multiple principals sharing one durable outbox. That would be a different
privacy and partitioning requirement, and likely needs per-principal storage namespaces before any
per-record auth field is considered.

## Prior-Art Support

Added 2026-08-26 from D0a (`wiki/references/prior-art-survey.reference.md`). The survey did not
contradict this decision.

- **No surveyed system persists credentials inside queued records.** Across both cohorts, auth is
  resolved at send time from ambient session state, never snapshotted per queued write.
- **Redux Offline** is the closest match to frontbox's shape and puts token refresh at exactly this
  seam. Its documented async `discard` calls `refreshAccessToken()` on a `401` and returns `false`
  so the request retries with a fresh token, rather than treating the 401 as terminal. That is
  frontbox's rule in another codebase: auth failure is a property of the *attempt*, not of the
  queued record.
- **PowerSync** and **Zero** both scope access through authenticated parameters resolved per
  connection, and PowerSync warns that client-supplied parameters must not be trusted for
  authorization on their own.

The 401-as-transient case is worth noting against decision 005: a `401` is an HTTP client error but
is not a terminal application refusal. Redux Offline's default discards all 4xx, and its own docs
override that for 401. frontbox's `Rejected` status is a server verdict rather than a status-code
class, which sidesteps the problem — but any future status-code-derived classification must not
treat `401` as `Rejected`.

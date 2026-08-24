# UI State + Reuse Spec (Dioxus)

- Document Class: Spec
- Status: Active
- Date: 2026-04-12
- Category: UX behavior
- Scope: Current product behavior


> **Domain note**: This spec defines cross-cutting UI patterns (state management, sync, auth).
> For workout-specific screens and components, see `ui/README.md` and `ui/screens/*`.

## Goal

Maximize code reuse between desktop, mobile, and web:
- Shared presentational components live in `code/frontend/shared-ui`
- App wiring/state/persistence/sync live in `code/frontend/app`
- Authentication via Keycloak (see `09-authentication-and-authorization.spec.md`)

---

## Offline Support by Platform (MVP)

> **Authoritative source**: See `mvp.spec.md` for the canonical MVP scope.

| Platform | Offline Support | Notes |
|----------|-----------------|-------|
| **Mobile (iOS/Android)** | Offline-first | Commands queued locally (SQLite), synced when online |
| **Desktop** | Offline-first | Commands queued locally (SQLite), synced when online |
| **Web** | Offline-first | Commands queued locally (IndexedDB), synced when online |

**Implication**: The sync patterns described below (outbox, optimistic projection) apply to **all platforms** in MVP.

Web caveat:
- Browsers may disable or evict storage (private browsing, storage pressure). When persistence is unavailable, the web client MUST surface an "offline unavailable" state and MUST NOT claim offline capability.

---

## Component boundaries

### `code/frontend/shared-ui`

Must contain only presentational components:
- Inputs: props (data) + callbacks
- Outputs: UI events via callbacks

No:
- IndexedDB calls
- HTTP calls
- global state

See `ui/README.md` for the component inventory (HomeHub, ActiveSession, TemplatesList, etc.).

### `code/frontend/app`

Owns:
- Local storage (IndexedDB on web, SQLite on native)
- API client
- Sync logic (push commands, pull state)
- App-level state

---

## State model (app-level)

App should track:
- Domain state (exercises, templates, sessions, preferences) sourced from local cache
- `pending_ops: usize` (count from outbox)
- `rejected_count: usize` (count from dead_letter store)
- `sync_error: Option<String>` (current sync error, if any)
- `auth_state: AuthState` (see below)

### Authentication state

```rust
enum AuthState {
    Loading,           // Checking for stored tokens
    Unauthenticated,   // No valid tokens, show login
    Authenticated {    // Valid session
        user_id: Uuid,
        username: String,
        access_token: String,
        refresh_token: String,
    },
    TokenExpired,      // Token expired, redirect to login
    Error(String),     // Auth error
}
```

Rules:
- UI reads from local cache, not from remote responses.
- After any local mutation, refresh state from local cache (or update in-memory and persist).
- Tokens are persisted in sessionStorage (web) or secure storage (native).

---

## Flows

### App startup

1. Initialize DB + run migrations.
2. Check for stored auth tokens.
3. If tokens found, restore session and set `AuthState::Authenticated`.
4. If no tokens, set `AuthState::Unauthenticated` and show login screen.
5. Load domain state from local cache.
6. Load outbox count into state.

### Authentication

Authentication differs by platform:

- **Web**: OIDC Authorization Code flow with PKCE (browser redirect to Keycloak)
- **Native (Mobile/Desktop)**: Device Authorization Grant (RFC 8628)

> **Authoritative spec**: See `09-authentication-and-authorization.spec.md` for full authentication details.

#### Login flow (Web - OIDC with PKCE)

1. User clicks "Sign in".
2. Generate PKCE challenge (code_verifier + code_challenge).
3. Store code_verifier in sessionStorage.
4. Redirect to Keycloak authorization endpoint.
5. User authenticates with Keycloak (supports SSO).
6. Keycloak redirects back with authorization code.
7. Exchange code + code_verifier for tokens.
8. Parse access token to extract user_id and username.
9. Store tokens in sessionStorage.
10. Set `AuthState::Authenticated`.

#### Login flow (Native - Device Authorization Grant)

Native apps use Device Authorization Grant (RFC 8628) for passwordless authentication:

```
┌─────────────────────────────────────────────────────────────────────┐
│                  NATIVE DEVICE AUTH FLOW                             │
│                                                                      │
│  Step 1: User enters email → [Continue]                             │
│          ↓                                                           │
│  App: POST /realms/{realm}/protocol/openid-connect/auth/device      │
│       Body: client_id, scope                                         │
│       Response: { device_code, user_code, verification_uri, ... }   │
│          ↓                                                           │
│  Step 2: App sends magic link email OR shows verification_uri       │
│          ↓                                                           │
│  Step 3: User clicks link/enters code in browser → authenticates    │
│          ↓                                                           │
│  Step 4: App polls POST /realms/{realm}/protocol/openid-connect/token
│          Body: grant_type=device_code, device_code, client_id        │
│          Response: { access_token, refresh_token, ... }              │
│          ↓                                                           │
│  Step 5: Store tokens securely, set AuthState::Authenticated        │
└─────────────────────────────────────────────────────────────────────┘
```

#### Logout flow

1. Clear tokens from memory and storage.
2. **Web**: Redirect to Keycloak logout endpoint (clears SSO session).
3. **Native**: Simply clear local tokens (no SSO session to clear).
4. Set `AuthState::Unauthenticated`.

### Domain mutations (create/update/delete)

All domain mutations follow the same pattern:

1. User performs action (e.g., log a set, create a session).
2. App writes to local cache with optimistic projection.
3. App writes a mutation intent to the local outbox (with `mutation_id`).
4. Refresh UI state from local cache.
5. **Immediately push** outbox to server.

See `../adr/offline-sync-technical-mechanics.adr.md` for the current mutation-intent sync flow.

### SSE-based sync loop

A background task maintains real-time sync via Server-Sent Events.

> **Authoritative spec**: See `08-offline-sync.spec.md` §8 for full sync loop specification.

```
┌─────────────────────────────────────────────────────────────────────┐
│                        SYNC LOOP                                     │
│                                                                      │
│  loop {                                                              │
│      // 1. Wait for authentication                                   │
│      if not authenticated { wait 1s; continue }                      │
│                                                                      │
│      // 2. Sync on (re)connect                                       │
│      push_outbox()                                                   │
│      if outbox_empty { pull_state() }  // GET /api/v1/sync/state     │
│      refresh_ui()                                                    │
│                                                                      │
│      // 3. Connect SSE                                               │
│      stream = connect_sse(token)  // GET /api/v1/events              │
│                                                                      │
│      // 4. Listen for events (with watchdog timeout)                 │
│      while event = stream.next_with_timeout(120s) {                  │
│          match event {                                               │
│              Invalidation => {                                       │
│                  if outbox_empty { pull_state(); refresh_ui() }      │
│              }                                                       │
│              Timeout | Error => break  // triggers reconnect         │
│          }                                                           │
│      }                                                               │
│                                                                      │
│      // 5. Wait before reconnecting                                  │
│      wait 2s                                                         │
│  }                                                                   │
└─────────────────────────────────────────────────────────────────────┘
```

### Background retry task

A separate task retries failed pushes every 30 seconds:

1. Check outbox count.
2. If pending commands exist, attempt push.
3. On success, SSE invalidation will trigger pull in main loop.

### Offline behavior

- When offline, SSE connection fails → wait 2s → retry.
- Local mutations continue to work (outbox + optimistic projection).
- When connectivity returns, sync loop reconnects and catches up.

---

## Mobile/desktop networking note

All clients connect to the BFF:
- Desktop/Web: `http://localhost:8081` (dev) or `https://api.repforge.com` (prod)
- Android emulator: `http://10.0.2.2:8081` (special alias for host machine)

The BFF serves:
- `/api/v1/*` - REST endpoints
- `/api/v1/events` - SSE stream

---

## Platform-specific storage

| Platform | Storage | Notes |
|----------|---------|-------|
| Web (wasm32) | IndexedDB via `rexie` | Browser's native IndexedDB |
| Desktop/Mobile (native) | SQLite via `rusqlite` | File-based, platform data dir |

Both backends provide the same API:
- Domain cache stores (exercises, templates, sessions, preferences)
- `OutboxStore`: pending commands
- `DeadLetterStore`: rejected commands

---

## Acceptance criteria

- [ ] shared-ui contains only reusable components (props-driven)
- [ ] app crate owns persistence + networking
- [ ] offline actions update UI immediately (optimistic projection)
- [ ] sync happens automatically when online and after reconnect
- [ ] sync status visible and accurate
- [ ] SSE-based real-time sync (not polling)
- [ ] Authentication via Keycloak (Device Grant for native, PKCE for web)
- [ ] Session persistence (survives app restart)
- [ ] Watchdog timeout for SSE connection health

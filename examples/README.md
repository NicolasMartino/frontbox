# Offline Todo Trial

The trial is split into four packages:

- `todo-server`: axum/sqlx server that hand-writes the frontbox wire shapes.
- `user-server`: a second domain server, independent of the first, that hand-writes its own wire
  shapes for the same reason. Two services behind one queue is what makes cross-service ordering
  and the inbound invalidation path observable rather than asserted (D4d).
- `todo-core`: framework-neutral application logic and the observation tests.
- `todo-app`: Dioxus UI over `todo-core`, on **four platforms from one component tree** — web,
  macOS desktop, iOS and Android, with nothing below `main.rs` conditionally compiled. Everything a
  platform decides lives in `examples/todo-app/src/platform/mod.rs`, and that it is four items long
  rather than forty is the finding (D4c).

Run the automated observations and regressions from the workspace root:

```bash
cargo test -p todo-core -p todo-server -p user-server
```

That is the whole of the automated evidence. **The UI is gated by compiling and linting and by
nothing else** — there is no headless Dioxus runtime here, so anything the UI half does wrong is
found by running it, not by the suite.

All four targets compile under `scripts/verify.sh`: web and desktop always, **iOS and Android when
the machine has the toolchain** and announced as `SKIPPED` when it does not. Those two were ungated
until 2026-09-01 on the reasoning that a build gate cannot assume Xcode or an NDK — true, and not a
reason to have no gate. `wiki/plans/d4c-multi-platform-trial.plan.md` records what running them on
emulators found, including one open defect on desktop.

Everything below has a `just` recipe, which is the shorter path and the one that
gets the per-platform server URLs right:

```bash
just examples          # the Docker stack: two servers and the web UI
just examples web      # dx serve, against the containerised servers
just examples native   # macOS desktop
just examples ios      # boots the frontbox-iphone simulator
just examples android  # starts the frontbox AVD
just examples e2e      # the Playwright run
just examples down
```

**Override the port, never the URL.** `just` derives `TODO_SERVER_URL` and `USER_SERVER_URL` from
`TODO_SERVER_PORT` and `USER_SERVER_PORT`, and substitutes `10.0.2.2` for Android on its own. The
URLs are compiled into the binary through `option_env!`, so a mismatch is a stack where every
service is healthy and the application silently talks to the wrong port — and there is no error for
that, because a fetch to a dead port fails the way being offline does.

Run the full demo with Docker Compose directly:

```bash
docker compose up --build
```

Three services, because D4d needed a second domain to make cross-service ordering testable:

| Service | Port | What it is |
| --- | --- | --- |
| `todo-app` | 8081 | The Dioxus UI, served by nginx as static WASM |
| `todo-server` | 3000 | Todos, and the batch sync endpoint |
| `user-server` | 3001 | User records, and the batch sync endpoint |

Swagger UI is at `/swagger-ui/` on both servers and the raw OpenAPI document at
`/api-docs/openapi.json`. Both store data in memory, so stopping the containers deletes the demo
rows.

`todo-server` validates a todo's `user_id` against `user-server` before accepting it, and
`user-server` deletes a user's todos through `todo-server` before deleting the user. Both are
**fixtures rather than architecture recommendations** — together they are a dependency cycle a real
split would break — and they exist because they are what make cross-service ordering observable, in
both directions. `examples/todo-server/src/users.rs` and `examples/user-server/src/todos.rs` say so
at length, and `wiki/decisions/039-user-delete-cascades-server-side.decision.md` records the cost.

The UI is two levels: sign up, then a list of users, and inside each user their todos. Nothing
todo-shaped appears before you have signed up. Your own record is an ordinary row in that list,
tagged `(you)` — sign up with the network off and watch it queue rather than send.

Publish on different ports when something already holds these:

```bash
TODO_APP_PORT=8081 TODO_SERVER_PORT=4000 USER_SERVER_PORT=4001 docker compose up --build
```

The URLs compiled into the WASM follow the published ports automatically — `docker-compose.yml`
interpolates them rather than repeating them, because a stack where every container is healthy and
the application silently talks to the wrong port has no error to show you: the browser's fetch fails
the way being offline does, which is the one condition this application treats as normal.

Set `TODO_SERVER_URL` or `USER_SERVER_URL` directly only to point the UI somewhere other than the
loopback, and use a real host URL rather than a Docker service name. The browser executes the WASM
and has no route to Docker's internal hostnames.

Run the demo servers:

```bash
cargo run -p user-server   # binds 127.0.0.1:3001
USER_SERVER_URL=http://127.0.0.1:3001 cargo run -p todo-server   # binds 127.0.0.1:3000
```

`todo-server` runs happily without `USER_SERVER_URL`; the reference check is simply off, which is
the honest state of a single-service deployment and what every trial before D4d ran against.

Override the bind addresses when needed:

```bash
TODO_SERVER_ADDR=127.0.0.1:4000 cargo run -p todo-server
USER_SERVER_ADDR=127.0.0.1:4001 cargo run -p user-server
```

Swagger UI follows the same bind address:

```bash
open http://127.0.0.1:3000/swagger-ui/
```

Run the Dioxus UI from the app directory. `dx` is the Dioxus CLI, which is a separate install:

```bash
cargo install dioxus-cli   # once; the UI is built with dioxus 0.7.10
cd examples/todo-app
dx serve --web
```

The other three platforms are the same crate with a different feature:

```bash
dx serve --platform desktop
dx serve --platform ios                              # needs Xcode
ANDROID_NDK_HOME=<ndk> dx serve --platform android   # needs an NDK
```

**Nothing below `main.rs` is conditionally compiled** — one `platform` module holds everything a
platform decides, which is D4c's result and the reason this list is three lines.

If the servers are not on their default URLs, pass them at build time. Both are read through
`option_env!`, so they are baked into the binary rather than read at startup:

```bash
TODO_SERVER_URL=http://127.0.0.1:4000 USER_SERVER_URL=http://127.0.0.1:4001 dx serve --web
```

An **Android emulator** needs `10.0.2.2` rather than `127.0.0.1` — the guest has its own loopback,
and the platform module already substitutes it. An **iOS simulator** shares the host's network
stack, so the loopback address is right there. A real device on either platform needs the host's
LAN address for both servers.

### End-to-end, in a real browser

`examples/todo-app/e2e/ui.mjs` drives the whole UI with Playwright — the signup gate, user CRUD,
nested todo CRUD, the delete confirm, the cascade asserted against the *todo service*, and a reload.
**It asserts on console errors as loudly as on behaviour**, which is what makes it worth having:

```bash
docker compose up -d --build
npm i -D playwright && npx playwright install chromium
APP_URL=http://127.0.0.1:8081 TODO_URL=http://127.0.0.1:3000 USER_URL=http://127.0.0.1:3001   node examples/todo-app/e2e/ui.mjs
```

Restart the two servers first if they hold state from an earlier run — they are in memory, so a
restart is the reset, and the signup gate cannot be reached while the user service still knows this
client's user id.

It is not in `scripts/verify.sh` because it needs a running stack and a browser install, which a
build gate cannot assume. It found two defects a compiler cannot see, both recorded in its header.

The UI has an `offline` checkbox wired to the trial transport. Turning it on makes writes queue
without stopping the server, which is the path the observation tests exercise deterministically.

Things worth doing by hand, because no test can:

- Tick `offline`, add three todos, watch each row say `saving…` and the status bar count them, then
  untick it and watch one drain clear all three.
- Rename a row to nothing but spaces while online. The server refuses it terminally, and it appears
  in the dead-letter panel in the server's own words rather than vanishing.
- Stop `todo-server` entirely with rows still queued. The status bar's `last drain` goes to
  `Offline` and the queue is untouched, which is the distinction decision 004 exists for: offline is
  not a failed attempt.

//! The harness the trial test suites share.
//!
//! `#![allow(dead_code)]` because each test binary compiles this module separately and uses a
//! different part of it: the `observations` and `regressions` suites predate D4d and run against
//! one server, `multi_domain` runs against two. Without the allow, every binary warns about the
//! helpers the other binaries use.
#![allow(dead_code)]
//!
//! Split out of the observations suite when the regressions one arrived and copied it: a second
//! transcription of a fixed clock and an ephemeral-port server is a second place for the trial's
//! setup to drift. Each of those is a directory with a `main.rs` facade now, which is why the
//! suites are named here without an extension.

use frontbox::Clock;
use todo_core::{Config, TodoApp};

/// The scope every trial test runs under, named because observation 3 turns on two applications
/// opening the *same* one.
///
/// A **local queue identity**, composed here with no server involved
/// (`wiki/decisions/009-local-scope-identity.decision.md`). Not to be confused with [`ALICE_USER`].
pub const ALICE: &str = "user:alice@tenant:acme";

/// The user *record* id Alice's todos belong to.
///
/// A row id on the user service, which D4d added. The two constants above and below are the two
/// meanings of "user" this trial keeps distinct: one is the queue's name for itself, the other is a
/// row a server either has or has not accepted yet. See `examples/todo-core/src/app/identity.rs`.
pub const ALICE_USER: &str = "user-alice";

/// A second user record, for the observations about somebody else's todos.
pub const BOB_USER: &str = "user-bob";

/// A clock that does not need a platform. The trial is not testing time.
pub struct FixedClock(std::rc::Rc<std::cell::Cell<i64>>);

impl FixedClock {
    pub fn new() -> Self {
        Self(std::rc::Rc::new(std::cell::Cell::new(1_700_000_000_000)))
    }
}

impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        // Advances by a millisecond per read, so `created_at` is distinct per enqueue without a
        // platform clock. Ordering does not depend on it — decision 016 made `seq` the sort key —
        // but a same-millisecond tie would make a test agree with the old behaviour by accident.
        let now = self.0.get();
        self.0.set(now + 1);
        now
    }
}

/// Start the trial's todo server on an ephemeral port, with no user service behind it.
///
/// What every trial before D4d had, and what the tests written for those trials still use. The
/// reference check is off, so a `user_id` on a todo is carried and not validated — which is the
/// honest state of a deployment with one service.
pub async fn server() -> (String, todo_server::AppState) {
    let state = todo_server::AppState::in_memory()
        .await
        .expect("server schema");
    let base = serve(todo_server::router(state.clone())).await;
    (base, state)
}

/// Both domain services, each pointed at the other.
///
/// # Bind first, build second, and the reason is the dependency cycle
///
/// `todo-server` calls `user-server` to check a todo's owner; `user-server` calls `todo-server` to
/// cascade a user delete. Neither can be *constructed* before it knows the other's address, so the
/// harness takes both ports first and only then builds the two states. Serving is the last step.
///
/// In Compose the addresses are static configuration and the question does not arise, which is
/// exactly why it is worth a comment here: the cycle is invisible until something has to resolve
/// it at run time. It is a genuine smell and `examples/user-server/src/todos.rs` says so.
pub async fn servers() -> Servers {
    let (user_listener, user_url) = bind().await;
    let (todo_listener, todo_url) = bind().await;

    let user = user_server::AppState::in_memory()
        .await
        .expect("user schema")
        .with_todo_server(todo_url.clone());
    let todo = todo_server::AppState::in_memory()
        .await
        .expect("todo schema")
        .with_user_server(user_url.clone());

    serve_on(user_listener, user_server::router(user.clone()));
    serve_on(todo_listener, todo_server::router(todo.clone()));

    Servers {
        todo_url,
        todo,
        user_url,
        user,
    }
}

/// The two services D4d runs against.
pub struct Servers {
    /// Base URL of the todo service.
    pub todo_url: String,
    /// The todo service's state, for the trial's hold and silence switches.
    pub todo: todo_server::AppState,
    /// Base URL of the user service.
    pub user_url: String,
    /// The user service's state.
    pub user: user_server::AppState,
}

/// Bind an ephemeral port and serve a router on it, returning its base URL.
pub(crate) async fn serve(app: axum::Router) -> String {
    let (listener, base) = bind().await;
    serve_on(listener, app);
    base
}

/// Take an ephemeral port without serving on it yet.
async fn bind() -> (tokio::net::TcpListener, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    (listener, base)
}

fn serve_on(listener: tokio::net::TcpListener, app: axum::Router) {
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
}

/// Build an application against a running server, on durable storage.
///
/// `:memory:` rather than a file, and that is not a shortcut: it is the same SQLite, the same
/// schema and the same transactions, so what the observations exercise is the durable code path
/// rather than the filesystem. The one thing it cannot show is a restart, which is exactly what
/// [`reopen`](todo_core::TodoApp::reopen) exists for and what `an_offline_reload_restores_rows_from_storage`
/// uses it to demonstrate.
pub async fn app(base: &str, scope: &str) -> TodoApp {
    open(Config::single_server(base, scope, ALICE_USER, ":memory:")).await
}

/// Build an application against both services.
pub async fn app_on(servers: &Servers, scope: &str, user_id: &str) -> TodoApp {
    open(Config {
        todo_url: servers.todo_url.clone(),
        user_url: Some(servers.user_url.clone()),
        scope: scope.to_owned(),
        user_id: user_id.to_owned(),
        storage: ":memory:".to_owned(),
    })
    .await
}

/// Open an application on durable storage from a finished configuration.
pub async fn open(config: Config) -> TodoApp {
    TodoApp::new(config, FixedClock::new())
        .await
        .expect("open storage")
}

/// A service that answers `/api/v1/sync` with 500, and its base URL.
///
/// # Why a live server and not a dead port
///
/// A dead port is *offline* by this transport's own classification — `map_send_error` maps a
/// refused connection to [`frontbox::Error::Offline`], deliberately, because a fetch to nothing is
/// indistinguishable from having no network
/// (`examples/todo-core/src/transport/error.rs`). The condition worth testing is the other one: an
/// endpoint that is reachable and wrong. Only a server that accepts the connection and then refuses
/// the request produces it.
pub async fn broken_sync_server() -> String {
    let app = axum::Router::new().route(
        "/api/v1/sync",
        axum::routing::post(|| async { axum::http::StatusCode::INTERNAL_SERVER_ERROR }),
    );
    serve(app).await
}

/// A base URL nothing is listening on, so a request to it is refused at the connection.
///
/// # Why this is "offline" and the broken server is not
///
/// The two are the transport's whole classification, and they must be reachable separately to test
/// the mixed case. A refused connection maps to [`frontbox::Error::Offline`] deliberately — a fetch
/// to nothing is indistinguishable from having no network — while a server that accepts and then
/// answers 500 is an *attempted failure*. See `examples/todo-core/src/transport/error.rs`.
///
/// The listener is bound and dropped rather than a port being guessed: binding to `:0` is what
/// makes the address certainly free, and dropping it before returning is what keeps it that way for
/// the moment that matters.
pub async fn unreachable_server() -> String {
    let (listener, base) = bind().await;
    drop(listener);
    base
}

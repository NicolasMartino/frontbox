//! The queue itself: one outbox, two services, and the order between them.
//!
//! Part of the `multi_domain` observations; see `main.rs` for what they are evidence of.

use crate::common::{self, ALICE, ALICE_USER, BOB_USER};
use frontbox::{DeadLetterStore, OutboxStore};
use todo_core::Config;
/// **1. Offline signup survives a restart.**
///
/// The scope is composed locally and is valid the instant this client picks it, so work queues
/// under it before any server has heard of anyone
/// (`wiki/decisions/009-local-scope-identity.decision.md`). Closing and reopening the application
/// must not change that.
#[tokio::test]
async fn observation_1_offline_signup_survives_a_restart() {
    let servers = common::servers().await;
    let directory = std::env::temp_dir().join(format!("frontbox-d4d-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("temp dir");
    let path = directory.join("observation-1.db");
    let _ = std::fs::remove_file(&path);
    let storage = path.to_str().expect("utf-8 path").to_owned();

    let config = Config {
        todo_url: servers.todo_url.clone(),
        user_url: Some(servers.user_url.clone()),
        scope: ALICE.to_owned(),
        user_id: ALICE_USER.to_owned(),
        storage,
    };

    {
        let app = common::open(config.clone()).await;
        app.transport().set_offline(true);
        app.sign_up("Alice").await.expect("sign up");
        app.create(ALICE_USER, "buy milk").await.expect("create");
        app.create(ALICE_USER, "call mum").await.expect("create");
        assert_eq!(
            app.outbox().pending_count().await.expect("count"),
            3,
            "one user record and two todos, all queued with no server involved"
        );
    } // Everything the first "process" held drops here.

    let restarted = common::open(config).await;
    assert_eq!(
        restarted.outbox().pending_count().await.expect("count"),
        3,
        "the queue outlived the process it was created in"
    );
    assert_eq!(
        servers.user.applied_count().await.expect("count"),
        0,
        "and none of it reached a server, which is what makes the scope's independence the point"
    );

    let _ = std::fs::remove_file(&path);
}

/// **2. Cross-service order holds.**
///
/// The user record reaches the user service before any todo reaches the todo service. Asserted on
/// the **servers' state**, not on the client's queue: the todo server refuses a todo whose user it
/// cannot find, so a todo that arrived first would be visible as a refusal rather than as a
/// reordering nobody checked.
#[tokio::test]
async fn observation_2_cross_service_order_holds_under_one_scope() {
    let servers = common::servers().await;
    let app = common::app_on(&servers, ALICE, ALICE_USER).await;

    app.transport().set_offline(true);
    app.sign_up("Alice").await.expect("sign up");
    let first = app.create(ALICE_USER, "buy milk").await.expect("create");
    let second = app.create(ALICE_USER, "call mum").await.expect("create");

    app.transport().set_offline(false);
    let report = app.sync().await.expect("drain");

    assert_eq!(
        app.outbox().pending_count().await.expect("count"),
        0,
        "everything drained: {report:?}"
    );
    assert_eq!(servers.user.applied_count().await.expect("count"), 1);
    assert_eq!(servers.todo.applied_count().await.expect("count"), 2);

    let todos: Vec<serde_json::Value> = reqwest::get(format!("{}/api/v1/todos", servers.todo_url))
        .await
        .expect("read todos")
        .json()
        .await
        .expect("json");
    assert_eq!(todos.len(), 2);
    for todo in &todos {
        assert_eq!(
            todo["user_id"], ALICE_USER,
            "every todo landed owned by the user record that preceded it"
        );
    }
    assert!(todos.iter().any(|t| t["id"] == first));
    assert!(todos.iter().any(|t| t["id"] == second));
}

/// **3. Sabotage: one scope per service loses the ordering, and the loss is visible.**
///
/// **Without this, observation 2 proves that a drain worked, not that ordering held.** Split the
/// queue — which is decision 036's rejected alternative, built here on purpose — and the todo
/// reaches a server that has never heard of its user.
///
/// # What the client must do with that, and what it must not
///
/// The todo server answers `404`. Decision 019 opens by saying core does not classify HTTP
/// responses and will not; what it places on a *transport* is the obligation that **a missing
/// prerequisite is transient** and must never become `Rejected`. So the trial's routing transport
/// maps it to `MutationStatus::Blocked` — the status 019 describes as "this write failed only
/// because a predecessor had not landed" — and the assertions below are on the retained surface,
/// not the anomaly one. Anomalies are raised by `Unknown` alone.
#[tokio::test]
async fn observation_3_two_scopes_lose_the_order_and_the_todo_is_blocked() {
    let servers = common::servers().await;

    // One scope for todos, a different one for the user record. Two queues, two `seq` counters,
    // and nothing relating them — which is exactly what decision 036 declined to build.
    let todos = common::app_on(&servers, "user:alice@svc:todo", ALICE_USER).await;
    let users = common::app_on(&servers, "user:alice@svc:user", ALICE_USER).await;

    users.transport().set_offline(true);
    users.sign_up("Alice").await.expect("sign up");
    todos.create(ALICE_USER, "buy milk").await.expect("create");

    // The todo queue drains while the user queue is still offline. Under one scope this ordering is
    // impossible; under two it is just what happened to run first.
    let report = todos.sync().await.expect("drain");
    assert_eq!(
        report.counts.blocked, 1,
        "a missing prerequisite is transient, so the record is retained as Blocked: {report:?}"
    );
    assert_eq!(
        report.counts.dead_lettered, 0,
        "and never terminally refused"
    );
    assert!(
        report.anomalies.is_empty(),
        "Blocked is a verdict, not an anomaly — only Unknown raises one: {:?}",
        report.anomalies
    );
    assert_eq!(
        todos.outbox().pending_count().await.expect("count"),
        1,
        "the write is still queued, which is the whole point of it being transient"
    );
    assert_eq!(
        DeadLetterStore::list(todos.outbox(), 10)
            .await
            .expect("dead letters")
            .len(),
        0
    );

    // And it heals the moment the prerequisite lands, which is what makes `Blocked` the right
    // classification rather than merely a kinder one.
    users.transport().set_offline(false);
    users.sync().await.expect("user drain");
    let healed = todos.sync().await.expect("retry");
    assert_eq!(healed.counts.applied, 1);
    assert_eq!(todos.outbox().pending_count().await.expect("count"), 0);
}

/// **4. A down service does not lose the other's verdicts.**
///
/// With the **todo** service declining to rule, user mutations still apply and todo mutations stay
/// queued. That is decision 037's partial-batch case, and it needed no protocol change: core
/// retains any record a response does not name.
///
/// # The direction is forced, and that is worth stating rather than hiding
///
/// The reference check makes the todo server depend on the user server, so stopping the *user*
/// server would make todo mutations fail too — for the fixture's reason rather than the
/// transport's — and the observation would prove nothing about partial batches. Stopping the todo
/// side is the only one with no dependency, so it is the only one that isolates the mechanic.
#[tokio::test]
async fn observation_4_a_down_service_does_not_lose_the_others_verdicts() {
    let servers = common::servers().await;
    let app = common::app_on(&servers, ALICE, ALICE_USER).await;

    app.transport().set_offline(true);
    app.sign_up("Alice").await.expect("sign up");
    app.create(ALICE_USER, "buy milk").await.expect("create");
    app.transport().set_offline(false);

    servers.todo.silent(true);
    let report = app.sync().await.expect("drain");

    assert_eq!(
        servers.user.applied_count().await.expect("count"),
        1,
        "the reachable service ruled, and its verdict survived the other one's silence"
    );
    assert_eq!(servers.todo.applied_count().await.expect("count"), 0);
    assert_eq!(
        app.outbox().pending_count().await.expect("count"),
        1,
        "only the todo is still queued: {report:?}"
    );

    let queued = app.outbox().pending_batch(10).await.expect("batch");
    assert_eq!(
        queued[0].attempts, 2,
        "a record the server declined to name is a *failed attempt*, so decision 017's bound moves"
    );
    // **Two, not one, and that is a cost of decision 037 worth naming.** A drain is a loop that
    // stops on the first pass making no progress. The user record draining *is* progress, so the
    // loop runs a second pass, which re-sends the todo to the same silent service and burns a
    // second attempt. A partial batch therefore consumes decision 017's retention bound faster
    // than one attempt per drain — an interaction between 037's routing and 029's termination rule
    // that neither page anticipated, and that only a two-service client can produce.

    servers.todo.silent(false);
    app.sync().await.expect("retry");
    assert_eq!(app.outbox().pending_count().await.expect("count"), 0);
}

/// **5. A cached todo is readable offline, including one owned by somebody else.**
///
/// Decision 032's row store holding rows from two domains. The other user's record lives in *this*
/// scope's row store, which is correct and reads as a leak: the store holds what this client
/// fetched, not what this user owns.
#[tokio::test]
async fn observation_5_another_users_todo_is_readable_offline() {
    let servers = common::servers().await;

    let bob = common::app_on(&servers, "user:bob@tenant:acme", BOB_USER).await;
    bob.sign_up("Bob").await.expect("sign up");
    bob.create(BOB_USER, "bob's errand").await.expect("create");
    bob.sync().await.expect("bob drains");

    let alice = common::app_on(&servers, ALICE, ALICE_USER).await;
    alice.sign_up("Alice").await.expect("sign up");
    alice.sync().await.expect("alice drains");
    alice.start().await.expect("hydrate");

    assert_eq!(
        alice.rows().len(),
        1,
        "Alice's client cached the todo it fetched, whoever owns it"
    );
    assert_eq!(alice.rows()[0].user_id, BOB_USER);
    assert_eq!(
        alice.user_name(BOB_USER).as_deref(),
        Some("Bob"),
        "and cached the user record needed to attribute it"
    );

    // The offline half: a brand new application over the same durable storage, with no network.
    let reopened = alice.reopen();
    reopened.transport().set_offline(true);
    reopened.start().await.expect("offline start");
    assert_eq!(reopened.rows().len(), 1, "read back from storage alone");
    assert_eq!(reopened.user_name(BOB_USER).as_deref(), Some("Bob"));
}

//! The first invalidation runner this project ever ran, and what it marks stale.
//!
//! Part of the `multi_domain` observations; see `main.rs` for what they are evidence of.

use crate::common::{self, ALICE, ALICE_USER};
use frontbox::CacheVersionStore;
use todo_core::invalidation::{InvalidationSource, Source};
/// **6. Invalidation runs, at last.**
///
/// A change made by one client becomes visible to another after a poll — the first time
/// `InvalidationRunner::apply` is ever called by an application.
#[tokio::test]
async fn observation_6_a_poll_marks_the_changed_entity_stale() {
    let servers = common::servers().await;
    let device_a = common::app_on(&servers, ALICE, ALICE_USER).await;
    device_a.sign_up("Alice").await.expect("sign up");
    device_a.sync().await.expect("drain");

    let device_b = common::app_on(&servers, "user:alice@device:b", ALICE_USER).await;

    // First poll establishes a baseline. Nothing is stale yet on the strength of never having
    // looked — an unknown version is a normal starting condition, not a change.
    let first = device_b.poll_invalidation().await.expect("first poll");
    assert!(
        first.dropped.is_empty(),
        "both services answered: {first:?}"
    );
    assert!(first.unknown.is_empty());

    // Device A writes. Device B has no idea yet.
    device_a
        .create(ALICE_USER, "buy milk")
        .await
        .expect("create");
    device_a.sync().await.expect("drain");

    let second = device_b.refresh_now().await.expect("second poll");
    assert!(
        second.changed.iter().any(|entity| entity == "todo"),
        "the todo entity's version moved: {second:?}"
    );
    assert!(
        !second.changed.iter().any(|entity| entity == "user"),
        "and the user entity's did not, because nothing about users changed: {second:?}"
    );
}

/// **7. The refresh button converges on demand, through the same seam.**
///
/// A source under its staleness budget is not due, so a cadence tick asks it nothing.
/// `refresh_now` asks it anyway — the same path, no second code route to keep in step.
///
/// # Where this diverged from the plan
///
/// The plan calls the refresh button "the second of the two sources this deliverable builds". It is
/// not a source, and trying to make it one is what showed why: **a button has no versions to
/// deliver.** A source answers *what changed*; the only honest answer a button has is *go and
/// look*, which is a trigger. See `TodoApp::refresh_now`.
#[tokio::test]
async fn observation_7_refresh_polls_a_source_that_is_not_due() {
    let servers = common::servers().await;
    let app = common::app_on(&servers, ALICE, ALICE_USER).await;

    let first = app.poll_invalidation().await.expect("first poll");
    assert_eq!(
        first.polled.len(),
        2,
        "the two *scheduled* sources have never been polled, so both are due: {first:?}"
    );
    assert!(
        !first.polled.iter().any(|name| name == "handed-in"),
        "and the push source is not, because it has no schedule to be behind. It used to be due \
         exactly once — 'never polled' outranked its infinite budget — which put it in `polled` on \
         the first round only, and the trial's cache line then rendered it ageing beside two \
         services that really are on a clock: {first:?}"
    );

    // The clock advances a millisecond per read, so nothing has come close to its budget.
    let second = app.poll_invalidation().await.expect("second poll");
    assert!(
        second.polled.is_empty(),
        "every source is inside its staleness budget: {second:?}"
    );

    let forced = app.refresh_now().await.expect("refresh");
    assert_eq!(
        forced.polled.len(),
        3,
        "the button asks anyway, through the same seam: {forced:?}"
    );
}

/// **8. A source that drops marks its own entities stale and leaves the other's alone.**
///
/// The per-source reconnect semantic decision 038 names as the reason a second origin is a
/// promotion criterion. With one entity, "this source's entities" and "every entity" are the same
/// set and a bug between them is invisible — which is why this observation could not exist before
/// there were two servers.
#[tokio::test]
async fn observation_8_one_source_dropping_marks_only_its_own_entities() {
    let servers = common::servers().await;
    let app = common::app_on(&servers, ALICE, ALICE_USER).await;

    // A baseline is required, and *why* is the first thing this observation teaches: on a client's
    // very first poll every entity is marked stale, because an unknown version differs from the
    // server's and a differing identity means refetch (`wiki/decisions/021-cache-version-identity.decision.md`).
    // Without this step every assertion below would pass for the wrong reason.
    app.poll_invalidation().await.expect("baseline");
    app.cache().mark_fresh(&"todo").await.expect("fresh todo");
    app.cache().mark_fresh(&"user").await.expect("fresh user");

    // Only the user service stops answering. `set_offline` would stop both sources, which is the
    // condition this observation is not about.
    servers.user.down(true);
    let tick = app.refresh_now().await.expect("tick");

    assert_eq!(tick.dropped.len(), 1, "exactly one source failed: {tick:?}");
    assert_eq!(tick.dropped[0].source, "user-service");
    assert_eq!(
        tick.dropped[0].entities,
        vec!["user".to_owned()],
        "and it marked its own entity, not every entity the registry models"
    );

    let todo_state = app.cache().store().state("todo").await.expect("todo state");
    let user_state = app.cache().store().state("user").await.expect("user state");
    assert!(
        !todo_state.stale,
        "the todo service answered and its version had not moved, so its source is still \
         vouching for it"
    );
    assert!(
        user_state.stale,
        "the user service did not answer, so a refetch is owed for its entity alone"
    );
    assert!(
        user_state.version.is_some(),
        "a drop means `I can no longer vouch for this`, not `I have forgotten what I knew` — the \
         last known version survives"
    );
}

/// **9. A manual source delivers what it is handed.**
///
/// The push shape, and the half polling alone would let the seam quietly stop supporting. A future
/// stream source differs from this one only in where the events come from.
#[tokio::test]
async fn observation_9_a_manual_source_delivers_what_it_is_handed() {
    let servers = common::servers().await;
    let app = common::app_on(&servers, ALICE, ALICE_USER).await;

    app.poll_invalidation().await.expect("baseline");

    let Some(Source::Manual(manual)) = app.sources().get(2) else {
        panic!("the third source is the manual one");
    };
    assert_eq!(manual.name(), "handed-in");
    manual.offer("todo", "a-version-nobody-has-seen");

    let tick = app.refresh_now().await.expect("tick");
    assert!(
        tick.changed.iter().any(|entity| entity == "todo"),
        "the handed-in event was applied through the same runner a poll uses: {tick:?}"
    );
}

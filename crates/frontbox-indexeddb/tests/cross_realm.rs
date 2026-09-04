//! Decision 031's cross-realm half, observed rather than argued.
//!
//! # What was missing, and why no conformance case could supply it
//!
//! `tests/conformance.rs` ends with `the_drain_lock_is_taken_and_released`, and that test is
//! careful to say what it cannot reach: it drains twice in **one** page, so it proves the lease is
//! taken and released and nothing about a second realm. The conformance suite has the same ceiling
//! by construction — it drives a backend through one process, and `case_61` therefore passes on a
//! backend that has done nothing at all about cross-realm exclusion.
//!
//! So the guarantee decision 031 states — *at most one drain in flight per scope across every
//! realm on the origin* — was, until this file, the only D5 promise with no witness.
//!
//! # The second realm is a dedicated worker
//!
//! Not a second tab. A tab needs a driver session that opens one, and the automation protocol that
//! runs these tests drives a single page. A dedicated worker is what is reachable from inside the
//! test, and it is sufficient for the property under test: Web Locks are managed **per origin**,
//! shared across every agent on it, and a dedicated worker is a separate agent with its own global
//! scope, its own event loop, and no view of this page's memory. That is precisely the isolation
//! two tabs have and core cannot bridge (`src/store.rs`, `claim_drain`).
//!
//! What a worker does *not* reproduce is a second **wasm instance**: the worker here is plain
//! JavaScript, so it exercises the lock manager rather than a second copy of frontbox. The two
//! tests below are built so that this costs nothing, and the argument is worth stating because it
//! is the reason one worker is enough:
//!
//! 1. `another_realm_draining_stops_this_one_from_sending` proves **our claim refuses** a lock
//!    another realm holds.
//! 2. `a_pass_in_flight_holds_the_lock_against_another_realm` proves **our pass holds** a lock
//!    another realm therefore cannot take.
//!
//! Compose them and the guarantee follows for two frontbox realms: realm A's pass holds the lock
//! (2), and realm B's claim refuses while it is held (1). Neither half is assumed.
//!
//! Both tests name the lock **from outside the crate**, by the string `locks.rs` builds privately.
//! That is deliberate: it makes the lock name a contract rather than an implementation detail. Were
//! the prefix or the scope encoding to change, test 1 would drain when it should stand down and
//! test 2 would let the worker in — so the name cannot drift silently.
//!
//! # Running these
//!
//! ```text
//! CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
//! CHROMEDRIVER=<driver matching the installed Chrome> WASM_BINDGEN_TEST_ONLY_WEB=1 \
//!   cargo test -p frontbox-indexeddb --release --target wasm32-unknown-unknown --features testing
//! ```
//!
//! The runner variable is not optional and its absence does not look like a missing runner: cargo
//! hands the `.wasm` to the shell, which reports `cannot execute binary file` and exit 126.
#![cfg(all(target_arch = "wasm32", feature = "testing"))]

use std::cell::RefCell;
use std::rc::Rc;

use frontbox::testing::{id, scope, StoreFactory};
use frontbox::{
    Error, MutationBatchRequest, MutationBatchResponse, MutationIntent, MutationResult,
    MutationStatus, OutboxStore, ScopeKey, SyncPass, SyncRunner, SyncTransport,
};
use frontbox_indexeddb::{IdbFactory, IdbStore};
use js_sys::Promise;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

/// The lock name for a scope, rebuilt here rather than imported.
///
/// `locks.rs::lock_name` is `pub(crate)`, and reaching for it would make this file assert that the
/// backend agrees with itself. Spelling the name out is what turns it into an origin-wide contract
/// that a service worker or another library could hold the other end of.
fn lock_name(key: &ScopeKey) -> String {
    format!("frontbox:drain:{}", key.as_str())
}

// The second realm, and the three things this file needs to ask of it.
//
// Bound as inline JS rather than through `web-sys` for the reason `locks.rs` gives about
// `LockManager`: the surface needed is tiny, and `Worker`/`Blob`/`Url` would each add a feature to
// a manifest whose features are otherwise all load-bearing for the backend itself.
#[wasm_bindgen(inline_js = r#"
const WORKER_SRC = `
let releaseHeld = null;
let holding = null;

self.onmessage = async (e) => {
  const { id, cmd, name } = e.data;

  if (cmd === 'hold') {
    // Take the lock and keep holding it until 'release'. A Web Lock is held for as long as the
    // callback's promise is pending, so the hold is a promise this side resolves later.
    let answered = false;
    holding = navigator.locks.request(name, { ifAvailable: true }, (lock) => {
      answered = true;
      if (!lock) { self.postMessage({ id, ok: false }); return; }
      self.postMessage({ id, ok: true });
      return new Promise((resolve) => { releaseHeld = resolve; });
    });
    holding.catch(() => { if (!answered) self.postMessage({ id, ok: false }); });
    return;
  }

  if (cmd === 'release') {
    if (releaseHeld) { releaseHeld(); releaseHeld = null; }
    // Awaited so the caller learns the lock is actually back, not merely that resolve was called.
    if (holding) { try { await holding; } catch (err) { /* nothing to salvage */ } holding = null; }
    self.postMessage({ id, ok: true });
    return;
  }

  if (cmd === 'probe') {
    // Ask for the lock without waiting, and give it straight back. 'ok' answers the only question
    // this file asks of the worker: was it free?
    let taken = false;
    await navigator.locks.request(name, { ifAvailable: true }, (lock) => { taken = !!lock; });
    self.postMessage({ id, ok: taken });
  }
};
`;

let worker = null;
let nextId = 1;
const pending = new Map();

export function startRealm() {
  if (worker) return;
  const blob = new Blob([WORKER_SRC], { type: 'text/javascript' });
  // A blob URL inherits this page's origin, which is what puts the worker on the same lock manager.
  worker = new Worker(URL.createObjectURL(blob));
  worker.onmessage = (e) => {
    const { id, ok } = e.data;
    const resolve = pending.get(id);
    if (resolve) { pending.delete(id); resolve(ok); }
  };
}

function call(msg) {
  const id = nextId++;
  return new Promise((resolve) => {
    pending.set(id, resolve);
    worker.postMessage(Object.assign({ id }, msg));
  });
}

export function realmTakesLock(name) { return call({ cmd: 'hold', name }); }
export function realmDropsLock() { return call({ cmd: 'release' }); }
export function realmCouldTakeLock(name) { return call({ cmd: 'probe', name }); }
"#)]
extern "C" {
    #[wasm_bindgen(js_name = startRealm)]
    fn start_realm();
    #[wasm_bindgen(js_name = realmTakesLock)]
    fn realm_takes_lock(name: &str) -> Promise;
    #[wasm_bindgen(js_name = realmDropsLock)]
    fn realm_drops_lock() -> Promise;
    #[wasm_bindgen(js_name = realmCouldTakeLock)]
    fn realm_could_take_lock(name: &str) -> Promise;
}

/// Await one of the worker's answers.
async fn answer(promise: Promise) -> bool {
    JsFuture::from(promise)
        .await
        .expect("the worker answers every message it is sent")
        .as_bool()
        .expect("every answer is a boolean")
}

/// Queue `count` mutations under `key`, in a database this file owns.
///
/// The factory comes back with the store and both tests hold it, because `IdbFactory::drop` closes
/// the connection and deletes the database — test scaffolding that keeps a browser profile from
/// filling up with one database per case. Dropping it here instead would leave the store holding a
/// closing connection, which surfaces one call later as `InvalidStateError` rather than as
/// anything that names the cause.
async fn queued(db: &str, key: &ScopeKey, count: u128) -> (IdbFactory, IdbStore) {
    let factory = IdbFactory::open(db).await.expect("open");
    let store = factory.open(key.clone()).await.expect("scope");
    for n in 1..=count {
        store
            .enqueue(MutationIntent::new(
                id(n),
                "POST",
                "/api/v1/things",
                serde_json::json!({ "n": n }),
                1_700_000_000_000 + n as i64,
            ))
            .await
            .expect("enqueue");
    }
    (factory, store)
}

/// A transport that answers every mutation `Applied`, and asks the other realm one question first.
///
/// The probe happens **inside** `send_batch`, which is the whole trick: the runner claims the lease
/// before it builds a batch and drops it after the outcomes are applied, so any moment inside this
/// method is a moment the pass provably holds the lock. No second future, no timing assumption, and
/// nothing that could pass by racing.
struct ProbingTransport {
    lock: String,
    /// Whether the other realm got the lock while this pass held it. `None` until the pass runs,
    /// which is itself asserted — a transport that was never called would otherwise read as a pass.
    realm_got_in: Rc<RefCell<Option<bool>>>,
}

impl SyncTransport for ProbingTransport {
    async fn send_batch(
        &self,
        request: MutationBatchRequest,
    ) -> Result<MutationBatchResponse, Error> {
        *self.realm_got_in.borrow_mut() = Some(answer(realm_could_take_lock(&self.lock)).await);
        Ok(MutationBatchResponse::new(
            request
                .mutations
                .iter()
                .map(|m| MutationResult::new(m.mutation_id, MutationStatus::Applied))
                .collect(),
        ))
    }
}

/// **A drain in another realm stops this one from sending.**
///
/// The half of decision 031 that a single realm has never been able to fail. The worker holds the
/// scope's lock exactly as a second tab mid-drain would, and this realm must stand down: report
/// `AlreadyRunning`, send nothing, and leave the queue where it was.
///
/// "Leave the queue where it was" is the assertion that matters most. `AlreadyRunning` with an
/// emptied queue would be the double-send this mechanism exists to prevent, wearing the right
/// label.
#[wasm_bindgen_test::wasm_bindgen_test]
async fn another_realm_draining_stops_this_one_from_sending() {
    start_realm();

    let key = scope("user:refused@tenant:acme");
    let (_factory, store) = queued("frontbox-cross-realm-refused", &key, 2).await;
    let lock = lock_name(&key);

    assert!(
        answer(realm_takes_lock(&lock)).await,
        "the other realm must actually get the lock, or this test asserts nothing"
    );

    let runner = SyncRunner::new(
        store,
        ProbingTransport {
            lock: lock.clone(),
            // Unused here: reaching the transport at all is the failure this test is about.
            realm_got_in: Rc::new(RefCell::new(None)),
        },
    );

    let refused = runner
        .sync_once()
        .await
        .expect("a refused pass is not an error");
    assert_eq!(
        refused.pass,
        SyncPass::AlreadyRunning,
        "another realm holds this scope's drain lock"
    );
    assert_eq!(refused.sent, 0, "nothing may go to the server");
    assert_eq!(
        runner.store().pending_count().await.expect("count"),
        2,
        "and the work must still be queued — an emptied queue behind AlreadyRunning is the \
         double-send this mechanism exists to prevent"
    );

    // The other side of the same guarantee: the exclusion must end when the other realm is done,
    // or one abandoned tab wedges the scope for every future one.
    assert!(answer(realm_drops_lock()).await, "the worker releases");

    let allowed = runner.sync_once().await.expect("second pass");
    assert_eq!(
        allowed.pass,
        SyncPass::Completed,
        "with the other realm finished, this one drains"
    );
    assert_eq!(allowed.counts.applied, 2);
    assert_eq!(runner.store().pending_count().await.expect("count"), 0);
}

/// **A pass in flight holds the lock against another realm.**
///
/// The converse, and the half that stops the mechanism from being a no-op. A backend that claimed
/// nothing — or claimed a lock under a name no other realm would ask for — passes every conformance
/// case, passes `the_drain_lock_is_taken_and_released`, and passes the test above, because all
/// three only ever observe *this* realm standing down.
///
/// So this one asks the other realm to try, at the one instant it must fail.
#[wasm_bindgen_test::wasm_bindgen_test]
async fn a_pass_in_flight_holds_the_lock_against_another_realm() {
    start_realm();

    let key = scope("user:holder@tenant:acme");
    let (_factory, store) = queued("frontbox-cross-realm-holder", &key, 1).await;
    let lock = lock_name(&key);

    assert!(
        answer(realm_could_take_lock(&lock)).await,
        "before any drain the scope is free, so a later refusal is caused by the drain"
    );

    let realm_got_in = Rc::new(RefCell::new(None));
    let runner = SyncRunner::new(
        store,
        ProbingTransport {
            lock: lock.clone(),
            realm_got_in: Rc::clone(&realm_got_in),
        },
    );

    let report = runner.sync_once().await.expect("pass");
    assert_eq!(report.pass, SyncPass::Completed);
    assert_eq!(report.counts.applied, 1);

    assert_eq!(
        *realm_got_in.borrow(),
        Some(false),
        "while this pass held the scope, the other realm asked for the same lock and had to be \
         refused; `None` here means the transport never ran and the test proved nothing"
    );

    // And the hold is for the pass, not forever.
    assert!(
        answer(realm_could_take_lock(&lock)).await,
        "the lease released with the pass, so the scope is free for whichever realm drains next"
    );
}

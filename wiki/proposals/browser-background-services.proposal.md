# Browser Background Services: What frontbox Can Use, And What It Cannot Yet

Document Class: Proposal
Status: Proposed; §5's bfcache wake built 2026-08-30 as register entry 16, and §4's finding built as decision 031's in-realm half
Date: 2026-08-30
Category: Platform Integration
Scope: Whether frontbox should use Background Sync, Background Fetch, or the back/forward cache; why the browser's storage panel shows nothing today; and the cross-realm finding the question surfaced.
Sources: `examples/todo-core/src/app/mod.rs`, `examples/todo-app/src/main.rs`, `examples/todo-app/src/sync.rs`, `src/memory/mod.rs`, `src/runner/mod.rs`, `crates/frontbox-dioxus/src/sync/mod.rs`
Related: `wiki/decisions/031-cross-realm-single-flight.decision.md`, `wiki/decisions/018-single-flight-drain-mode.decision.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/plans/d4a-offline-todo-trial.plan.md`, `wiki/roadmaps/extraction.roadmap.md`, `wiki/references/open-decisions.reference.md`

## 0. Summary

Chrome DevTools shows three background-service panels and a storage panel. The question was whether
frontbox can use any of them, including for debugging, and why the storage panel is empty.

| Surface | Verdict |
| --- | --- |
| **Storage panel is empty** | Correct, not a bug. Nothing is persisted — the trial runs on `InMemoryBackend` |
| **Background Fetch** | **Rejected.** Built for large user-visible transfers; our payloads are small JSON POSTs |
| **Background Sync** | **Right shape, deferred.** Strictly downstream of D5, and gated on decision 031 |
| **Back/forward cache** | **Not a feature to adopt; a hazard to handle.** Register entry 16 |
| **Network → Offline throttle** | **Already works**, unchanged, and exercises the real offline path |

And one finding that outgrew the question: **single-flight held per runner, not per scope**, which
becomes a live defect the moment storage is durable and does not require a service worker to reach.
That is decision 031 — **the in-realm half was built the same day**, and what remains is the
cross-realm obligation on D5's backends. It is the reason this page exists rather than a short
answer.

## 1. Why The Storage Panel Is Empty

`examples/todo-core/src/app/mod.rs:57` constructs `InMemoryBackend::new(clock)`, and that is the
only backend the trial has. The queue lives in an `Rc<RefCell<State>>` inside the page's wasm linear
memory. There is no IndexedDB, no localStorage, no Cache Storage, and no service worker anywhere in
the tree — every IndexedDB mention in this repository is a wiki page or a doc comment about D5.

So the panel is reporting accurately. **D4a demonstrates offline tolerance within a session**, which
is the line its own proposal drew; reload the tab and the queue is gone. D4b is the other half and
depends on D5.

This is worth stating plainly because the empty panel looks like a defect and is instead the most
compact possible statement of what has and has not been built.

## 2. Background Fetch — Rejected

`BackgroundFetchManager` exists for transfers the user should be able to watch and resume: large
downloads and uploads, with browser-supplied progress UI and a notification. It survives page
closure, which is the property that makes it look relevant.

It is the wrong instrument. A mutation batch is a small JSON POST, and attaching a
browser-chrome download indicator to it would misrepresent the operation to the user. Nothing in the
outbox needs resumable transfer semantics; it needs *retry* semantics, which frontbox already owns
through decisions 005 and 017.

## 3. Background Sync — Right Shape, Blocked On Two Things

One-shot Background Sync — `registration.sync.register(tag)`, with a `sync` event delivered to the
service worker when the browser believes connectivity returned — is conceptually a good match for an
outbox. It is also the only mechanism here that could drain a queue after the tab is closed.

Three obstacles, in increasing order of interest.

**It needs a shared durable store, so it is downstream of D5.** A service worker is a separate
JavaScript realm. It cannot read the page's wasm linear memory — not with difficulty, categorically.
The queue has to live somewhere both realms can open, which is IndexedDB, which is D5's web backend.
There is no ordering in which Background Sync lands first.

**It is Chromium-only.** Absent from Safari, and not shipped by Firefox. So it can never be the only
drain path; it would always be an accelerator over the in-page cadence loop, which doubles the
number of drain paths that have to agree with each other.

**It makes a second drainer the normal case, not a mistake.** This is the substantive one, and it is
covered in §4.

A fourth, smaller point worth recording: the browser retries a failed `sync` event on its own
schedule and eventually gives up. That is a **third retry policy** stacked on decision 017's
`attempts` bound and the cadence's own backoff. Three budgets over one record, only one of which
frontbox can observe, is the kind of arrangement that produces bug reports nobody can reproduce.

## 4. The Finding: Single-Flight Is Per Runner, Not Per Scope

Investigating the service-worker path surfaced something that needs no service worker.

`in_flight` is a `Cell<bool>` on `SyncRunner` (`src/runner/mod.rs:46`), while
`InMemoryBackend::open` (`src/memory/mod.rs:142`) returns a handle over shared state
(`src/memory/mod.rs:113`). Two handles on one scope, two runners, two flags, one queue. Both read
the same pending batch and both send it — and because a drain pass is *read* → *POST* → *apply*,
with two non-overlapping transactions, storage atomicity cannot prevent it.

**The D4a plan recorded this as a near-miss a developer could write.** With a durable store it stops
requiring anyone to write anything: two browser tabs on one origin are two realms over one IndexedDB
database. The user opens a second tab. A Background Sync service worker would be a third realm.

What breaks is specific — idempotency survives, ordering and the retention bound do not — and the
full analysis, the rejected alternatives, and the obligation now placed on backends are in
`wiki/decisions/031-cross-realm-single-flight.decision.md`.

**This reframes Background Sync entirely.** It is not a feature to schedule after D5; it is a
consumer of a guarantee D5 has to establish anyway for the two-tab case. Deferring it costs nothing
as long as decision 031 is honoured, and building it before decision 031 would silently break
ordering.

## 5. Back/Forward Cache — A Hazard, Not A Feature

There is nothing to register for. bfcache is something a page is eligible for, and DevTools' panel
exists to report why a page was not.

The frontbox-relevant behaviour is that **a bfcached page is frozen**: timers do not fire, so the
`Sleeper` future backing `SyncCadence` (`crates/frontbox-dioxus/src/sync/mod.rs`) stalls. On restore the
loop resumes as though its wait had been continuous, while `Date.now()` — which is what the trial's
`WebClock` reads — has jumped forward by however long the page sat in the cache. A user who
navigates away for ten minutes and comes back gets a client that believes it is mid-sleep.

~~The fix is small: drain on `pageshow` when `event.persisted` is true~~ **Built 2026-08-30**, with
entries 14 and 15 as the register's note 6 argued. It went on the *wait* rather than the loop —
`Wake` is a latch and `Sleeper::wakeable` races it against the sleep — so `use_sync_loop`'s
signature did not have to change twice in one release.

**Measured rather than asserted.** Chromium under an automation protocol will not actually
back/forward-cache a page, so the restore event was synthesised and everything downstream of it is
the real path: with a write queued and the app just back online, the queue drained **38 ms** after
the event against **15 015 ms** for the same sequence without it — `offline_ms` served out in full.
That gap is the feature.

Nothing in the current setup should *block* eligibility: `examples/todo-app/nginx.conf` sets no
`Cache-Control: no-store`, and there is no `unload` handler. That is worth confirming with the
panel's own test button rather than asserting.

## 6. What Is Usable Today

**Network → Offline works, unchanged, and it is not a coincidence.** The throttle makes `fetch`
reject; reqwest's wasm client reports that as `Kind::Request`; `map_send_error` in
`examples/todo-core/src/transport/mod.rs` maps `is_request()` to `Error::Offline`; the runner returns
`SyncPass::Offline` and retains the queue. That mapping was written during D4a — see the plan's
`## Finding 5` — and the DevTools toggle is the cheapest way to exercise it.

**A token service worker registered only to populate the DevTools panels is not worth building.** It
would record an empty Background Sync stream, because it has nothing to say about a queue it cannot
see. The panels are not a debugging affordance for us until there is something in the shared store
for a worker to act on.

**The real DevTools win arrives with D5, for free.** An IndexedDB outbox is a live table in the
Application panel: pending rows with their `seq` and `attempts`, dead letters, quarantine. No code
is needed to get it. What it does argue for is a **legible schema** — which reinforces a constraint
decision 009 already imposes for ordering reasons, that `mutation_id` is stored as its
`MutationId::to_string` text rather than as bytes. A second, independent reason for a rule already
adopted is the cheapest kind of confirmation available.

## 7. Recommendation

1. **Do not build a service worker now.** Record why, which is this page.
2. ~~**Adopt decision 031**, and let D5 discharge it.~~ **Adopted and half built.** Core claims the
   scope in-realm; D5 discharges the cross-realm half, and owes a two-realm fixture to observe it,
   because a conformance suite drives a backend through one process.
3. ~~**Register the bfcache wake-up** as entry 16~~ **Built**, batched with 14 and 15 as intended.
4. **Revisit Background Sync after D5 and D4b**, at which point it is a bounded question — one lock
   name, one worker entry point, and `postMessage` for reports — rather than an architecture change.

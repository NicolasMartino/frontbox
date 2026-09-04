//! Cross-realm drain exclusion, through the Web Locks API.
//!
//! # The half core cannot reach
//!
//! Decision 031 requires at most one drain in flight per scope. Core's scope registry closes that
//! within a realm; two browser tabs are two wasm instances with two registries and no view of each
//! other. Web Locks is the platform's answer: a lock name is origin-wide, so a lock taken in one
//! tab is visible to every other tab, worker and service worker on that origin — which is also
//! what would later let Background Sync participate rather than race
//! (`wiki/proposals/browser-background-services.proposal.md`).
//!
//! # `ifAvailable`, not waiting
//!
//! The request is made with `ifAvailable: true`, so a busy scope hands back `null` immediately
//! instead of queueing. That is the behaviour the runner already has words for: a pass that cannot
//! claim the scope reports `AlreadyRunning` and does nothing. Waiting would be worse than useless —
//! it would turn a cadence into a queue of stacked drains, each firing as the previous one ends.
//!
//! # Bound here rather than taken from `web-sys`
//!
//! `web_sys::LockManager` exists but sits behind `--cfg=web_sys_unstable_apis`, which is a
//! *workspace-wide* rustflag. Setting it would change how every crate in the tree compiles in
//! order to reach one method. The API surface actually needed is a single call, so it is bound
//! directly and the build stays ordinary.

use std::cell::RefCell;
use std::rc::Rc;

use frontbox::{DrainLease, Error, ScopeKey};
use js_sys::{Function, Object, Promise, Reflect};
use wasm_bindgen::prelude::*;

/// The lock name for one scope.
///
/// The scope key verbatim behind a fixed prefix. Decision 024 asks that two distinct `ScopeKey`s
/// never collide in physical storage; the same argument applies to a lock name and the same answer
/// works — two distinct keys are two distinct strings, so no encoding can merge them. The prefix
/// keeps frontbox's locks out of the application's namespace.
fn lock_name(scope: &ScopeKey) -> String {
    format!("frontbox:drain:{}", scope.as_str())
}

/// Try to take the drain lock for `scope`, without waiting.
///
/// `Ok(None)` means another realm holds it.
pub(crate) async fn claim(scope: &ScopeKey) -> Result<Option<DrainLease>, Error> {
    // Two attempts, with a task turn between them, and the second one is not belt-and-braces.
    //
    // **Releasing a Web Lock is asynchronous.** The lease resolves the callback's promise on drop,
    // but the lock manager hands the lock back a turn later. A drain is a loop of passes, so pass
    // N+1 asks for the lock microseconds after pass N released it and is told it is still held —
    // which made every drain stop after one pass and failed seven conformance cases.
    //
    // One `setTimeout(0)` is enough to let that release land, and it does not weaken the guarantee
    // it exists for: a lock genuinely held by another tab is held for the length of that tab's
    // pass, which is a network round trip, not a task turn. So the retry distinguishes "my own
    // release has not landed yet" from "somebody else is draining", which is exactly the
    // distinction the claim has to make.
    if let Some(lease) = try_claim(scope).await? {
        return Ok(Some(lease));
    }
    yield_task().await?;
    try_claim(scope).await
}

/// Resolve after one macrotask, giving a pending lock release time to land.
async fn yield_task() -> Result<(), Error> {
    let global = js_sys::global();
    let Ok(set_timeout) = Reflect::get(&global, &JsValue::from_str("setTimeout")) else {
        return Ok(());
    };
    let Ok(set_timeout) = set_timeout.dyn_into::<Function>() else {
        return Ok(());
    };
    let promise = Promise::new(&mut |resolve, _reject| {
        let _ = set_timeout.call2(&JsValue::NULL, &resolve, &JsValue::from_f64(0.0));
    });
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(crate::request::js_error)?;
    Ok(())
}

async fn try_claim(scope: &ScopeKey) -> Result<Option<DrainLease>, Error> {
    let Some((locks, request)) = manager() else {
        // No Web Locks here. The in-realm claim still holds, so this degrades to the guarantee
        // every other backend offers rather than to nothing — and says so rather than pretending
        // to the stronger one.
        return Ok(Some(DrainLease::granted()));
    };

    // A Web Lock is held for as long as the callback's promise is pending; there is no imperative
    // release. So the callback returns a promise this side controls, and the lease resolves it.
    let release: Rc<RefCell<Option<Function>>> = Rc::new(RefCell::new(None));
    let granted = Rc::new(RefCell::new(false));

    // A second promise, resolved by the callback in *both* branches, purely to tell this function
    // that the callback has run.
    //
    // A bare microtask turn is not enough and the first version of this made that mistake: the
    // lock manager invokes the callback from a task, so a `Promise::resolve` checkpoint returns
    // long before it fires. Every claim then read `granted == false`, reported `AlreadyRunning`,
    // and 38 conformance cases failed at once — a mechanism meant to prevent double drains
    // preventing all of them.
    let mut signal_resolve: Option<Function> = None;
    let signal = Promise::new(&mut |resolve, _reject| signal_resolve = Some(resolve));
    let signal_resolve = Rc::new(RefCell::new(signal_resolve));

    let callback = {
        let (release, granted, signal_resolve) = (
            Rc::clone(&release),
            Rc::clone(&granted),
            Rc::clone(&signal_resolve),
        );
        Closure::<dyn FnMut(JsValue) -> Promise>::new(move |lock: JsValue| {
            let refused = lock.is_null() || lock.is_undefined();
            if !refused {
                *granted.borrow_mut() = true;
            }
            if let Some(resolve) = signal_resolve.borrow_mut().take() {
                let _ = resolve.call0(&JsValue::UNDEFINED);
            }
            if refused {
                // `ifAvailable` said no. Settle at once so nothing is held.
                return Promise::resolve(&JsValue::UNDEFINED);
            }
            let release = Rc::clone(&release);
            Promise::new(&mut move |resolve, _reject| {
                *release.borrow_mut() = Some(resolve);
            })
        })
    };

    let options = Object::new();
    Reflect::set(&options, &JsValue::from_str("ifAvailable"), &JsValue::TRUE)
        .map_err(crate::request::js_error)?;

    request
        .call3(
            &locks,
            &JsValue::from_str(&lock_name(scope)),
            &options,
            callback.as_ref(),
        )
        .map_err(crate::request::js_error)?;

    // Awaited until the callback has run, and no further.
    //
    // The promise `request` returned is deliberately *not* awaited: it does not settle until the
    // hold ends, and the hold ends when the lease is dropped — which has not been created yet.
    // Awaiting it would deadlock on this function's own return value.
    wasm_bindgen_futures::JsFuture::from(signal)
        .await
        .map_err(crate::request::js_error)?;

    if !*granted.borrow() {
        return Ok(None);
    }

    Ok(Some(DrainLease::held(move || {
        if let Some(resolve) = release.borrow_mut().take() {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        }
        // Held until here on purpose: dropping the closure while the lock manager still holds a
        // reference to it would invalidate the function it is waiting on.
        drop(callback);
    })))
}

/// `navigator.locks` and its `request`, from a window or a worker.
fn manager() -> Option<(JsValue, Function)> {
    let global = js_sys::global();
    let navigator = Reflect::get(&global, &JsValue::from_str("navigator")).ok()?;
    if navigator.is_undefined() || navigator.is_null() {
        return None;
    }
    let locks = Reflect::get(&navigator, &JsValue::from_str("locks")).ok()?;
    if locks.is_undefined() || locks.is_null() {
        return None;
    }
    let request = Reflect::get(&locks, &JsValue::from_str("request"))
        .ok()?
        .dyn_into::<Function>()
        .ok()?;
    Some((locks, request))
}

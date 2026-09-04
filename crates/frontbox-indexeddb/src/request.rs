//! Turning IndexedDB's callback API into futures, without letting a transaction die.
//!
//! # The rule this module exists to make followable
//!
//! An IndexedDB transaction is active only while a request against it is outstanding. It goes
//! inactive the moment control returns to the event loop with nothing pending, and any further use
//! throws `TransactionInactiveError`. So a method that must be atomic may await **IDB requests and
//! nothing else** — no timers, no `fetch`, no channels resolved by anything but a request's own
//! callback.
//!
//! `await_request` honours that: the future resolves from `onsuccess`/`onerror`, which fire as
//! part of the transaction's own lifecycle, so issuing the next request from the resumed task
//! keeps the transaction alive. Nothing here ever awaits a promise the transaction does not own.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use frontbox::Error;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{IdbRequest, IdbTransaction, IdbTransactionMode};

/// What a pending request has produced, if anything yet.
struct State {
    result: Option<Result<JsValue, Error>>,
    waker: Option<Waker>,
}

/// A future over one IndexedDB request.
pub(crate) struct RequestFuture {
    state: Rc<RefCell<State>>,
    /// The request the handlers below are attached to, so [`Drop`] can take them off again.
    ///
    /// `None` for [`Txn`]'s completion, whose handlers live on the *transaction* and are detached
    /// by [`Txn::drop`]. One `Option` rather than two types, because the two differ only in which
    /// object holds the callbacks.
    source: Option<IdbRequest>,
    // Held so the closures outlive the request. Dropping them early would leave the callbacks
    // dangling and the future would never wake.
    _on_success: Closure<dyn FnMut()>,
    _on_error: Closure<dyn FnMut()>,
}

impl Drop for RequestFuture {
    fn drop(&mut self) {
        // **The same rule `Txn::drop` follows, for the same reason.** The browser holds pointers to
        // these closures until they are detached, and the closures are freed when this struct's
        // fields drop. A request that settles *after* the future is dropped therefore calls a
        // `Closure` the Rust side has already freed, which throws `closure invoked recursively or
        // after being dropped` inside the browser's event dispatch — raised in a handler, so no
        // future sees it and no test goes red. It is an uncaught console error and nothing else.
        //
        // Nothing in this crate cancels a request today: every call site is
        // `await_request(..).await?` in one expression, so the future is dropped only after it has
        // settled and detaching is a no-op. That is a property of the callers, not of this type,
        // and it is exactly the property that quietly stopped holding for read transactions once
        // something dropped one without committing.
        //
        // `Drop::drop` runs before the fields drop, so the handlers are off before the closures go.
        if let Some(request) = &self.source {
            request.set_onsuccess(None);
            request.set_onerror(None);
        }
    }
}

impl Future for RequestFuture {
    type Output = Result<JsValue, Error>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.borrow_mut();
        match state.result.take() {
            Some(result) => Poll::Ready(result),
            None => {
                state.waker = Some(context.waker().clone());
                Poll::Pending
            }
        }
    }
}

/// Await one request's outcome.
///
/// The error carries the browser's own `DOMException` name where there is one — `QuotaExceededError`
/// and `TransactionInactiveError` are the two an operator actually needs to tell apart, and a bare
/// "storage failure" would hide both.
pub(crate) fn await_request(request: IdbRequest) -> RequestFuture {
    let state = Rc::new(RefCell::new(State {
        result: None,
        waker: None,
    }));

    let on_success = {
        let (state, request) = (Rc::clone(&state), request.clone());
        Closure::<dyn FnMut()>::new(move || {
            let outcome = request.result().map_err(js_error);
            let mut state = state.borrow_mut();
            state.result = Some(outcome);
            if let Some(waker) = state.waker.take() {
                waker.wake();
            }
        })
    };

    let on_error = {
        let (state, request) = (Rc::clone(&state), request.clone());
        Closure::<dyn FnMut()>::new(move || {
            let reason = request
                .error()
                .ok()
                .flatten()
                .map(|exception| format!("{}: {}", exception.name(), exception.message()))
                .unwrap_or_else(|| "indexeddb request failed".to_owned());
            let mut state = state.borrow_mut();
            state.result = Some(Err(Error::storage_message(reason)));
            if let Some(waker) = state.waker.take() {
                waker.wake();
            }
        })
    };

    request.set_onsuccess(Some(on_success.as_ref().unchecked_ref()));
    request.set_onerror(Some(on_error.as_ref().unchecked_ref()));

    RequestFuture {
        state,
        source: Some(request),
        _on_success: on_success,
        _on_error: on_error,
    }
}

/// A transaction with its completion already armed.
///
/// # Why the handlers go on before any request is issued
///
/// A transaction completes on its own as soon as its last request settles and control returns to
/// the event loop. Attaching `oncomplete` *after* the final `await_request` therefore races: if the
/// transaction has already finished, the handler lands on a dead object and the completion future
/// never resolves. The suite hangs rather than fails, which is the worst way for a bug to present.
///
/// Found by running the conformance suite in a real browser, and it is exactly the class of defect
/// a compile-only gate cannot reach — every method involved type-checks perfectly.
/// # Why dropping one aborts it
///
/// Dropping a `rusqlite::Transaction` rolls it back. Dropping an `IdbTransaction` handle does the
/// opposite: the browser owns the transaction, and it commits as soon as it goes inactive. So on a
/// write transaction every `?` between the first request and the commit is a partial write that
/// lands — an unrecognised `Disposition`, a `to_js` failure, a request that errors after earlier
/// records in the loop have already been deleted and reinserted. Precisely the torn state
/// `apply_outcomes` exists to make unrepresentable
/// (`wiki/decisions/003-atomic-outcome-application.decision.md`).
///
/// The fault-injection path knew this and aborted by hand; nothing else did, and "remember to abort
/// on every early return" is not a rule a reviewer can hold. So the type holds it: a write
/// transaction that is dropped without [`commit`](Txn::commit) aborts, and `?` is safe again. This
/// is the same device [`DrainLease`](frontbox::DrainLease) uses in core for the same reason —
/// release on drop, whatever ends the pass.
///
/// Read transactions are exempt. They have nothing to roll back, and a reader that returns its rows
/// without ceremony is the shape every `list` here wants.
pub(crate) struct Txn {
    transaction: IdbTransaction,
    completion: Option<RequestFuture>,
    /// Set for write transactions, cleared once `commit` has taken responsibility.
    abort_on_drop: bool,
}

impl Drop for Txn {
    fn drop(&mut self) {
        // **Handlers off on every path, not just the aborting one.**
        //
        // `completion` owns the `oncomplete`/`onabort` closures and frees them when this struct
        // drops. The browser is still holding pointers to them until they are detached, so a
        // transaction that settles *after* the drop calls a `Closure` the Rust side has already
        // freed — which throws `closure invoked recursively or after being dropped` inside the
        // browser's event dispatch.
        //
        // That is not a hypothetical. Detaching used to happen only in the abort branch, so it
        // covered write transactions and missed every **read**: a reader is dropped without
        // `commit`, its closures go, and the transaction then completes normally and calls one.
        // Nothing failed — the exception is raised inside the event handler, so no future sees it
        // and the conformance suite stayed green — it just threw an uncaught error per read
        // transaction, forever. Found by driving the trial UI in a browser and reading the console,
        // which is a thing no gate here does.
        //
        // Field drop order is what makes this correct: `Drop::drop` runs before the fields are
        // dropped, so the handlers are off the transaction before the closures behind them go.
        self.transaction.set_oncomplete(None);
        self.transaction.set_onabort(None);
        if self.abort_on_drop {
            // `abort()` is asynchronous, which is the second reason the detach above has to come
            // first: the abort event would otherwise land on freed closures too.
            let _ = self.transaction.abort();
        }
    }
}

impl Txn {
    /// Arm the completion handlers on a fresh transaction.
    pub fn new(transaction: IdbTransaction, mode: IdbTransactionMode) -> Self {
        let state = Rc::new(RefCell::new(State {
            result: None,
            waker: None,
        }));

        let on_complete = {
            let state = Rc::clone(&state);
            Closure::<dyn FnMut()>::new(move || settle(&state, Ok(JsValue::UNDEFINED)))
        };
        let on_abort = {
            let state = Rc::clone(&state);
            Closure::<dyn FnMut()>::new(move || {
                settle(
                    &state,
                    Err(Error::storage_message("indexeddb transaction aborted")),
                )
            })
        };

        transaction.set_oncomplete(Some(on_complete.as_ref().unchecked_ref()));
        transaction.set_onabort(Some(on_abort.as_ref().unchecked_ref()));

        Self {
            transaction,
            completion: Some(RequestFuture {
                state,
                // The transaction owns these handlers, and `Txn::drop` below takes them off.
                source: None,
                _on_success: on_complete,
                _on_error: on_abort,
            }),
            abort_on_drop: mode == IdbTransactionMode::Readwrite,
        }
    }

    /// The underlying transaction, for opening object stores.
    pub fn inner(&self) -> &IdbTransaction {
        &self.transaction
    }

    /// Abort now, rather than at the end of the enclosing scope.
    ///
    /// Redundant with the [`Drop`] above and kept anyway, at the one site that exists to prove the
    /// rollback happens: injecting a failure and then *saying* abort is what makes case 09 and case
    /// 50 read as assertions about atomicity rather than about scope exit.
    pub fn abort(self) {
        drop(self);
    }

    /// Wait for the commit to actually land.
    ///
    /// **Awaited rather than assumed.** A method returning as soon as its last request succeeded
    /// would report success for writes the browser can still abort — on a quota failure, say.
    /// Promising atomicity means promising the commit landed.
    pub async fn commit(mut self) -> Result<(), Error> {
        // Cleared before the await, not after: if this future is cancelled mid-commit the browser
        // has already been asked to commit, and aborting from `Drop` at that point would undo a
        // transaction the caller correctly finished.
        self.abort_on_drop = false;
        let completion = self
            .completion
            .take()
            .expect("commit consumes the transaction, so the completion is always present");
        completion.await.map(|_| ())
    }
}

/// Publish a result and wake whoever is waiting.
fn settle(state: &Rc<RefCell<State>>, result: Result<JsValue, Error>) {
    let mut state = state.borrow_mut();
    state.result = Some(result);
    if let Some(waker) = state.waker.take() {
        waker.wake();
    }
}

/// Turn a JavaScript exception into core's storage error.
pub(crate) fn js_error(value: JsValue) -> Error {
    let described = value
        .dyn_ref::<web_sys::DomException>()
        .map(|exception| format!("{}: {}", exception.name(), exception.message()))
        .or_else(|| value.as_string())
        .unwrap_or_else(|| "indexeddb failure".to_owned());
    Error::storage_message(described)
}

//! The two browser seams this crate owns, behind the `web` feature.
//!
//! Core points here for both. [`Clock`](frontbox::Clock) is injected because core has no timer of
//! its own, and `SystemClock` is native-only — its own documentation says a web caller supplies a
//! clock over `Date.now()` "through their adapter". This crate is that adapter and supplied none,
//! so every browser consumer wrote the same five lines and took the same `js-sys` dependency
//! (`wiki/references/open-decisions.reference.md`, entry 15).
//!
//! The second seam is the page lifecycle, which is the one that decides whether this feature is a
//! convenience or the point of the crate (entry 16).

use std::rc::Rc;

use dioxus_core::use_hook;
use frontbox::Clock;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;
use web_sys::PageTransitionEvent;

use crate::sync::Wake;

/// A [`Clock`] over the browser's `Date.now()`.
///
/// Reads epoch milliseconds, truncating the fractional part `Date.now()` may carry. What it stamps
/// is `created_at`, which decision 016 removed from the ordering path — `seq` is the primary sort
/// key — so a clock that jumps or drifts costs a wrong timestamp on a record rather than a wrong
/// order in the queue.
#[derive(Debug, Clone, Copy, Default)]
pub struct WebClock;

impl Clock for WebClock {
    fn now_ms(&self) -> i64 {
        js_sys::Date::now() as i64
    }
}

/// A [`Wake`] that fires when this page is restored from the back/forward cache.
///
/// # Why a restored page needs waking
///
/// A page in the back/forward cache is **frozen**: its timers do not fire. The future behind
/// [`Sleeper`](crate::Sleeper) therefore stalls for the whole time the page sits in the cache, and
/// on restore the loop resumes as though its wait had been continuous — while the wall clock has
/// jumped forward by however long the user was away. A person who navigates off for ten minutes and
/// comes back gets a client that believes it is mid-sleep and will not sync until an interval
/// chosen for a live page runs out.
///
/// `pageshow` with `persisted` set is the documented restore signal, and it is the only one that
/// distinguishes a restore from an ordinary load — an ordinary load has a fresh loop anyway.
///
/// Hand it to [`Sleeper::wakeable`](crate::Sleeper::wakeable). The listener is removed when the
/// calling component is dropped.
pub fn use_bfcache_wake() -> Wake {
    use_hook(|| {
        let wake = Wake::new();
        Registration {
            wake: wake.clone(),
            listener: Rc::new(PageshowListener::register(wake)),
        }
    })
    .wake
}

/// The hook's state: the wake to hand out, and the listener's lifetime.
///
/// `Rc` around the listener because `use_hook` hands back a *clone* of its state on every render,
/// and those clones are dropped at the end of it. A bare listener would unregister itself on the
/// first re-render; a reference-counted one unregisters when the component is dropped, which is
/// what was meant.
#[derive(Clone)]
struct Registration {
    wake: Wake,
    #[allow(dead_code, reason = "held for its Drop, which removes the listener")]
    listener: Rc<PageshowListener>,
}

/// A `pageshow` handler, kept alive for as long as the browser may call it.
type PageshowHandler = Closure<dyn FnMut(PageTransitionEvent)>;

/// A `pageshow` listener that removes itself when dropped.
struct PageshowListener {
    /// `None` when there is no window to listen on, which is not an error worth panicking over:
    /// a Dioxus build can run this tree outside a browser, and the honest result is a wake that
    /// never fires rather than a crash on mount.
    registered: Option<(web_sys::Window, PageshowHandler)>,
}

impl PageshowListener {
    fn register(wake: Wake) -> Self {
        let Some(window) = web_sys::window() else {
            return Self { registered: None };
        };
        let handler = PageshowHandler::new(move |event: PageTransitionEvent| {
            // Only a restore. An ordinary load already starts a fresh loop, and waking that one
            // would drain on mount for a reason that has nothing to do with the cache.
            if event.persisted() {
                wake.wake();
            }
        });
        if window
            .add_event_listener_with_callback("pageshow", handler.as_ref().unchecked_ref())
            .is_err()
        {
            return Self { registered: None };
        }
        Self {
            registered: Some((window, handler)),
        }
    }
}

impl Drop for PageshowListener {
    fn drop(&mut self) {
        if let Some((window, handler)) = self.registered.take() {
            let _ = window
                .remove_event_listener_with_callback("pageshow", handler.as_ref().unchecked_ref());
        }
    }
}

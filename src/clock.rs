//! Time injection.

/// The only source of wall-clock time in this crate.
///
/// The source system calls `Utc::now()` inside record constructors, which makes `rejected_at`
/// untestable. Here every timestamp arrives through this trait.
///
/// This crate depends on `chrono` without its `clock` feature, so `Utc::now()` does not compile
/// anywhere inside it. The injection is enforced by the build, not by convention.
pub trait Clock {
    /// Milliseconds since the Unix epoch.
    fn now_ms(&self) -> i64;
}

impl<T: Clock + ?Sized> Clock for &T {
    fn now_ms(&self) -> i64 {
        (**self).now_ms()
    }
}

impl<T: Clock + ?Sized> Clock for std::rc::Rc<T> {
    fn now_ms(&self) -> i64 {
        (**self).now_ms()
    }
}

/// A [`Clock`] backed by the operating system.
///
/// Not available on `wasm32-unknown-unknown`, where `std::time::SystemTime::now` has no
/// implementation. Web callers supply a clock over `Date.now()` through their adapter, which keeps
/// the JavaScript glue out of core.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

#[cfg(not(target_arch = "wasm32"))]
impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_millis() as i64,
            // Pre-epoch system clock. Negative milliseconds are a legitimate instant, and the
            // record model stores a signed value precisely so this does not need a panic.
            Err(e) => -(e.duration().as_millis() as i64),
        }
    }
}

/// A [`Clock`] the test or caller drives by hand.
///
/// Cloning shares the underlying instant, so a store and a runner built from the same clock see
/// the same time. This is what makes `rejected_at` and retention cutoffs assertable instead of
/// depending on when the test happened to run.
#[derive(Debug, Clone, Default)]
pub struct ManualClock {
    now_ms: std::rc::Rc<std::cell::Cell<i64>>,
}

impl ManualClock {
    /// Start a clock at a fixed instant.
    pub fn new(now_ms: i64) -> Self {
        Self {
            now_ms: std::rc::Rc::new(std::cell::Cell::new(now_ms)),
        }
    }

    /// Jump to an instant.
    pub fn set(&self, now_ms: i64) {
        self.now_ms.set(now_ms);
    }

    /// Move forward.
    pub fn advance(&self, delta_ms: i64) {
        self.now_ms.set(self.now_ms.get() + delta_ms);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.now_ms.get()
    }
}

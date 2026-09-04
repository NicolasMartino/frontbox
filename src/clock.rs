//! Time injection.

/// The only source of wall-clock time in this crate.
///
/// The source system calls `Utc::now()` inside record constructors, which makes `rejected_at`
/// untestable. Here every timestamp arrives through this trait.
///
/// This crate has no date library to call. Timestamps are `i64` milliseconds throughout, and the
/// only code that turns one into a calendar date is the private wire-format renderer in
/// `src/rfc3339.rs`, which converts and never reads a clock. `chrono` survives as a dev-dependency
/// oracle (decision 011) and is declared without its `clock` feature there too, so `Utc::now()`
/// does not compile anywhere — tests included. The injection is enforced by the build, not by
/// convention.
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
/// the JavaScript glue out of core — `frontbox_dioxus::WebClock` behind that crate's `web` feature
/// is the one this repository ships. Named in prose rather than linked: core does not depend on an
/// adapter, and it is a `Clock` implementation like any other, not a blessed one.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

#[cfg(not(target_arch = "wasm32"))]
impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        // Saturating rather than `as`, which truncates silently. Both branches are unreachable
        // for any real clock — `i64` milliseconds covers roughly 292 million years either side of
        // the epoch — but a lossy cast that wraps a timestamp is not worth the two saved lines.
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
            // Pre-epoch system clock. Negative milliseconds are a legitimate instant, and the
            // record model stores a signed value precisely so this does not need a panic.
            Err(e) => i64::try_from(e.duration().as_millis())
                .map(|ms| -ms)
                .unwrap_or(i64::MIN),
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

    /// Move by a signed delta.
    ///
    /// `delta_ms` is an `i64` and a negative one moves the clock *backwards*. That is supported,
    /// not an oversight: a real device clock can jump backwards — an NTP correction, a user editing
    /// the date — and a test that needs to show what the queue does when it happens has to be able
    /// to arrange it. [`set`](ManualClock::set) reaches any instant too; this is the relative form
    /// of the same capability.
    ///
    /// What is *not* supported is arriving there by accident, which is why this saturates rather
    /// than wrapping. A wrapped overflow would send time backwards with nothing in the call to say
    /// so, and a clock that moves backwards unnoticed breaks both the pending order and dead-letter
    /// retention — in release builds, with no panic to catch it.
    pub fn advance(&self, delta_ms: i64) {
        self.now_ms.set(self.now_ms.get().saturating_add(delta_ms));
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.now_ms.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cloning shares the instant, which is what lets a store and a runner agree about "now".
    ///
    /// If clones were independent, every case that moves time and then asserts a `rejected_at` or a
    /// retention cutoff would be asserting against whichever handle it happened to hold.
    #[test]
    fn clones_share_one_instant() {
        let clock = ManualClock::new(1_000);
        let other = clock.clone();

        clock.advance(500);
        assert_eq!(other.now_ms(), 1_500);

        other.set(42);
        assert_eq!(clock.now_ms(), 42);
    }

    /// A negative delta moves the clock backwards, per the contract on `advance`.
    #[test]
    fn a_negative_delta_moves_backwards() {
        let clock = ManualClock::new(1_000);
        clock.advance(-400);
        assert_eq!(clock.now_ms(), 600);

        // Past the epoch is a legitimate instant, not an error: the record model stores a signed
        // value precisely so pre-1970 does not need special handling.
        clock.advance(-1_000);
        assert_eq!(clock.now_ms(), -400);
    }

    /// Overflow saturates. This is the case the signed-delta contract must not silently invert.
    #[test]
    fn overflow_saturates_rather_than_wrapping() {
        let forward = ManualClock::new(i64::MAX - 1);
        forward.advance(i64::MAX);
        assert_eq!(
            forward.now_ms(),
            i64::MAX,
            "a wrapped add would land far in the past"
        );

        let backward = ManualClock::new(i64::MIN + 1);
        backward.advance(i64::MIN);
        assert_eq!(backward.now_ms(), i64::MIN);
    }

    /// The blanket impls exist so a caller can pass `&clock` or an `Rc<dyn Clock>` unchanged.
    #[test]
    fn references_and_rcs_delegate() {
        let clock = ManualClock::new(7);

        fn read(clock: impl Clock) -> i64 {
            clock.now_ms()
        }

        assert_eq!(read(&clock), 7);
        assert_eq!(read(std::rc::Rc::new(clock.clone())), 7);

        let erased: std::rc::Rc<dyn Clock> = std::rc::Rc::new(clock);
        assert_eq!(erased.now_ms(), 7, "the `?Sized` bound is what allows this");
    }

    /// `SystemClock` reads the host clock, and reads it in milliseconds.
    ///
    /// Deliberately loose. Asserting a value would assert the test machine's clock; what is worth
    /// pinning is the unit and the direction, because a `SystemClock` accidentally returning
    /// seconds or nanoseconds would put every `rejected_at` off by three orders of magnitude and
    /// still look like a plausible integer.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_system_clock_reads_epoch_milliseconds() {
        // 2020-01-01 and 2100-01-01, in epoch milliseconds.
        const AFTER: i64 = 1_577_836_800_000;
        const BEFORE: i64 = 4_102_444_800_000;

        let now = SystemClock.now_ms();
        assert!(
            (AFTER..BEFORE).contains(&now),
            "expected epoch milliseconds, got {now}"
        );
    }
}

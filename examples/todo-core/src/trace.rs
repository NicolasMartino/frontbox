//! Browser-visible diagnostics for the trial application.
//!
//! This is deliberately tiny and private. The demo needs enough visibility to debug polling,
//! hydration and sync from DevTools, without turning `todo-core` into an observability API.

/// Emit one diagnostic line.
pub(crate) fn log(message: impl AsRef<str>) {
    let line = format!("[frontbox] {}", message.as_ref());
    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&line.into());
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{line}");
}

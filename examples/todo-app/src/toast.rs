//! Everything the application says to the person using it, in one queue.
//!
//! # Why there is one of these and there used to be three
//!
//! Messages had accumulated three surfaces with no rule separating them: `SyncView::error` (the
//! background failure, rendered as `.alert` in the status bar and cleared by the next drain that
//! succeeded), `SyncView::notice` (a dismissible line in the same bar, for the outcome of something
//! the user did), and `SessionBar`'s own local `status` signal, rendered somewhere else entirely.
//!
//! Three surfaces is not the problem by itself. **The problem is that nothing stopped a call site
//! picking the wrong one**, and one had: the todo composer routed its failures to `error`, which
//! the drain loop sets to `None` on every successful pass — so a rejected "add todo" vanished
//! within five seconds. `crate::sync`'s own doc comment explained at length why those two signals
//! were separate and the call site next door used the wrong one anyway.
//!
//! # Events here, state in the status bar
//!
//! The split that replaces it is by *duration*, not by who wrote it. A toast reports a thing that
//! **happened** — a refresh started, a sign-up queued, a write was refused. A condition that is
//! still **true** — how many records are pending, which service has not answered, whether polling
//! is paused — belongs in [`crate::sync::StatusBar`], because a toast for it would expire and leave
//! the user looking at a screen that no longer mentions a service that has been down for an hour.

use dioxus::prelude::*;

/// How long a finished message stays up.
const SUCCESS_MS: i64 = 4_000;

/// What sort of thing a toast is reporting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Something is happening now. Has no expiry — it is resolved by [`Toasts::resolve`] when the
    /// work ends, so "refreshing…" becomes "refreshed todos" in place rather than stacking a
    /// second toast under the first.
    Progress,
    /// Something finished, and the person does not need to do anything about it.
    Success,
    /// Something failed. **Dismissed by hand, never on a timer**: a failure the user did not
    /// happen to be looking at is a failure they never saw.
    Error,
}

impl Kind {
    /// When a toast of this kind should disappear, relative to now.
    fn ttl_ms(self) -> Option<i64> {
        match self {
            Self::Success => Some(SUCCESS_MS),
            Self::Progress | Self::Error => None,
        }
    }
}

/// One message.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Toast {
    pub id: u64,
    pub kind: Kind,
    pub text: String,
    /// Wall-clock milliseconds after which this is gone, or `None` for the kinds that are not on a
    /// timer.
    pub expires_at: Option<i64>,
}

/// The queue, as a handle every component can hold.
///
/// `Copy`, because both fields are `Signal`s and a signal is a handle rather than a value — the
/// same reason `SyncView` is.
#[derive(Clone, Copy)]
pub struct Toasts {
    items: Signal<Vec<Toast>>,
    next_id: Signal<u64>,
}

impl Toasts {
    /// Say something, and get back the handle that can replace or dismiss it.
    ///
    /// The return value only matters for [`Kind::Progress`]; the others are fire-and-forget.
    pub fn push(&self, kind: Kind, text: impl Into<String>) -> u64 {
        let now_ms = crate::platform::now_ms();
        let (mut items, mut next_id) = (self.items, self.next_id);
        let id = *next_id.peek();
        // `wrapping_add` because ids only have to be unique among the toasts alive at once, and a
        // debug build panicking on overflow would be a worse answer than reusing a number no live
        // toast can still be holding. Reaching it needs 2^64 messages; the point is that the
        // arithmetic states its own intent rather than relying on that.
        next_id.set(id.wrapping_add(1));
        let mut held = items.write();
        held.retain(|toast| toast.expires_at.is_none_or(|at| at > now_ms));
        held.push(Toast {
            id,
            kind,
            text: text.into(),
            expires_at: kind.ttl_ms().map(|ttl| now_ms + ttl),
        });
        id
    }

    /// Turn a [`Kind::Progress`] toast into its outcome, in place.
    ///
    /// Keeping the position is the point: a "refreshing…" that moves to the bottom of the stack to
    /// become "refreshed" reads as two separate things happening.
    pub fn resolve(&self, id: u64, kind: Kind, text: impl Into<String>) {
        let now_ms = crate::platform::now_ms();
        let mut items = self.items;
        let mut held = items.write();
        if let Some(toast) = held.iter_mut().find(|toast| toast.id == id) {
            toast.kind = kind;
            toast.text = text.into();
            toast.expires_at = kind.ttl_ms().map(|ttl| now_ms + ttl);
        }
    }

    /// Take one down.
    pub fn dismiss(&self, id: u64) {
        let mut items = self.items;
        items.write().retain(|toast| toast.id != id);
    }

    /// Drop everything whose time is up.
    ///
    /// Driven by `Ui::now`, so the queue is trimmed by the same tick that re-renders the "4s ago"
    /// in the status bar rather than by a timer of its own.
    pub fn prune(&self, now_ms: i64) {
        let mut items = self.items;
        // Checked before writing: an unconditional `write()` marks every subscriber dirty twice a
        // second forever, which is a re-render of the whole tree for nothing.
        if items
            .peek()
            .iter()
            .any(|toast| toast.expires_at.is_some_and(|at| at <= now_ms))
        {
            items
                .write()
                .retain(|toast| toast.expires_at.is_none_or(|at| at > now_ms));
        }
    }
}

/// Provide the queue to the subtree.
pub fn use_toasts() -> Toasts {
    let toasts = Toasts {
        items: use_signal(Vec::new),
        next_id: use_signal(|| 0),
    };
    use_context_provider(|| toasts);
    toasts
}

/// The stack itself, rendered once for the whole page.
#[component]
pub fn ToastStack() -> Element {
    let toasts = use_context::<Toasts>();
    let items = toasts.items.read().clone();
    if items.is_empty() {
        return rsx! {};
    }

    rsx! {
        // `aria-live="polite"` rather than `assertive`: these announce after whatever the user is
        // doing, which is right for "saved" and right enough for a failure they have to dismiss.
        div { class: "toasts", role: "status", "aria-live": "polite",
            for toast in items {
                div { key: "{toast.id}", class: "toast {class_for(toast.kind)}",
                    span { class: "toast-mark", {mark_for(toast.kind)} }
                    span { class: "toast-text", "{toast.text}" }
                    // Progress has nothing to dismiss yet — it is replaced when its work ends.
                    if toast.kind != Kind::Progress {
                        button {
                            class: "toast-close",
                            // Both, and they are not the same thing: for a `<button>` the accessible
                            // name comes from its *content* — here the glyph — and `title` only ever
                            // becomes the description. Without the `aria-label` this announces as
                            // "×, button".
                            "aria-label": "Dismiss",
                            title: "dismiss",
                            onclick: move |_| toasts.dismiss(toast.id),
                            "×"
                        }
                    }
                }
            }
        }
    }
}

fn class_for(kind: Kind) -> &'static str {
    match kind {
        Kind::Progress => "progress",
        Kind::Success => "success",
        Kind::Error => "error",
    }
}

/// A glyph rather than a coloured dot alone, so the kind survives a colour-blind reader and a
/// monochrome screenshot.
fn mark_for(kind: Kind) -> &'static str {
    match kind {
        Kind::Progress => "↻",
        Kind::Success => "✓",
        Kind::Error => "!",
    }
}

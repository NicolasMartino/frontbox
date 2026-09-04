//! The whole stylesheet, in its own file.
//!
//! Lifted out of `main.rs` when the toast stack took that file past the four-hundred-line cap
//! `AGENTS.md` sets. It is one `const` and no logic, which is exactly the kind of bulk
//! that should not be what pushes a module over.

/// # What "mobile friendly" actually required
///
/// Four things, and only the last is cosmetic:
///
/// - **`font-size: 16px` on every input.** Below that, iOS Safari and the iOS webview zoom the page
///   when a field takes focus and do not zoom back out. It presents as the layout breaking on tap,
///   which reads like a layout bug and is not one.
/// - **`env(safe-area-inset-*)`.** A phone with a notch or a home indicator hands back a viewport
///   whose top and bottom strips are occluded. Without this the title sits under the status bar.
/// - **44px touch targets under `(pointer: coarse)`.** The desktop control sizes are fine for a
///   mouse and too small for a thumb; keying it off the pointer type rather than the width means a
///   touchscreen laptop gets the large ones too, which is the actual condition.
/// - **A single column below 30rem**, so the composer's field and button stack instead of
///   squeezing the field to nothing.
pub const CSS: &str = "
:root { color-scheme: dark; }
* { -webkit-tap-highlight-color: transparent; }
body { margin: 0; background: #111; color: #ddd; font: 16px/1.5 system-ui, sans-serif;
  overscroll-behavior-y: none; }
/* The bottom padding reserves the strip the toast stack floats in. Without it a short viewport
   puts transient messages on top of the status bar, which is exactly backwards: the toasts are the
   things that leave, and the bar underneath is the standing state a person is trying to read. */
.app { max-width: 34rem; margin: 0 auto; box-sizing: border-box;
  padding: calc(1.5rem + env(safe-area-inset-top)) calc(1rem + env(safe-area-inset-right))
           calc(6rem + env(safe-area-inset-bottom)) calc(1rem + env(safe-area-inset-left)); }
@media (min-width: 40rem) { .app { padding-top: 3rem; } }
h1 { font-size: 1.4rem; font-weight: 600; margin-top: 0; }
h2 { font-size: 1rem; font-weight: 600; margin: 1.5rem 0 .5rem; }
.compose { display: flex; gap: .5rem; margin-bottom: 1rem; }
.compose input { flex: 1; min-width: 0; }
input, button { font-family: inherit; font-size: 16px; padding: .4rem .55rem; border-radius: 6px;
  border: 1px solid #444; background: #1c1c1c; color: inherit; }
button { cursor: pointer; }
/* A thumb needs 44px; a mouse does not. Keyed off the pointer rather than the viewport, because a
   touchscreen laptop is wide and still being tapped. */
@media (pointer: coarse) {
  input, button { min-height: 44px; padding: .55rem .75rem; }
  .row input[type=checkbox] { width: 24px; height: 24px; }
  .row { padding: .5rem 0; }
  .status .notice button { min-height: 32px; }
}
@media (max-width: 30rem) {
  .compose { flex-direction: column; }
  .compose button { width: 100%; }
}
.controls { display: flex; flex-wrap: wrap; align-items: center; gap: .5rem;
  justify-content: flex-end; margin-bottom: .75rem; color: #aaa; }
/* The identity and refresh bar reads left-to-right: who you are, then what you can ask for. */
.session { justify-content: space-between; }
.session .notice { flex: 1 1 100%; order: 3; text-align: right; }
.identity { color: #7a6; }
.offline { display: inline-flex; align-items: center; gap: .5rem; }
.offline input { accent-color: #d08b38; }
ul { list-style: none; margin: 0; padding: 0; }
/* Two levels, one pattern: a user head and a todo row are the same flex line at two depths, which
   is why `.row` below is reused unchanged inside `.nested`. */
.users { border: 1px solid #2a2a2a; border-radius: 8px; }
.user + .user { border-top: 1px solid #2a2a2a; }
.user-head { display: flex; align-items: center; gap: .5rem; padding: .5rem .6rem; }
.user-head .title { flex: 1; min-width: 0; font-weight: 600; }
.expander { flex: none; width: 2rem; padding: .2rem 0; text-align: center;
  background: none; border: none; color: #888; }
.tag { flex: none; color: #7a6; font-size: .8rem; }
.count { flex: none; color: #888; font-size: .8rem; font-variant-numeric: tabular-nums; }
.confirm { display: inline-flex; align-items: center; gap: .4rem; color: #d08b38;
  font-size: .85rem; }
.nested { padding: 0 .6rem .6rem 2.5rem; }
.nested .compose { margin: .5rem 0 0; }
.nested .notice { margin: .25rem 0; }
.row { display: flex; align-items: center; gap: .5rem; padding: .25rem 0; }
.row .title { flex: 1; min-width: 0; }
.row.saving .title { border-color: #7a6; }
.mark { color: #7a6; font-size: .8rem; flex: none; }
.status { display: flex; flex-wrap: wrap; gap: 1rem; margin-top: 1.5rem;
  border-top: 1px solid #333; padding-top: .75rem; font-size: .85rem; color: #999; }
.status .alert { color: #d77; }
.status .notice { color: #d9a441; display: inline-flex; align-items: center; gap: .4rem; }
.status .notice button { padding: 0 .4rem; border-color: #6a5426; background: transparent;
  color: inherit; font-size: .85rem; line-height: 1.2; }
.status .gap { color: #d9a441; }
.status .cache { color: #8a8; display: inline-flex; flex-wrap: wrap; gap: .4rem; }
.status .cache .source + .source::before { content: \"\\00b7\"; margin-right: .4rem; color: #555; }
/* The two states a person actually asks about: is it looking, and is it stopped. */
.status .working { color: #7a9ad0; }
.status .paused { color: #d9a441; }
.dead li { border-left: 2px solid #d77; padding-left: .6rem; margin-bottom: .5rem; }
.dead .op { display: block; font-family: ui-monospace, monospace; font-size: .8rem; color: #999;
  overflow-wrap: anywhere; }
/* # The toast stack
 *
 * `position: fixed` and bottom-anchored, so it does not push the list around as messages come and
 * go — a status line that reflows the thing you are reading is worse than no status line.
 *
 * `pointer-events: none` on the stack and `auto` on each toast: the strip spans the width of the
 * page and would otherwise swallow clicks on whatever is underneath it. */
.toasts { position: fixed; z-index: 10; left: 0; right: 0;
  bottom: calc(1rem + env(safe-area-inset-bottom));
  display: flex; flex-direction: column; align-items: center; gap: .4rem;
  padding: 0 1rem; pointer-events: none; }
.toast { pointer-events: auto; display: flex; align-items: center; gap: .5rem;
  box-sizing: border-box; width: 100%; max-width: 30rem;
  padding: .5rem .7rem; border-radius: 8px; font-size: .9rem;
  border: 1px solid #333; background: #1c1c1c; color: #ddd;
  box-shadow: 0 6px 20px rgb(0 0 0 / .45); }
.toast-mark { flex: none; font-size: .95rem; line-height: 1; }
.toast-text { flex: 1; min-width: 0; overflow-wrap: anywhere; }
.toast-close { flex: none; padding: 0 .4rem; border: none; background: transparent; color: inherit;
  font-size: 1rem; line-height: 1.2; opacity: .7; }
.toast.progress { border-color: #33495e; }
.toast.progress .toast-mark { color: #7a9ad0; }
.toast.success { border-color: #2f4a2f; }
.toast.success .toast-mark { color: #7a6; }
.toast.error { border-color: #5e3333; }
.toast.error .toast-mark { color: #d77; }
/* The one animation, and it is the one that carries information: something is in flight. */
@keyframes toast-spin { to { transform: rotate(360deg); } }
.toast.progress .toast-mark { display: inline-block; animation: toast-spin 1.1s linear infinite; }
@media (prefers-reduced-motion: reduce) { .toast.progress .toast-mark { animation: none; } }
@media (pointer: coarse) { .toast-close { min-height: 32px; min-width: 32px; } }
";

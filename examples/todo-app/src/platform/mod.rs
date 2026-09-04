//! What actually differs between a browser, a desktop window and a phone.
//!
//! # The point of this module being small
//!
//! `todo-core` already draws the *storage* seam — SQLite where there is a filesystem, IndexedDB
//! where there is a browser — in one module, and D4b's finding was how little that cost. This is
//! the same experiment one level up: the UI crate now serves three platforms, and everything a
//! platform genuinely decides is here. Nothing in `main.rs`, `rows.rs` or `sync.rs` is
//! conditionally compiled, and no component knows which platform it is on.
//!
//! Three things turned out to be platform decisions, and only three:
//!
//! - **The clock.** `Date.now()` in a browser, `SystemTime` everywhere else.
//! - **The timer.** `gloo-timers` in a browser, `tokio::time` everywhere else. It arrives through
//!   [`Sleeper`], which is the seam `frontbox-dioxus` drew for exactly this.
//! - **Where storage lives.** A name in a browser, a file path everywhere else — and on Android,
//!   a question only the running `Activity` can answer.
//!
//! **Being in front is a fourth, and it took a second pass to see that it is one difference and not
//! two.** It looked like a browser concept — `visibilitychange` and `focus` — with no native
//! counterpart, so the native builds attached nothing and `focused()` was hard-coded `true`. They
//! do have a counterpart: a window reports focus and an OS reports suspension, both on tao's event
//! loop. Both arms now answer the same question through [`Foreground`], and the wake that ends a
//! pending wait is the same one on every target.
//!
//! That fourth one is what turned this file into a directory: it is in [`foreground`] next door,
//! because it is the only platform difference here that changes *while the application runs* rather
//! than being decided once at startup.

use frontbox_dioxus::Sleeper;

/// One demo user's queue.
///
/// A real client derives this from whoever is signed in — the key is what separates two users'
/// outboxes in one profile, so a constant is a single-tenant demo and nothing more.
///
/// D4d added a second string that also says "user" and means something else entirely; see
/// [`USER_ID`].
pub const SCOPE: &str = "user:demo@tenant:local";

/// The user *record* this demo writes its todos under.
///
/// **Not [`SCOPE`], and the difference is the whole of what D4d demonstrates.** The scope is the
/// local queue identity, valid the instant this client picks it and never issued by anyone
/// (`wiki/decisions/009-local-scope-identity.decision.md`). This is a row id on the user service —
/// a row that does not exist until a queued mutation drains, which may be days later on a client
/// that started offline.
///
/// A constant for the same reason `SCOPE` is one: a demo has one identity, and the affordance for
/// choosing another is the sign-up field in the UI rather than a rebuild.
pub const USER_ID: &str = "user-demo";

/// The clock this platform reads wall time from.
#[cfg(target_arch = "wasm32")]
pub type PlatformClock = frontbox_dioxus::WebClock;

/// The clock this platform reads wall time from.
#[cfg(not(target_arch = "wasm32"))]
pub type PlatformClock = frontbox::SystemClock;

/// Build the clock.
#[cfg(target_arch = "wasm32")]
pub fn clock() -> PlatformClock {
    frontbox_dioxus::WebClock
}

/// Build the clock.
#[cfg(not(target_arch = "wasm32"))]
pub fn clock() -> PlatformClock {
    frontbox::SystemClock
}

/// Where the server is.
///
/// # Why the default is not `127.0.0.1` on every platform
///
/// It is on desktop and in a browser, where the loopback address means the machine the server is
/// running on. **On an Android emulator it does not**: the guest has its own loopback, so
/// `127.0.0.1` reaches the emulated device itself and the request fails with connection refused.
/// `10.0.2.2` is the address the emulator maps to the host, which is what makes an offline-first
/// demo testable there at all.
///
/// An iOS simulator shares the host's network stack, so the loopback address is right there and
/// this distinction does not apply to it — which is worth stating, because "it worked on iOS" is
/// exactly the reasoning that would leave the Android build silently offline.
pub const BASE_URL: &str = match option_env!("TODO_SERVER_URL") {
    Some(url) => url,
    None => {
        if cfg!(target_os = "android") {
            "http://10.0.2.2:3000"
        } else {
            "http://127.0.0.1:3000"
        }
    }
};

/// Where the *user* service is, which D4d added beside the todo one.
///
/// The same Android reasoning applies unchanged, which is the point of putting it here: the
/// platform module is where a platform decides things, and "which loopback reaches the host" is a
/// platform decision that a second service must not get a second answer to.
pub const USER_BASE_URL: &str = match option_env!("USER_SERVER_URL") {
    Some(url) => url,
    None => {
        if cfg!(target_os = "android") {
            "http://10.0.2.2:3001"
        } else {
            "http://127.0.0.1:3001"
        }
    }
};

/// Build the sleeper the drain loop waits on.
///
/// On web it also carries the back/forward cache wake: a frozen page's timers do not fire, so a
/// restored tab would otherwise serve out the remainder of an interval chosen for a live one.
#[cfg(target_arch = "wasm32")]
pub fn sleeper(wake: frontbox_dioxus::Wake) -> Sleeper {
    Sleeper::new(gloo_timers::future::TimeoutFuture::new).wakeable(wake)
}

/// Wall time, in epoch milliseconds.
///
/// The same clock the application hands to `TodoApp`, reached without one — the toast timers and
/// the status bar's "4s ago" are UI concerns and have no application object to ask.
pub fn now_ms() -> i64 {
    use frontbox::Clock;
    clock().now_ms()
}

/// A timer with no wake attached, for waits that must not be cut short.
///
/// The UI clock in `main.rs` is one: it ticks to re-render "4s ago" and to retire finished toasts,
/// and racing it against the app wake would mean every window focus also fired a render tick. A
/// wait that nothing should interrupt is a different thing from a wait that anything may, and
/// `Sleeper::wakeable` being opt-in is what lets both exist.
pub fn timer() -> Sleeper {
    #[cfg(target_arch = "wasm32")]
    {
        Sleeper::new(gloo_timers::future::TimeoutFuture::new)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Sleeper::new(|ms: u32| tokio::time::sleep(std::time::Duration::from_millis(u64::from(ms))))
    }
}

/// Build the sleeper the drain loop waits on.
///
/// The wake is the application's own — see [`app_wake`]. A desktop window and a phone application
/// are not frozen the way a page in the
/// back/forward cache is, so there is no event that makes a pending wait meaningless — and an
/// invented one would be a claim this trial has not tested.
///
/// **A phone genuinely does suspend**, and it now gets the same wake a restored tab does: an
/// application the OS stops scheduling is the frozen page exactly, which is why
/// [`app_wake`] observes `Resumed` there. See
/// `wiki/decisions/043-in-front-on-every-platform.decision.md`; the question used to be cited to
/// `wiki/references/open-decisions.reference.md`, which never had an entry for it.
#[cfg(not(target_arch = "wasm32"))]
pub fn sleeper(wake: frontbox_dioxus::Wake) -> Sleeper {
    Sleeper::new(|ms: u32| tokio::time::sleep(std::time::Duration::from_millis(u64::from(ms))))
        .wakeable(wake)
}

/// Where this platform's durable storage lives.
///
/// A database *name* on web and a file *path* everywhere else — the same argument with two
/// meanings, which is as far as `todo-core`'s storage seam lets the difference travel.
/// # Errors
///
/// Never on web. The signature matches the native one so the call site does not have to know which
/// platform it is on — a `storage()` that returned a bare `String` here and a `Result` there is two
/// APIs wearing one name, which is the thing `todo-core`'s own backend seam refused to do.
#[cfg(target_arch = "wasm32")]
pub fn storage() -> Result<String, String> {
    // Visible in a browser's Application panel under this name: the outbox, the dead letters, the
    // quarantine, the read-model rows and the cache versions, each in its own object store.
    Ok("frontbox-todo".to_owned())
}

/// Where this platform's durable storage lives.
///
/// # Errors
///
/// No directory this application may write to, which on Android means the activity could not be
/// reached and on desktop means the platform has no data directory.
#[cfg(not(target_arch = "wasm32"))]
pub fn storage() -> Result<String, String> {
    let directory = data_directory()?.join("frontbox-todo");
    std::fs::create_dir_all(&directory).map_err(|failure| failure.to_string())?;
    let file = directory.join("todo.sqlite3");
    file.to_str()
        .map(str::to_owned)
        // `SqliteBackend::open` takes an `AsRef<Path>`, so this is only a limitation of carrying
        // the location as a `String` for the web half's sake. Reported rather than lossily
        // converted: a path that is not UTF-8 would open a *different* file than the one named.
        .ok_or_else(|| format!("storage path is not valid UTF-8: {}", directory.display()))
}

/// The per-application data directory, on everything but Android.
///
/// `dirs` answers this correctly on iOS too — the simulator and the device both put
/// `data_dir()` inside the application sandbox, which is exactly where a SQLite file belongs.
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn data_directory() -> Result<std::path::PathBuf, String> {
    dirs::data_dir().ok_or_else(|| "no data directory on this platform".to_owned())
}

/// The per-application data directory on Android, from the running activity.
///
/// # Why this needs JNI and the other platforms do not
///
/// **There is no environment variable that answers this on Android.** `HOME` is unset, `TMPDIR`
/// points at nothing writable, and `dirs::data_dir()` returns [`None`] as a result — so a build
/// that reused the desktop path would fail at the first write with a permission error rather than
/// at compile time. The per-application directory is a property of the `Context`, and the only way
/// to it is to ask the `Activity` the runtime already created.
///
/// `ndk-context` is how a Rust library reaches that activity without owning the entry point:
/// wry's `WryActivity` installs the JVM and activity pointers on startup, and this reads them back.
/// The alternative — threading a path down from the platform entry point — would put a parameter
/// on `TodoApp::new` that exists for one operating system.
#[cfg(target_os = "android")]
fn data_directory() -> Result<std::path::PathBuf, String> {
    let context = ndk_context::android_context();
    // SAFETY: `context.vm()` is the `JavaVM*` the Android runtime installed before any Rust code
    // ran — `ndk_context` holds what wry's `WryActivity` wrote at startup, and the VM outlives the
    // process's Rust half by construction. The cast is `*mut c_void` to `*mut JavaVM`, which is the
    // pointer's actual type; `ndk-context` erases it and `jni` asks for it back.
    let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) }
        .map_err(|failure| format!("android vm: {failure}"))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|failure| format!("android thread attach: {failure}"))?;
    // SAFETY: `context.context()` is the global reference to the `Activity` from the same source,
    // valid for as long as the activity exists — which is longer than this call, since the call is
    // made *by* that activity's process while it runs. `JObject::from_raw` borrows rather than
    // takes ownership, so this must not and does not delete the reference.
    let activity = unsafe { jni::objects::JObject::from_raw(context.context().cast()) };

    let files_dir = env
        .call_method(&activity, "getFilesDir", "()Ljava/io/File;", &[])
        .and_then(|value| value.l())
        .map_err(|failure| format!("android getFilesDir: {failure}"))?;
    let path = env
        .call_method(&files_dir, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .and_then(|value| value.l())
        .map_err(|failure| format!("android getAbsolutePath: {failure}"))?;
    let path: String = env
        .get_string(&jni::objects::JString::from(path))
        .map_err(|failure| format!("android path decode: {failure}"))?
        .into();
    Ok(std::path::PathBuf::from(path))
}

mod foreground;

pub use foreground::{app_wake, Foreground};

#!/usr/bin/env bash
# Verification gates from wiki/roadmaps/extraction.roadmap.md and the D1/D3 plans.
#
# Every gate names the package it means. This became load-bearing when D3 made the repository a
# workspace: a bare `cargo clippy` or `cargo llvm-cov` in a workspace silently changes which crates
# it covers, and a gate that quietly stops checking core is worse than no gate.
#
# Runnable from anywhere: it locates the repository root from its own path.
set -uo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" || exit 1

failures=0
skipped=0

# A gate that needs something this machine may not have.
#
# Printed, counted, and named in the summary, because the alternative — leaving it out when its
# prerequisite is missing — is a script that reports ALL GATES PASSED for a smaller set of gates
# than the reader thinks it ran.
skip() {
  printf '\n=== %s\n    SKIPPED: %s\n' "$1" "$2"
  skipped=$((skipped + 1))
}

run() {
  local label="$1"; shift
  printf '\n=== %s\n' "$label"
  if "$@"; then
    printf '    ok\n'
  else
    printf '    FAILED: %s\n' "$label"
    failures=$((failures + 1))
  fi
}

# ---------------------------------------------------------------------------
# Primary gates. These are the ones that actually establish correctness.
# ---------------------------------------------------------------------------

run "fmt"                 cargo fmt --all --check
run "clippy native"       cargo clippy -p frontbox --all-targets --all-features -- -D warnings
run "tests"               cargo test -p frontbox --all-features
run "build wasm"          cargo build -p frontbox --target wasm32-unknown-unknown --all-features
run "build wasm minimal"  cargo build -p frontbox --target wasm32-unknown-unknown --no-default-features
run "clippy wasm"         cargo clippy -p frontbox --target wasm32-unknown-unknown --all-features -- -D warnings

# The Dioxus adapter (D3b). The conformance suite cannot reach it — there is no headless Dioxus
# runtime here — so compiling for both targets under `-D warnings` is most of its gate, and the D4
# example is what actually exercises it.
#
# `tests adapter` is the exception, and it earned itself: `Wake` and `Sleeper::wakeable` are a
# latch and a hand-written race, which are plain Rust and testable with a counting waker. What is
# still untestable here is the `pageshow` listener that fires the wake, because that needs a
# browser.
run "tests adapter"       cargo test -p frontbox-dioxus
# The SQLite backend runs the *same* conformance functions the in-memory one does, which is the
# whole claim of the shared suite. Its own gate rather than a line in the core one, because a
# backend that stops compiling must not be reported as a core failure.
#
# The cache suite is deliberately absent from its test file: it has no `CacheVersionStore` yet, so
# it does not implement `VersionStoreFactory` and the gap is visible there rather than hidden.
run "clippy sqlite"       cargo clippy -p frontbox-sqlite --all-features --all-targets -- -D warnings
run "tests sqlite"        cargo test -p frontbox-sqlite --features testing

# The IndexedDB backend. The two gates below compile the suite; the one after them runs it, when
# the machine has what running it needs.
#
# The compile gates are not made redundant by the run: they type-check every case against this
# backend's traits through the async macro variants, which is where a missing `.await` or a stale
# case name would otherwise surface halfway through a browser session — and they hold on a machine
# with no browser at all.
run "build idb wasm"      cargo build -p frontbox-indexeddb --target wasm32-unknown-unknown --features testing
run "clippy idb wasm"     cargo clippy -p frontbox-indexeddb --target wasm32-unknown-unknown --features testing --all-targets -- -D warnings

# The browser run: 68 conformance cases and the two cross-realm fixtures, in headless Chrome.
#
# **Opt-in rather than absent.** It needs a Chrome and a chromedriver matching it, which a build
# gate cannot assume — so it runs when `CHROMEDRIVER` names one and is announced as SKIPPED when it
# does not. It used to be a comment saying how to run it by hand, and a comment is not a gate:
# `tests/cross_realm.rs` exists to observe decision 031's cross-realm half, and a proof nothing
# executes is the state D5's status line spent two days overstating.
#
# Get a driver matching the installed Chrome from
# https://googlechromelabs.github.io/chrome-for-testing/ and point `CHROMEDRIVER` at it.
#
# Two environment variables, both load-bearing:
#   - `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER` — without it cargo hands the `.wasm` to the
#     shell, which answers `cannot execute binary file` and exit 126. The repository carries no
#     `.cargo/config.toml`, so nothing supplies this by default.
#   - `WASM_BINDGEN_TEST_ONLY_WEB` — Node has no IndexedDB, so the harness must not fall back to it.
#
# `--release` is not optional either: a debug build of the suite exceeds chromedriver's 300-second
# renderer timeout before a single test runs.
if [ -n "${CHROMEDRIVER:-}" ]; then
  run "tests idb browser"  env CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
                               WASM_BINDGEN_TEST_ONLY_WEB=1 \
                               cargo test -p frontbox-indexeddb --release --target wasm32-unknown-unknown --features testing
else
  skip "tests idb browser" "set CHROMEDRIVER to a driver matching the installed Chrome"
fi

run "clippy adapter"      cargo clippy -p frontbox-dioxus --all-targets -- -D warnings
run "build adapter wasm"  cargo build -p frontbox-dioxus --target wasm32-unknown-unknown

# The `web` feature (register entries 15 and 16): a clock over `Date.now()` and a `pageshow`
# listener. A feature nothing compiles is a feature that does not work, and the two gates above are
# the other half of the pair — they run with it *off*, which is what proves a Dioxus desktop build
# still takes no `js-sys` and no `web-sys`.
run "clippy adapter web"  cargo clippy -p frontbox-dioxus --target wasm32-unknown-unknown --features web --all-targets -- -D warnings
run "build adapter web"   cargo build -p frontbox-dioxus --target wasm32-unknown-unknown --features web

# The `native` feature (decision 043): the focus and lifecycle events that say whether the
# application is in front. Same pairing argument as `web` above, in the other direction — the
# feature-off gates prove a browser build never links `dioxus-desktop`, which is the one renderer
# dependency this crate has and the reason the feature is default-off.
run "clippy adapter native" cargo clippy -p frontbox-dioxus --features native --all-targets -- -D warnings
run "build adapter native"  cargo build -p frontbox-dioxus --features native

# The D4a and D4d trials. These are examples, but their tests are the only end-to-end evidence the
# project has: `todo-server` writes its own wire shapes from the spec and does not depend on
# `frontbox`, so a disagreement about the wire format fails them and nothing else. That makes
# them a primary gate rather than a demo.
#
# `user-server` joined in D4d and does not depend on `todo-server` either, which is the same
# argument one level up: two servers sharing `wire.rs` would be one implementation with two
# endpoints, and the second reading of the spec is the whole reason a second server was worth
# building (`examples/user-server/src/wire.rs`).
run "clippy trial"        cargo clippy -p todo-core -p todo-server -p user-server --all-targets -- -D warnings
run "tests trial"         cargo test -p todo-core -p todo-server -p user-server

# `todo-core` is the framework-neutral half, and until D4a's UI landed nothing ever compiled it for
# the browser — the two gates above are host-only. It was in fact broken: `reqwest::Error::is_connect`
# is `#[cfg(not(target_arch = "wasm32"))]`, so the crate whose entire claim is that it runs anywhere
# did not build for the one target that claim is about. A wasm gate for the trial is not symmetry
# with core's; it is the gate that would have caught it.
run "build trial wasm"    cargo build -p todo-core --target wasm32-unknown-unknown
run "build ui wasm"       cargo build -p todo-app --target wasm32-unknown-unknown
run "clippy ui wasm"      cargo clippy -p todo-app --target wasm32-unknown-unknown --all-targets -- -D warnings

# The same UI crate on a native target (D4c). **This is the gate that keeps three platforms from
# becoming one.** `examples/todo-app` now builds for web, desktop, iOS and Android from one component
# tree, and the only thing standing between that and silent rot is a build that does not use the
# browser: `examples/todo-app/src/platform/mod.rs`'s native half — `SystemClock`, the tokio
# `Sleeper`, `dirs`, a SQLite file
# path — is compiled by nothing else here.
#
# `--no-default-features` is load-bearing: the default feature is `web`, so without it this would
# enable `dioxus/web` alongside `dioxus/desktop` and fail for a reason that has nothing to do with
# the code under test.
#
# What the desktop gate buys the mobile targets is not nothing: iOS and Android compile the *same*
# `not(target_arch = "wasm32")` half, minus Android's JNI block. The two gates below compile them
# for real, when the machine has what that needs.
run "build ui desktop"    cargo build -p todo-app --no-default-features --features desktop
run "clippy ui desktop"   cargo clippy -p todo-app --no-default-features --features desktop --all-targets -- -D warnings

# The mobile targets, opt-in rather than absent.
#
# **These were ungated for two deliverables**, on the reasoning that iOS needs Xcode and Android an
# NDK and a build gate can assume neither. That is true and it is not a reason to have no gate: the
# nested user/todo UI rewrote most of `examples/todo-app` with nothing checking that either target
# still compiled, and "run it on an emulator sometimes" is not a gate. So they run when the
# toolchain is present and are announced as SKIPPED when it is not — the same shape as
# `tests idb browser`.
#
# `--no-default-features` is load-bearing here for the reason it is on desktop: the default feature
# is `web`.
if xcrun --sdk iphonesimulator --show-sdk-version > /dev/null 2>&1; then
  run "build ui ios"      cargo build -p todo-app --target aarch64-apple-ios-sim --no-default-features --features mobile
else
  skip "build ui ios" "needs the iOS SDK — install Xcode or its command line tools"
fi

# Android needs more than the NDK's presence: a plain `cargo build` fails looking for
# `aarch64-linux-android-clang`, which no NDK ships. The wrappers carry an API level in the name, so
# the toolchain has to be named explicitly — which `dx --platform android` does for you and cargo
# does not. Pinned at 24 because that is what `rusqlite`'s bundled SQLite needs and what D4c ran.
android_toolchain() {
  local host
  case "$(uname -s)" in
    Darwin) host=darwin-x86_64 ;;
    Linux)  host=linux-x86_64 ;;
    *) return 1 ;;
  esac
  printf '%s/toolchains/llvm/prebuilt/%s/bin' "${ANDROID_NDK_HOME}" "$host"
}

build_ui_android() {
  local tc
  tc="$(android_toolchain)" || return 1
  PATH="$tc:$PATH" \
  CC_aarch64_linux_android="$tc/aarch64-linux-android24-clang" \
  AR_aarch64_linux_android="$tc/llvm-ar" \
  CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$tc/aarch64-linux-android24-clang" \
    cargo build -p todo-app --target aarch64-linux-android --no-default-features --features mobile
}

if [ -n "${ANDROID_NDK_HOME:-}" ] && [ -d "$(android_toolchain 2>/dev/null)" ]; then
  run "build ui android"  build_ui_android
else
  skip "build ui android" "set ANDROID_NDK_HOME to an installed NDK"
fi

# Coverage. The threshold is a floor on the whole crate, not a per-file rule: a module of pure
# trait declarations has nothing to execute, and chasing a number there would mean writing tests
# that assert the compiler works.
#
# Skipped rather than failed when the tool is absent, and the skip says so — a gate that reports
# "ok" for work it did not do is worse than no gate.
COVERAGE_FLOOR=80
coverage() {
  cargo llvm-cov -p frontbox --all-features --summary-only \
    --fail-under-lines "$COVERAGE_FLOOR" --fail-under-functions "$COVERAGE_FLOOR"
}
if command -v cargo-llvm-cov >/dev/null 2>&1; then
  run "coverage >= ${COVERAGE_FLOOR}%" coverage
else
  printf '\n=== coverage >= %s%%\n' "$COVERAGE_FLOOR"
  printf '    SKIPPED: cargo-llvm-cov is not installed (cargo install cargo-llvm-cov)\n'
fi

# ---------------------------------------------------------------------------
# Secondary gates: textual checks for properties the compiler cannot express.
#
# These are a backstop, not evidence. Each one is a cheap proxy for something a
# primary gate or a test already covers more convincingly:
#
#   - the wasm builds are what actually prove the crate stays frontend-compatible;
#   - tests/not_send.rs proves the traits accept a `!Send` type, and the trial pins
#     its concrete runner with a negative assertion plus a compile-fail doctest.
#     Those prove named types. This grep is for the absence of a `Send` bound
#     anywhere in source, which no finite set of type assertions can cover — a
#     bound on a type nobody wrote an assertion for is exactly what it catches;
#   - the RepForge-name check is a smell test for a boundary that is ultimately a
#     review judgment, not a mechanical property.
#
# A green grep proves nothing on its own. A red one is worth reading.
# ---------------------------------------------------------------------------

check_absent() {
  local label="$1"; shift
  printf '\n=== %s (secondary)\n' "$label"
  if "$@"; then
    printf '    FAILED: %s\n' "$label"
    failures=$((failures + 1))
  else
    printf '    ok\n'
  fi
}

# `grep` without `-q` throughout: a failing gate has to show what it matched, or diagnosing it
# from a CI log means reproducing the run locally.
run "docs resolve" env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --quiet

# Asked of the resolved graph rather than of Cargo.toml. The manifest check this replaced could
# not survive `frontbox-dioxus` joining the workspace: a `members` entry matches the grep and is
# not a dependency, so the gate would have gone red for the one arrangement it was written to
# permit. The graph answers the question the gate actually means, and it is the same argument the
# date-library check below already makes for itself.
dioxus_in_core_graph() {
  cargo tree -p frontbox --edges normal --all-features | grep -i dioxus
}
check_absent "no dioxus in the frontbox dependency graph" dioxus_in_core_graph

# Decision 011: the wire format's timestamp rendering is ours, and `chrono` is a test oracle only.
# Asked of the resolved graph rather than of Cargo.toml, because the risk is a date crate arriving
# transitively — which no amount of reading our own manifest would show. `--edges normal` is what
# excludes dev-dependencies, so chrono in `src/rfc3339/tests.rs` stays legal and chrono in `src/`
# does not.
date_crate_in_runtime_graph() {
  cargo tree -p frontbox --edges normal --all-features | grep -iE '(chrono|jiff|^time |[^-]time v)'
}
check_absent "no date library in the runtime dependency graph" date_crate_in_runtime_graph

check_absent "no RepForge entity names in core or the adapter" \
  grep -rnE '\b(Exercise|WorkoutSession|WorkoutTemplate|SessionExercise|MuscleGroup|Preferences)\b' \
  src/ crates/*/src/

# Comment lines are excluded: the decision is discussed in prose in several places, and matching
# that prose would make the gate cry wolf until nobody reads it.
# `examples/todo-server` is excluded on purpose: it is a server, axum requires `Send` handlers,
# and nothing about a server has to satisfy decision 001. `examples/todo-core` is *not* excluded,
# because it is the client half and it is the first application to have to live with the
# posture. The `observations` suite and `todo-core`'s compile-fail doctest both require the concrete
# application runner to remain `!Send`; this grep stays secondary because absence of a bound is
# still easier to accidentally lose in prose than in code.
no_send_bound() {
  grep -rn --include='*.rs' -E '(\+|:)[[:space:]]*Send\b' src/ crates/*/src/ examples/todo-core/src/ examples/todo-app/src/ \
    | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//'
}
check_absent "no Send bound in core or the adapter" no_send_bound


# Every `path/to/file.rs` citation in the prose resolves to a file that exists.
#
# The wiki cites source by path constantly, and those citations are load-bearing: a decision that
# says "see `src/runner/mod.rs`" is making a checkable claim about where its evidence lives. They
# rot silently, because splitting a file into a directory renames it and nothing recompiles the
# prose. Seven had already gone stale by the time anyone looked.
#
# What it does not check: whether the cited file still says what the citing page claims it says.
# That is review's job, and no grep replaces it.
#
# `wiki/log.md` is exempt. It is append-only by `project_guidelines.md`, so an entry citing a path
# that was correct on the day it was written is a historical record, not a broken link — repointing
# it would edit history to make a gate green.
#
# Lines mentioning RepForge are exempt too: `wiki/references/repforge-section-c-answers.reference.md`
# records paths in *another* repository, and says so where it records them.
cited_paths_resolve() {
  python3 - <<'PYEOF'
import pathlib, re, subprocess, sys

# The `:line` suffix is optional and *outside* the capture, which is the whole point of this
# version. The wiki cites source as `src/runner/mod.rs:275` far more often than bare, and the
# original pattern required the closing backtick immediately after the extension — so every
# citation carrying a line number was invisible to the gate that exists to check citations. Four
# had already gone stale behind that hole, including a file split two deliverables ago.
pattern = re.compile(r'`((?:src|crates|examples|scripts|wiki)/[A-Za-z0-9_./-]+\.(?:rs|md|sh|toml|sql))(?::\d+)?`')
tracked = subprocess.run(["git", "ls-files"], capture_output=True, text=True).stdout.split()
broken = []
for name in tracked:
    if name.startswith("raw/") or name == "wiki/log.md":
        continue
    try:
        lines = pathlib.Path(name).read_text().splitlines()
    except (OSError, UnicodeDecodeError):
        continue
    for number, line in enumerate(lines, 1):
        if "RepForge" in line:
            continue
        for cited in pattern.findall(line):
            if not pathlib.Path(cited).exists():
                broken.append(f"{name}:{number}: {cited}")
for entry in broken:
    print(entry)
sys.exit(1 if broken else 0)
PYEOF
}
run "cited paths resolve" cited_paths_resolve

# No source file over the line cap `AGENTS.md` sets.
#
# The cap is a shape rule, not a correctness one, and it was being kept by hand — which meant
# `examples/todo-core/src/app/mod.rs` grew to 579 lines, 45% over, while its own module doc opened
# by explaining that it had become a directory *because of the cap*. A file that argues for a rule
# in its first paragraph and breaks it by its last is the case for making the rule mechanical.
#
# Generated and vendored files are out of scope, as is `raw/`, which is immutable provenance.
FILE_LINE_CAP=400
files_within_line_cap() {
  local over=0
  while IFS= read -r file; do
    # Tracked-but-deleted paths appear here until a split is staged; skip them rather than
    # reporting a missing file as a cap violation.
    [ -f "$file" ] || continue
    local count
    count=$(wc -l < "$file")
    if [ "$count" -gt "$FILE_LINE_CAP" ]; then
      printf '    %s: %s lines\n' "$file" "$count"
      over=1
    fi
  done < <(git ls-files 'src/*.rs' 'crates/*/src/*.rs' 'examples/*/src/*.rs')
  [ "$over" -eq 0 ]
}
run "no source file over ${FILE_LINE_CAP} lines" files_within_line_cap

printf '\n'
if [ "$failures" -eq 0 ]; then
  printf 'ALL GATES PASSED\n'
else
  printf '%d GATE(S) FAILED\n' "$failures"
fi
# Printed either way. A skipped gate alongside ALL GATES PASSED is the honest reading; a pass that
# quietly covered fewer gates than the last run is not.
if [ "$skipped" -ne 0 ]; then
  printf '%d GATE(S) SKIPPED\n' "$skipped"
fi
exit "$failures"

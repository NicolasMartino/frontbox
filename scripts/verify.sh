#!/usr/bin/env bash
# D1 verification gates from wiki/roadmaps/extraction.roadmap.md and
# wiki/plans/d1-core-cache-runtime.plan.md.
#
# Runnable from anywhere: it locates the repository root from its own path.
set -uo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" || exit 1

failures=0
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

run "fmt"                 cargo fmt --check
run "clippy native"       cargo clippy --all-targets --all-features -- -D warnings
run "tests"               cargo test --all-features
run "build wasm"          cargo build --target wasm32-unknown-unknown --all-features
run "build wasm minimal"  cargo build --target wasm32-unknown-unknown --no-default-features
run "clippy wasm"         cargo clippy --target wasm32-unknown-unknown --all-features -- -D warnings

# Coverage. The threshold is a floor on the whole crate, not a per-file rule: a module of pure
# trait declarations has nothing to execute, and chasing a number there would mean writing tests
# that assert the compiler works.
#
# Skipped rather than failed when the tool is absent, and the skip says so — a gate that reports
# "ok" for work it did not do is worse than no gate.
COVERAGE_FLOOR=80
coverage() {
  cargo llvm-cov --all-features --summary-only \
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
#   - tests/not_send.rs proves the traits accept a `!Send` type, which is the real
#     `Send` gate — a type-level assertion can show a bound is satisfied but never
#     that it is absent, which is why the textual check exists at all;
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
check_absent "no dioxus dependency" grep -ni 'dioxus' Cargo.toml

# Decision 011: the wire format's timestamp rendering is ours, and `chrono` is a test oracle only.
# Asked of the resolved graph rather than of Cargo.toml, because the risk is a date crate arriving
# transitively — which no amount of reading our own manifest would show. `--edges normal` is what
# excludes dev-dependencies, so chrono in `src/rfc3339/tests.rs` stays legal and chrono in `src/`
# does not.
date_crate_in_runtime_graph() {
  cargo tree --edges normal --all-features | grep -iE '(chrono|jiff|^time |[^-]time v)'
}
check_absent "no date library in the runtime dependency graph" date_crate_in_runtime_graph

check_absent "no RepForge entity names in core" \
  grep -rnE '\b(Exercise|WorkoutSession|WorkoutTemplate|SessionExercise|MuscleGroup|Preferences)\b' src/

# Comment lines are excluded: the decision is discussed in prose in several places, and matching
# that prose would make the gate cry wolf until nobody reads it.
no_send_bound() {
  grep -rn --include='*.rs' -E '(\+|:)[[:space:]]*Send\b' src/ \
    | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//'
}
check_absent "no Send bound in public core" no_send_bound

printf '\n'
if [ "$failures" -eq 0 ]; then
  printf 'ALL GATES PASSED\n'
else
  printf '%d GATE(S) FAILED\n' "$failures"
fi
exit "$failures"

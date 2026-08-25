#!/usr/bin/env bash
# D1 verification gates from wiki/roadmaps/extraction.roadmap.md and
# wiki/plans/d1-core-cache-runtime.plan.md. Run from the repository root.
set -uo pipefail

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

run "fmt"                 cargo fmt --check
run "clippy native"       cargo clippy --all-targets --all-features -- -D warnings
run "tests"               cargo test --all-features
run "build wasm"          cargo build --target wasm32-unknown-unknown --all-features
run "build wasm minimal"  cargo build --target wasm32-unknown-unknown --no-default-features
run "clippy wasm"         cargo clippy --target wasm32-unknown-unknown --all-features -- -D warnings

# Prose gates from the D1 plan, made executable.

printf '\n=== no dioxus dependency\n'
if grep -qi 'dioxus' Cargo.toml; then
  printf '    FAILED: dioxus appears in Cargo.toml\n'; failures=$((failures + 1))
else
  printf '    ok\n'
fi

printf '\n=== no RepForge entity names in core\n'
if grep -rnE '\b(Exercise|WorkoutSession|WorkoutTemplate|SessionExercise|MuscleGroup|Preferences)\b' src/; then
  printf '    FAILED: RepForge entity name in src/\n'; failures=$((failures + 1))
else
  printf '    ok\n'
fi

# A type-level assertion cannot prove a bound's *absence*, so the honest check is textual.
# It is backed by tests/not_send.rs, which holds an Rc across an await of every trait method.
printf '\n=== no Send bound in public core\n'
# Comment lines are excluded: the decision is discussed in prose in several places, and matching
# that prose would make the gate unfailable-in-practice by crying wolf.
if grep -rn --include='*.rs' -E '(\+|:)[[:space:]]*Send\b' src/ | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//'; then
  printf '    FAILED: Send bound in src/\n'; failures=$((failures + 1))
else
  printf '    ok\n'
fi

printf '\n'
if [ "$failures" -eq 0 ]; then
  printf 'ALL GATES PASSED\n'
else
  printf '%d GATE(S) FAILED\n' "$failures"
fi
exit "$failures"

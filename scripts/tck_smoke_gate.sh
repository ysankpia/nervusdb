#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

PROFILE="${TCK_SMOKE_PROFILE:-core}"

run_case() {
  local title="$1"
  shift
  echo ""
  echo "[tck-smoke] ${title}"
  "$@"
}

run_core() {
  run_case "binding validation regression" \
    cargo test -p nervusdb --test t332_binding_validation

  run_case "variable-length direction regression" \
    cargo test -p nervusdb --test t333_varlen_direction

  run_case "merge executor regression" \
    cargo test -p nervusdb --test merge_test

  run_case "merge idempotency regression" \
    cargo test -p nervusdb --test t105_merge_test

  run_case "merge on-create/on-match regression" \
    cargo test -p nervusdb --test t323_merge_semantics

  run_case "tck Match2 variable conflict scenario" \
    cargo test -p nervusdb --test tck_harness -- \
      --name "Fail when a node has the same variable in a preceding MATCH" \
      --input tests/opencypher_tck/tck/features/clauses/match/Match2.feature

  run_case "tck Match6 variable conflict scenario" \
    cargo test -p nervusdb --test tck_harness -- \
      --name "Fail when a node has the same variable in a preceding MATCH" \
      --input tests/opencypher_tck/tck/features/clauses/match/Match6.feature
}

run_extended() {
  run_case "direction clause regression" \
    cargo test -p nervusdb --test t315_direction

  run_case "variable-length suite regression" \
    cargo test -p nervusdb --test t60_variable_length_test

  run_case "literals feature sanity" \
    cargo test -p nervusdb --test tck_harness -- \
      --input tests/opencypher_tck/tck/features/expressions/literals/Literals1.feature
}

echo "[tck-smoke] profile=${PROFILE}"
case "${PROFILE}" in
  core)
    run_core
    ;;
  extended)
    run_core
    run_extended
    ;;
  *)
    echo "unknown TCK_SMOKE_PROFILE: ${PROFILE}" >&2
    echo "supported values: core | extended" >&2
    exit 2
    ;;
esac

echo ""
echo "[tck-smoke] all checks passed"

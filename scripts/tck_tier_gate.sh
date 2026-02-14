#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TIER="${1:-tier0}"
ALLOW_FAIL="${TCK_ALLOW_FAIL:-0}"
REPORT_DIR="${TCK_REPORT_DIR:-artifacts/tck}"
mkdir -p "$REPORT_DIR"

run_case() {
  local title="$1"
  shift
  echo ""
  echo "[tck-${TIER}] ${title}"
  "$@"
}

run_tck_harness_entry() {
  local feature="$1"
  local scenario="${2:-}"
  if [[ -n "$scenario" ]]; then
    run_case "${feature} :: ${scenario}" \
      cargo test -p nervusdb --test tck_harness -- \
      --input "$feature" \
      --name "$scenario"
  else
    run_case "${feature}" \
      cargo test -p nervusdb --test tck_harness -- \
      --input "$feature"
  fi
}

run_whitelist() {
  local list_file="$1"
  if [[ ! -f "$list_file" ]]; then
    echo "missing whitelist file: $list_file" >&2
    exit 2
  fi

  while IFS='|' read -r feature scenario; do
    feature="${feature%%#*}"
    feature="$(echo "$feature" | xargs)"
    scenario="$(echo "${scenario:-}" | xargs)"
    if [[ -z "$feature" ]]; then
      continue
    fi
    run_tck_harness_entry "$feature" "$scenario"
  done <"$list_file"
}

run_tier0() {
  TCK_SMOKE_PROFILE="${TCK_SMOKE_PROFILE:-core}" bash scripts/tck_smoke_gate.sh
}

run_tier1() {
  run_whitelist "scripts/tck_whitelist/tier1_clauses.txt"
}

run_tier2() {
  run_whitelist "scripts/tck_whitelist/tier2_expressions.txt"
}

run_tier3() {
  local log_file="$REPORT_DIR/tier3-full.log"
  local report_file="$REPORT_DIR/tier3-cluster.md"

  echo "[tck-tier3] running full TCK harness, log -> $log_file"

  set +e
  cargo test -p nervusdb --test tck_harness 2>&1 | tee "$log_file"
  local rc=${PIPESTATUS[0]}
  set -e

  bash scripts/tck_failure_cluster.sh "$log_file" "$report_file"
  echo "[tck-tier3] cluster report -> $report_file"

  if [[ "$ALLOW_FAIL" == "1" ]]; then
    echo "[tck-tier3] allow-fail enabled (exit code was $rc)"
    return 0
  fi

  return "$rc"
}

case "$TIER" in
  tier0)
    run_tier0
    ;;
  tier1)
    run_tier1
    ;;
  tier2)
    run_tier2
    ;;
  tier3)
    run_tier3
    ;;
  *)
    echo "unknown tier: $TIER" >&2
    echo "usage: scripts/tck_tier_gate.sh [tier0|tier1|tier2|tier3]" >&2
    exit 2
    ;;
esac

echo ""
echo "[tck-${TIER}] checks passed"

#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${ROOT_DIR}/scripts/perf_slo_window.sh"

if ! command -v jq >/dev/null 2>&1; then
  echo "[perf-slo-window-fixture] jq not found" >&2
  exit 2
fi

DAYS=(
  "2026-03-02"
  "2026-03-03"
  "2026-03-04"
  "2026-03-05"
  "2026-03-06"
  "2026-03-07"
  "2026-03-08"
)

assert_eq() {
  local expected="$1"
  local actual="$2"
  local message="$3"
  if [ "$expected" != "$actual" ]; then
    echo "[perf-slo-window-fixture] assert failed: ${message} (expected=${expected}, actual=${actual})" >&2
    exit 1
  fi
}

run_expect_rc() {
  local expected="$1"
  shift
  set +e
  "$@"
  local rc=$?
  set -e
  if [ "$rc" -ne "$expected" ]; then
    echo "[perf-slo-window-fixture] rc mismatch: expected=${expected}, actual=${rc}" >&2
    echo "[perf-slo-window-fixture] command: $*" >&2
    exit 1
  fi
}

write_status_file() {
  local file="$1"
  local fail_day="${2:-}"
  local skip_day="${3:-}"
  {
    echo "{"
    for idx in "${!DAYS[@]}"; do
      day="${DAYS[$idx]}"
      pass=true
      if [ -n "$fail_day" ] && [ "$day" = "$fail_day" ]; then
        pass=false
      fi
      if [ -n "$skip_day" ] && [ "$day" = "$skip_day" ]; then
        continue
      fi
      comma=","
      if [ "$idx" -eq "$((${#DAYS[@]} - 1))" ]; then
        comma=""
      fi
      echo "  \"${day}\": ${pass}${comma}"
    done
    echo "}"
  } >"$file"
}

scenario_all_pass_7_days() {
  local tmp_dir report_dir status_file
  tmp_dir="$(mktemp -d)"
  report_dir="${tmp_dir}/report"
  status_file="${tmp_dir}/status.json"
  mkdir -p "$report_dir"
  write_status_file "$status_file"

  run_expect_rc 0 env \
    PERF_REPORT_DIR="$report_dir" \
    PERF_SLO_DAYS=7 \
    bash "$SCRIPT" --date 2026-03-08 --nightly-status-file "$status_file"

  assert_eq "true" "$(jq -r '.window_passed' "${report_dir}/perf-slo-window.json")" "all pass should satisfy window"
  assert_eq "7" "$(jq -r '.consecutive_days' "${report_dir}/perf-slo-window.json")" "all pass consecutive days"
  rm -rf "$tmp_dir"
}

scenario_failure_resets_chain() {
  local tmp_dir report_dir status_file
  tmp_dir="$(mktemp -d)"
  report_dir="${tmp_dir}/report"
  status_file="${tmp_dir}/status.json"
  mkdir -p "$report_dir"
  write_status_file "$status_file" "2026-03-05"

  run_expect_rc 1 env \
    PERF_REPORT_DIR="$report_dir" \
    PERF_SLO_DAYS=7 \
    bash "$SCRIPT" --date 2026-03-08 --nightly-status-file "$status_file"

  assert_eq "false" "$(jq -r '.window_passed' "${report_dir}/perf-slo-window.json")" "failed day should break window"
  assert_eq "3" "$(jq -r '.consecutive_days' "${report_dir}/perf-slo-window.json")" "consecutive days after failure"
  rm -rf "$tmp_dir"
}

scenario_missing_daily_status() {
  local tmp_dir report_dir status_file
  tmp_dir="$(mktemp -d)"
  report_dir="${tmp_dir}/report"
  status_file="${tmp_dir}/status.json"
  mkdir -p "$report_dir"
  write_status_file "$status_file" "" "2026-03-06"

  run_expect_rc 1 env \
    PERF_REPORT_DIR="$report_dir" \
    PERF_SLO_DAYS=7 \
    bash "$SCRIPT" --date 2026-03-08 --nightly-status-file "$status_file"

  assert_eq "missing_status" "$(jq -r '.daily[] | select(.date=="2026-03-06") | .reason' "${report_dir}/perf-slo-window.json")" "missing daily status reason"
  rm -rf "$tmp_dir"
}

main() {
  scenario_all_pass_7_days
  scenario_failure_resets_chain
  scenario_missing_daily_status
  echo "[perf-slo-window-fixture] PASS"
}

main

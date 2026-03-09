#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${ROOT_DIR}/scripts/perf_slo_gate.sh"

if ! command -v jq >/dev/null 2>&1; then
  echo "[perf-slo-fixture] jq not found" >&2
  exit 2
fi

assert_eq() {
  local expected="$1"
  local actual="$2"
  local message="$3"
  if [ "$expected" != "$actual" ]; then
    echo "[perf-slo-fixture] assert failed: ${message} (expected=${expected}, actual=${actual})" >&2
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
    echo "[perf-slo-fixture] rc mismatch: expected=${expected}, actual=${rc}" >&2
    echo "[perf-slo-fixture] command: $*" >&2
    exit 1
  fi
}

write_concurrency_json() {
  local path="$1"
  local read_p99_ms="$2"
  local write_p99_ms="$3"
  cat >"$path" <<JSON
{
  "read_query_p99_ms": ${read_p99_ms},
  "write_txn_p99_ms": ${write_p99_ms}
}
JSON
}

write_hnsw_json() {
  local path="$1"
  local p99_us="$2"
  local recall="$3"
  cat >"$path" <<JSON
{
  "p99_us": ${p99_us},
  "recall_at_k": ${recall}
}
JSON
}

scenario_all_pass() {
  local tmp_dir out_dir c_json h_json report
  tmp_dir="$(mktemp -d)"
  out_dir="${tmp_dir}/out"
  c_json="${tmp_dir}/concurrency.json"
  h_json="${tmp_dir}/hnsw.json"

  write_concurrency_json "$c_json" 10.0 20.0
  write_hnsw_json "$h_json" 1000 0.99

  run_expect_rc 0 bash "$SCRIPT" \
    --concurrency-json "$c_json" \
    --hnsw-json "$h_json" \
    --out-dir "$out_dir" \
    --as-of-date "2026-03-08"

  report="${out_dir}/perf-slo-gate-2026-03-08.json"
  assert_eq "true" "$(jq -r '.pass' "$report")" "all pass verdict"
  rm -rf "$tmp_dir"
}

scenario_read_over_threshold() {
  local tmp_dir out_dir c_json h_json report
  tmp_dir="$(mktemp -d)"
  out_dir="${tmp_dir}/out"
  c_json="${tmp_dir}/concurrency.json"
  h_json="${tmp_dir}/hnsw.json"

  write_concurrency_json "$c_json" 121.0 20.0
  write_hnsw_json "$h_json" 1000 0.99

  run_expect_rc 1 bash "$SCRIPT" \
    --concurrency-json "$c_json" \
    --hnsw-json "$h_json" \
    --out-dir "$out_dir" \
    --as-of-date "2026-03-08"

  report="${out_dir}/perf-slo-gate-2026-03-08.json"
  assert_eq "false" "$(jq -r '.checks.read_query_p99_ms.pass' "$report")" "read gate"
  rm -rf "$tmp_dir"
}

scenario_write_over_threshold() {
  local tmp_dir out_dir c_json h_json report
  tmp_dir="$(mktemp -d)"
  out_dir="${tmp_dir}/out"
  c_json="${tmp_dir}/concurrency.json"
  h_json="${tmp_dir}/hnsw.json"

  write_concurrency_json "$c_json" 10.0 181.0
  write_hnsw_json "$h_json" 1000 0.99

  run_expect_rc 1 bash "$SCRIPT" \
    --concurrency-json "$c_json" \
    --hnsw-json "$h_json" \
    --out-dir "$out_dir" \
    --as-of-date "2026-03-08"

  report="${out_dir}/perf-slo-gate-2026-03-08.json"
  assert_eq "false" "$(jq -r '.checks.write_txn_p99_ms.pass' "$report")" "write gate"
  rm -rf "$tmp_dir"
}

scenario_vector_latency_over_threshold() {
  local tmp_dir out_dir c_json h_json report
  tmp_dir="$(mktemp -d)"
  out_dir="${tmp_dir}/out"
  c_json="${tmp_dir}/concurrency.json"
  h_json="${tmp_dir}/hnsw.json"

  write_concurrency_json "$c_json" 10.0 20.0
  write_hnsw_json "$h_json" 221001 0.99

  run_expect_rc 1 bash "$SCRIPT" \
    --concurrency-json "$c_json" \
    --hnsw-json "$h_json" \
    --out-dir "$out_dir" \
    --as-of-date "2026-03-08"

  report="${out_dir}/perf-slo-gate-2026-03-08.json"
  assert_eq "false" "$(jq -r '.checks.vector_search_p99_ms.pass' "$report")" "vector latency gate"
  rm -rf "$tmp_dir"
}

scenario_vector_recall_below_threshold() {
  local tmp_dir out_dir c_json h_json report
  tmp_dir="$(mktemp -d)"
  out_dir="${tmp_dir}/out"
  c_json="${tmp_dir}/concurrency.json"
  h_json="${tmp_dir}/hnsw.json"

  write_concurrency_json "$c_json" 10.0 20.0
  write_hnsw_json "$h_json" 1000 0.94

  run_expect_rc 1 bash "$SCRIPT" \
    --concurrency-json "$c_json" \
    --hnsw-json "$h_json" \
    --out-dir "$out_dir" \
    --as-of-date "2026-03-08"

  report="${out_dir}/perf-slo-gate-2026-03-08.json"
  assert_eq "false" "$(jq -r '.checks.vector_recall_at_k.pass' "$report")" "vector recall gate"
  rm -rf "$tmp_dir"
}

scenario_missing_field() {
  local tmp_dir out_dir c_json h_json report
  tmp_dir="$(mktemp -d)"
  out_dir="${tmp_dir}/out"
  c_json="${tmp_dir}/concurrency.json"
  h_json="${tmp_dir}/hnsw.json"

  cat >"$c_json" <<JSON
{
  "read_query_p99_ms": 10.0
}
JSON
  write_hnsw_json "$h_json" 1000 0.99

  run_expect_rc 1 bash "$SCRIPT" \
    --concurrency-json "$c_json" \
    --hnsw-json "$h_json" \
    --out-dir "$out_dir" \
    --as-of-date "2026-03-08"

  report="${out_dir}/perf-slo-gate-2026-03-08.json"
  assert_eq "false" "$(jq -r '.checks.write_txn_p99_ms.pass' "$report")" "missing write field gate"
  rm -rf "$tmp_dir"
}

main() {
  scenario_all_pass
  scenario_read_over_threshold
  scenario_write_over_threshold
  scenario_vector_latency_over_threshold
  scenario_vector_recall_below_threshold
  scenario_missing_field
  echo "[perf-slo-fixture] PASS"
}

main

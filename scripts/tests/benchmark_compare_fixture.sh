#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${ROOT_DIR}/scripts/benchmark_compare.sh"

assert_contains() {
  local needle="$1"
  local haystack_file="$2"
  local message="$3"
  if ! rg -Fq -- "$needle" "$haystack_file"; then
    echo "[benchmark-compare-fixture] assert failed: ${message}" >&2
    echo "[benchmark-compare-fixture] missing: ${needle}" >&2
    exit 1
  fi
}

main() {
  local tmp_dir input_json perf_gate_json perf_window_json out_dir report
  tmp_dir="$(mktemp -d)"
  input_json="${tmp_dir}/nervus.json"
  perf_gate_json="${tmp_dir}/perf-slo-gate.json"
  perf_window_json="${tmp_dir}/perf-slo-window.json"
  out_dir="${tmp_dir}/out"

  cat >"$input_json" <<'JSON'
{"nodes":50000,"degree":8,"iters":2000,"neighbors_hot_m2_edges_per_sec":25000000.0,"neighbors_hot_m2_p95_us":0.29,"neighbors_hot_m2_p99_us":0.38,"neighbors_cold_m2_edges_per_sec":15000000.0,"neighbors_cold_m2_p95_us":0.67,"neighbors_cold_m2_p99_us":1.33}
JSON

  cat >"$perf_gate_json" <<'JSON'
{"pass":true,"metrics":{"read_query_p99_ms":0.001458,"write_txn_p99_ms":53.50225,"vector_search_p99_ms":6.738917,"vector_recall_at_k":0.994}}
JSON

  cat >"$perf_window_json" <<'JSON'
{"window_passed":true,"consecutive_days":7,"required_days":7,"as_of_date":"2026-03-26"}
JSON

  BENCH_COMPARE_INPUT_JSON="$input_json" \
  BENCH_PERF_GATE_JSON="$perf_gate_json" \
  BENCH_PERF_WINDOW_JSON="$perf_window_json" \
  BENCH_OUT_DIR="$out_dir" \
  bash "$SCRIPT"

  report="$(find "$out_dir" -name 'benchmark-report-*.md' | head -n 1)"
  test -n "$report"

  assert_contains "## Beta Release Gate Binding" "$report" "gate binding section"
  assert_contains "- Perf SLO gate: **PASS**" "$report" "gate pass status"
  assert_contains "- Perf window: **PASS** (7 / 7 days as of 2026-03-26)" "$report" "window pass status"
  assert_contains "- Read query P99: 0.001458 ms" "$report" "read metric"
  assert_contains "- Vector recall@k: 0.994000" "$report" "recall metric"

  rm -rf "$tmp_dir"
  echo "[benchmark-compare-fixture] PASS"
}

main

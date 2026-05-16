#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${ROOT_DIR}/scripts/perf_slo_summary.sh"

assert_contains() {
  local needle="$1"
  local haystack="$2"
  local message="$3"
  if [[ "$haystack" != *"$needle"* ]]; then
    echo "[perf-slo-summary-fixture] assert failed: ${message}" >&2
    echo "[perf-slo-summary-fixture] expected substring: ${needle}" >&2
    exit 1
  fi
}

scenario_current_schema_pass() {
  local tmp_dir gate_json output
  tmp_dir="$(mktemp -d)"
  gate_json="${tmp_dir}/gate.json"

  cat >"$gate_json" <<'JSON'
{
  "pass": true,
  "checks": {
    "read_query_p99_ms": {
      "pass": true,
      "observed": 0.001458,
      "threshold": 120.0,
      "op": "<="
    },
    "write_txn_p99_ms": {
      "pass": true,
      "observed": 53.50225,
      "threshold": 180.0,
      "op": "<="
    },
    "vector_search_p99_ms": {
      "pass": true,
      "observed": 6.738917,
      "threshold": 220.0,
      "op": "<="
    },
    "vector_recall_at_k": {
      "pass": true,
      "observed": 0.994,
      "threshold": 0.95,
      "op": ">="
    }
  }
}
JSON

  output="$(bash "$SCRIPT" --date 2026-03-26 --gate-json "$gate_json")"
  assert_contains "- Overall: PASS" "$output" "current schema overall status"
  assert_contains "- Read p99: 0.001458 ms / threshold <= 120.000000 ms" "$output" "current schema read metric"
  assert_contains "- Recall@k: 0.994000 / threshold >= 0.950000" "$output" "current schema recall metric"
  rm -rf "$tmp_dir"
}

scenario_legacy_schema_pass() {
  local tmp_dir gate_json output
  tmp_dir="$(mktemp -d)"
  gate_json="${tmp_dir}/gate.json"

  cat >"$gate_json" <<'JSON'
{
  "overall_status": "pass",
  "checks": {
    "read_query_p99_ms": {
      "actual_ms": 1.25,
      "threshold_ms": 120.0
    },
    "write_txn_p99_ms": {
      "actual_ms": 80.5,
      "threshold_ms": 180.0
    },
    "vector_search_p99_ms": {
      "actual_ms": 9.75,
      "threshold_ms": 220.0
    },
    "vector_recall_at_k": {
      "actual": 0.97,
      "threshold_min": 0.95
    }
  }
}
JSON

  output="$(bash "$SCRIPT" --date 2026-03-26 --gate-json "$gate_json")"
  assert_contains "- Overall: PASS" "$output" "legacy schema overall status"
  assert_contains "- Vector p99: 9.750000 ms / threshold <= 220.000000 ms" "$output" "legacy schema vector metric"
  rm -rf "$tmp_dir"
}

scenario_missing_gate_fallback() {
  local tmp_dir output concurrency_json hnsw_json hnsw_log
  tmp_dir="$(mktemp -d)"
  concurrency_json="${tmp_dir}/concurrency.json"
  hnsw_json="${tmp_dir}/hnsw.json"
  hnsw_log="${tmp_dir}/hnsw.log"

  cat >"$concurrency_json" <<'JSON'
{"read_query_p99_ms": 10.0}
JSON
  cat >"$hnsw_json" <<'JSON'
{"p99_us": 1500, "recall_at_k": 0.99}
JSON
  cat >"$hnsw_log" <<'LOG'
line one
line two
line three
LOG

  output="$(bash "$SCRIPT" \
    --date 2026-03-26 \
    --gate-json "${tmp_dir}/missing.json" \
    --concurrency-json "$concurrency_json" \
    --hnsw-json "$hnsw_json" \
    --hnsw-log "$hnsw_log")"
  assert_contains "- Overall: failed before gate report generation" "$output" "fallback overall status"
  assert_contains "- concurrency JSON: present" "$output" "fallback concurrency presence"
  assert_contains "- hnsw JSON payload: {\"p99_us\": 1500, \"recall_at_k\": 0.99}" "$output" "fallback hnsw payload"
  assert_contains "### hnsw_tune log tail" "$output" "fallback log tail heading"
  assert_contains "line three" "$output" "fallback log tail content"
  rm -rf "$tmp_dir"
}

main() {
  scenario_current_schema_pass
  scenario_legacy_schema_pass
  scenario_missing_gate_fallback
  echo "[perf-slo-summary-fixture] PASS"
}

main

#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

if ! command -v jq >/dev/null 2>&1; then
  echo "[perf-bench-fixture] jq not found" >&2
  exit 2
fi

line="$(
  cargo run --example bench_v2 -p nervusdb-storage --release -- \
    --nodes 20 --degree 2 --iters 10 --write-iters 6 \
    | tail -n 1
)"

echo "$line" | jq -e '.write_txn_avg_us' >/dev/null
echo "$line" | jq -e '.write_txn_p95_us' >/dev/null
echo "$line" | jq -e '.write_txn_p99_us' >/dev/null
echo "$line" | jq -e '.write_txn_p99_ms' >/dev/null
echo "$line" | jq -e '.read_query_p99_ms' >/dev/null

echo "[perf-bench-fixture] PASS"

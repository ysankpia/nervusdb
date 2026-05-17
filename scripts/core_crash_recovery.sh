#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/nervusdb-core-crash.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

DB_PATH="$TMP_DIR/core-crash"

iterations=5
batch=64
node_pool=64
rel_pool=8

while [[ $# -gt 0 ]]; do
  case "$1" in
    --iterations) iterations="$2"; shift 2 ;;
    --batch) batch="$2"; shift 2 ;;
    --node-pool) node_pool="$2"; shift 2 ;;
    --rel-pool) rel_pool="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

echo "[core-crash] iterations=$iterations batch=$batch node_pool=$node_pool rel_pool=$rel_pool"
cargo run -p nervusdb-storage --bin nervusdb-v2-crash-test -- \
  driver "$DB_PATH" \
  --iterations "$iterations" \
  --min-ms 2 \
  --max-ms 8 \
  --batch "$batch" \
  --node-pool "$node_pool" \
  --rel-pool "$rel_pool" \
  --verify-retries 20 \
  --verify-backoff-ms 10

echo "[core-crash] ok"


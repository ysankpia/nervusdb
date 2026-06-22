#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

mode="small"
nodes=""
degree=""
iters=""
write_iters=""
custom=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --small) mode="small"; shift ;;
    --large) mode="large"; shift ;;
    --nodes) nodes="$2"; custom=1; shift 2 ;;
    --degree) degree="$2"; custom=1; shift 2 ;;
    --iters) iters="$2"; custom=1; shift 2 ;;
    --write-iters) write_iters="$2"; custom=1; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ "$mode" == "large" ]]; then
  nodes="${nodes:-1000000}"
  degree="${degree:-5}"
  iters="${iters:-5000}"
  write_iters="${write_iters:-500}"
else
  nodes="${nodes:-1000}"
  degree="${degree:-5}"
  iters="${iters:-100}"
  write_iters="${write_iters:-20}"
fi

out_dir="artifacts/core-bench"
mkdir -p "$out_dir"

ts="$(date -u +%Y%m%d-%H%M%S)"
if [[ "$custom" == "1" ]]; then
  label="custom-${nodes}n-${degree}d"
else
  label="$mode"
fi
out_file="$out_dir/core-bench-$label-$ts.json"
log_file="$out_dir/core-bench-$label-$ts.log"

echo "[core-bench] mode=$mode label=$label nodes=$nodes degree=$degree iters=$iters write_iters=$write_iters"
echo "[core-bench] output=$out_file"

set +e
cargo run --example bench_v2 -p nervusdb --release -- \
  --nodes "$nodes" \
  --degree "$degree" \
  --iters "$iters" \
  --write-iters "$write_iters" \
  2>&1 | tee "$log_file"
rc=${PIPESTATUS[0]}
set -e

if [[ "$rc" -ne 0 ]]; then
  echo "[core-bench] benchmark failed; log=$log_file" >&2
  exit "$rc"
fi

json_line="$(grep -E '^\{.*\}$' "$log_file" | tail -n 1 || true)"
if [[ -z "$json_line" ]]; then
  echo "[core-bench] missing JSON benchmark line in $log_file" >&2
  exit 1
fi

for field in \
  stage_open_ms \
  stage_get_schema_ms \
  stage_create_nodes_ms \
  stage_create_edges_ms \
  stage_commit_ms \
  stage_reopen_verify_ms \
  stage_neighbors_hot_ms \
  stage_neighbors_cold_ms \
  stage_property_lookup_scan_ms \
  stage_property_lookup_index_ms \
  stage_write_txn_ms \
  insert_total_ms \
  estimated_kv_writes \
  property_lookup_iters \
  property_lookup_rows \
  property_lookup_scan_p99_us \
  property_lookup_index_p99_us \
  property_lookup_speedup
do
  if [[ "$json_line" != *"\"$field\":"* ]]; then
    echo "[core-bench] missing JSON field: $field" >&2
    exit 1
  fi
done

printf '%s\n' "$json_line" > "$out_file"
echo "[core-bench] wrote $out_file"

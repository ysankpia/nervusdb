#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

mode="small"
nodes=""
degree=""
iters=""
mutation_iters=""
seed="1"
systems=("nervusdb" "sqlite-simple" "sqlite-materialized")
custom=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --small) mode="small"; shift ;;
    --medium) mode="medium"; shift ;;
    --system) systems=("$2"); shift 2 ;;
    --nodes) nodes="$2"; custom=1; shift 2 ;;
    --degree) degree="$2"; custom=1; shift 2 ;;
    --iters) iters="$2"; custom=1; shift 2 ;;
    --mutation-iters) mutation_iters="$2"; custom=1; shift 2 ;;
    --seed) seed="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ "$mode" == "medium" ]]; then
  nodes="${nodes:-100000}"
  degree="${degree:-5}"
  iters="${iters:-10000}"
  mutation_iters="${mutation_iters:-100}"
else
  nodes="${nodes:-1000}"
  degree="${degree:-5}"
  iters="${iters:-200}"
  mutation_iters="${mutation_iters:-20}"
fi

out_dir="artifacts/cross-db-bench"
mkdir -p "$out_dir"

ts="$(date -u +%Y%m%d-%H%M%S)"
if [[ "$custom" == "1" ]]; then
  label="custom-${nodes}n-${degree}d-${iters}i"
else
  label="$mode"
fi

summary_file="$out_dir/cross-db-bench-$label-$ts.ndjson"
: >"$summary_file"

echo "[cross-db-bench] mode=$mode label=$label nodes=$nodes degree=$degree iters=$iters mutation_iters=$mutation_iters seed=$seed"
echo "[cross-db-bench] summary=$summary_file"

for system in "${systems[@]}"; do
  out_file="$out_dir/cross-db-bench-$system-$label-$ts.json"
  log_file="$out_dir/cross-db-bench-$system-$label-$ts.log"

  echo "[cross-db-bench] running system=$system"
  set +e
  cargo run --example cross_db_bench -p nervusdb --release -- \
    --system "$system" \
    --nodes "$nodes" \
    --degree "$degree" \
    --iters "$iters" \
    --mutation-iters "$mutation_iters" \
    --seed "$seed" \
    2>&1 | tee "$log_file"
  rc=${PIPESTATUS[0]}
  set -e

  if [[ "$rc" -ne 0 ]]; then
    echo "[cross-db-bench] benchmark failed for $system; log=$log_file" >&2
    exit "$rc"
  fi

  json_line="$(grep -E '^\{.*\}$' "$log_file" | tail -n 1 || true)"
  if [[ -z "$json_line" ]]; then
    echo "[cross-db-bench] missing JSON benchmark line in $log_file" >&2
    exit 1
  fi

  for field in \
    benchmark_version \
    system \
    profile \
    load_mode \
    nodes \
    edges \
    load_nodes_ms \
    load_edges_ms \
    commit_ms \
    load_total_ms \
    reopen_open_ms \
    reopen_count_verify_ms \
    reopen_verify_ms \
    lookup_p99_us \
    one_hop_cold_edges_per_sec \
    incoming_cold_edges_per_sec \
    two_hop_paths_per_sec \
    update_p99_us \
    detach_delete_p99_us \
    db_bytes \
    db_file_count \
    correctness_hash
  do
    if [[ "$json_line" != *"\"$field\":"* ]]; then
      echo "[cross-db-bench] missing JSON field for $system: $field" >&2
      exit 1
    fi
  done

  printf '%s\n' "$json_line" >"$out_file"
  printf '%s\n' "$json_line" >>"$summary_file"
  echo "[cross-db-bench] wrote $out_file"
done

echo "[cross-db-bench] wrote summary $summary_file"

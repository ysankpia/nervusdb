#!/usr/bin/env bash
set -euo pipefail

nodes=50000
degree=8
iters=2000

while [[ $# -gt 0 ]]; do
  case "$1" in
    --nodes) nodes="$2"; shift 2 ;;
    --degree) degree="$2"; shift 2 ;;
    --iters) iters="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

out_dir="docs/perf/v2"
mkdir -p "$out_dir"

ts="$(date +%Y%m%d-%H%M%S)"
out_file="$out_dir/run-$ts-n${nodes}-d${degree}-i${iters}.json"

echo "[v2 bench] nodes=$nodes degree=$degree iters=$iters"

# Capture the last line (JSON) only.
json_line="$(
  cargo run --example bench_v2 -p nervusdb-v2-storage --release -- \
    --nodes "$nodes" --degree "$degree" --iters "$iters" \
    | tail -n 1
)"

echo "$json_line" > "$out_file"
echo "[v2 bench] wrote $out_file"


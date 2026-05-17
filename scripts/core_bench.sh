#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

mode="small"
nodes=""
degree=""
iters=""
write_iters=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --small) mode="small"; shift ;;
    --large) mode="large"; shift ;;
    --nodes) nodes="$2"; shift 2 ;;
    --degree) degree="$2"; shift 2 ;;
    --iters) iters="$2"; shift 2 ;;
    --write-iters) write_iters="$2"; shift 2 ;;
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
out_file="$out_dir/core-bench-$mode-$ts.json"
log_file="$out_dir/core-bench-$mode-$ts.log"

echo "[core-bench] mode=$mode nodes=$nodes degree=$degree iters=$iters write_iters=$write_iters"
echo "[core-bench] output=$out_file"

set +e
cargo run --example bench_v2 -p nervusdb-storage --release -- \
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

printf '%s\n' "$json_line" > "$out_file"
echo "[core-bench] wrote $out_file"


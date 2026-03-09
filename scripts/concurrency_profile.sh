#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="${PERF_OUT_DIR:-artifacts/perf}"
mkdir -p "$OUT_DIR"

NODES="${PERF_NODES:-50000}"
DEGREE="${PERF_DEGREE:-8}"
ITERS="${PERF_ITERS:-2000}"
WRITE_ITERS="${PERF_WRITE_ITERS:-200}"
TS="$(date +%Y%m%d-%H%M%S)"
JSON_FILE="$OUT_DIR/concurrency-${TS}.json"
REPORT_MD="$OUT_DIR/concurrency-${TS}.md"

echo "[concurrency] running bench_v2 for latency profile"
LINE="$({
  cargo run --example bench_v2 -p nervusdb-storage --release -- \
    --nodes "$NODES" --degree "$DEGREE" --iters "$ITERS" --write-iters "$WRITE_ITERS";
} | tail -n 1)"

echo "$LINE" > "$JSON_FILE"

ROOT_DIR="$ROOT_DIR" JSON_FILE="$JSON_FILE" REPORT_MD="$REPORT_MD" python - <<'PY'
import json
import os
from pathlib import Path
from datetime import datetime, timezone

root = Path(os.environ["ROOT_DIR"])
js = json.loads(Path(os.environ["JSON_FILE"]).read_text())

if "read_query_p99_ms" not in js:
    js["read_query_p99_ms"] = js.get("neighbors_cold_m2_p99_us", 0.0) / 1000.0
if "write_txn_p99_ms" not in js:
    if "write_txn_p99_us" in js:
        js["write_txn_p99_ms"] = js["write_txn_p99_us"] / 1000.0
    else:
        js["write_txn_p99_ms"] = 0.0
if "write_txn_avg_us" not in js:
    js["write_txn_avg_us"] = 0.0
if "write_txn_p95_us" not in js:
    js["write_txn_p95_us"] = 0.0
if "write_txn_p99_us" not in js:
    js["write_txn_p99_us"] = js["write_txn_p99_ms"] * 1000.0

Path(os.environ["JSON_FILE"]).write_text(json.dumps(js, ensure_ascii=False))

baseline_path = root / "docs/perf/v2/concurrency-baseline.json"
latest_ref_path = None
for cand in sorted((root / "docs/perf/v2").glob("run-*.json")):
    latest_ref_path = cand

baseline = None
if baseline_path.exists():
    baseline = json.loads(baseline_path.read_text())

lines = []
lines.append("# Concurrency Read Profile")
lines.append("")
lines.append(f"- Generated at: {datetime.now(timezone.utc).isoformat()}")
lines.append(f"- Nodes: {js['nodes']}")
lines.append(f"- Degree: {js['degree']}")
lines.append(f"- Iterations: {js['iters']}")
lines.append("")
lines.append("## Current Metrics")
lines.append("")
lines.append(f"- Hot throughput (M2): {js['neighbors_hot_m2_edges_per_sec']:.2f} edges/sec")
lines.append(f"- Cold throughput (M2): {js['neighbors_cold_m2_edges_per_sec']:.2f} edges/sec")
lines.append(f"- Hot latency: p95={js['neighbors_hot_m2_p95_us']:.2f}us, p99={js['neighbors_hot_m2_p99_us']:.2f}us")
lines.append(f"- Cold latency: p95={js['neighbors_cold_m2_p95_us']:.2f}us, p99={js['neighbors_cold_m2_p99_us']:.2f}us")
lines.append(f"- Read query P99: {js['read_query_p99_ms']:.4f}ms")
lines.append(f"- Write txn latency: avg={js['write_txn_avg_us']:.2f}us, p95={js['write_txn_p95_us']:.2f}us, p99={js['write_txn_p99_us']:.2f}us ({js['write_txn_p99_ms']:.4f}ms)")
lines.append("")

if baseline:
    lines.append("## Baseline Comparison (concurrency-baseline.json)")
    lines.append("")
    hot_delta = js['neighbors_hot_m2_edges_per_sec'] - baseline.get('neighbors_hot_m2_edges_per_sec', js['neighbors_hot_m2_edges_per_sec'])
    cold_delta = js['neighbors_cold_m2_edges_per_sec'] - baseline.get('neighbors_cold_m2_edges_per_sec', js['neighbors_cold_m2_edges_per_sec'])
    hot_p95_delta = js['neighbors_hot_m2_p95_us'] - baseline.get('neighbors_hot_m2_p95_us', js['neighbors_hot_m2_p95_us'])
    cold_p95_delta = js['neighbors_cold_m2_p95_us'] - baseline.get('neighbors_cold_m2_p95_us', js['neighbors_cold_m2_p95_us'])
    read_p99_delta = js['read_query_p99_ms'] - baseline.get('read_query_p99_ms', js['read_query_p99_ms'])
    write_p99_delta = js['write_txn_p99_ms'] - baseline.get('write_txn_p99_ms', js['write_txn_p99_ms'])
    lines.append(f"- Hot throughput delta: {hot_delta:+.2f} edges/sec")
    lines.append(f"- Cold throughput delta: {cold_delta:+.2f} edges/sec")
    lines.append(f"- Hot p95 delta: {hot_p95_delta:+.2f}us")
    lines.append(f"- Cold p95 delta: {cold_p95_delta:+.2f}us")
    lines.append(f"- Read query p99 delta: {read_p99_delta:+.4f}ms")
    lines.append(f"- Write txn p99 delta: {write_p99_delta:+.4f}ms")
    lines.append("")
else:
    lines.append("## Baseline Comparison")
    lines.append("")
    lines.append("- No concurrency baseline found; current run will be saved as baseline.")
    lines.append("")

if latest_ref_path:
    ref_js = json.loads(latest_ref_path.read_text())
    if 'neighbors_hot_m2_edges_per_sec' in ref_js:
        lines.append("## Reference Comparison (historical run-*.json)")
        lines.append("")
        lines.append(f"- Reference file: {latest_ref_path.name}")
        lines.append(
            f"- Hot throughput delta vs reference: {js['neighbors_hot_m2_edges_per_sec'] - ref_js['neighbors_hot_m2_edges_per_sec']:+.2f} edges/sec"
        )
        lines.append(
            f"- Cold throughput delta vs reference: {js['neighbors_cold_m2_edges_per_sec'] - ref_js['neighbors_cold_m2_edges_per_sec']:+.2f} edges/sec"
        )
        lines.append("")

Path(os.environ["REPORT_MD"]).write_text("\n".join(lines) + "\n")

if not baseline_path.exists():
    baseline_path.write_text(json.dumps(js, ensure_ascii=False))
PY

echo "[concurrency] wrote: $JSON_FILE"
echo "[concurrency] wrote: $REPORT_MD"

mkdir -p docs/perf/v2
cp "$JSON_FILE" "docs/perf/v2/concurrency-${TS}.json"
cp "$REPORT_MD" "docs/perf/v2/concurrency-${TS}.md"
cp "$REPORT_MD" "docs/perf/v2/concurrency-latest.md"
echo "[concurrency] synced docs/perf/v2 artifacts"

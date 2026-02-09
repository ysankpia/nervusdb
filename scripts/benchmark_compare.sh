#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="${BENCH_OUT_DIR:-artifacts/bench}"
mkdir -p "$OUT_DIR"

NODES="${BENCH_NODES:-50000}"
DEGREE="${BENCH_DEGREE:-8}"
ITERS="${BENCH_ITERS:-2000}"
TS="$(date +%Y%m%d-%H%M%S)"
NERVUS_JSON="$OUT_DIR/nervusdb-${TS}.json"
REPORT_MD="$OUT_DIR/benchmark-report-${TS}.md"

echo "[bench-compare] running NervusDB benchmark"
NERVUS_LINE="$({
  cargo run --example bench_v2 -p nervusdb-v2-storage --release -- \
    --nodes "$NODES" --degree "$DEGREE" --iters "$ITERS";
} | tail -n 1)"

echo "$NERVUS_LINE" > "$NERVUS_JSON"

DOCKER_AVAILABLE=0
if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  DOCKER_AVAILABLE=1
fi

NEO4J_STATUS="not-run"
MEMGRAPH_STATUS="not-run"
if [[ "$DOCKER_AVAILABLE" == "1" ]]; then
  NEO4J_STATUS="pending-manual"
  MEMGRAPH_STATUS="pending-manual"
fi

NERVUS_JSON="$NERVUS_JSON" REPORT_MD="$REPORT_MD" NEO4J_STATUS="$NEO4J_STATUS" MEMGRAPH_STATUS="$MEMGRAPH_STATUS" python - <<'PY'
import json
import os
from pathlib import Path
from datetime import datetime, timezone

raw = json.loads(Path(os.environ["NERVUS_JSON"]).read_text())
neo = os.environ["NEO4J_STATUS"]
mem = os.environ["MEMGRAPH_STATUS"]

lines = []
lines.append("# Benchmark Compare Report")
lines.append("")
lines.append(f"- Generated at: {datetime.now(timezone.utc).isoformat()}")
lines.append(f"- Nodes: {raw['nodes']}")
lines.append(f"- Degree: {raw['degree']}")
lines.append(f"- Iterations: {raw['iters']}")
lines.append("")
lines.append("## NervusDB")
lines.append("")
lines.append(f"- Raw JSON: {Path(os.environ['NERVUS_JSON']).as_posix()}")
lines.append(f"- Hot (M2): {raw.get('neighbors_hot_m2_edges_per_sec', 0):.2f} edges/sec, p95={raw.get('neighbors_hot_m2_p95_us', 0):.2f}us, p99={raw.get('neighbors_hot_m2_p99_us', 0):.2f}us")
lines.append(f"- Cold (M2): {raw.get('neighbors_cold_m2_edges_per_sec', 0):.2f} edges/sec, p95={raw.get('neighbors_cold_m2_p95_us', 0):.2f}us, p99={raw.get('neighbors_cold_m2_p99_us', 0):.2f}us")
lines.append("")
lines.append("```json")
lines.append(Path(os.environ["NERVUS_JSON"]).read_text().strip())
lines.append("```")
lines.append("")
lines.append("## Neo4j")
lines.append("")
lines.append(f"- Status: **{neo}**")
lines.append("- Note: Docker-backed compare harness scaffold is in place; load/query commands are pending incremental hardening.")
lines.append("")
lines.append("## Memgraph")
lines.append("")
lines.append(f"- Status: **{mem}**")
lines.append("- Note: Docker-backed compare harness scaffold is in place; load/query commands are pending incremental hardening.")
lines.append("")
lines.append("## Next Actions")
lines.append("")
lines.append("1. Add deterministic import dataset generation.")
lines.append("2. Add unified query set (1-hop/2-hop/aggregation/path).")
lines.append("3. Capture memory and P95/P99 latency in JSON artifacts.")

Path(os.environ["REPORT_MD"]).write_text("\n".join(lines) + "\n")
PY

echo "[bench-compare] wrote: $NERVUS_JSON"
echo "[bench-compare] wrote: $REPORT_MD"

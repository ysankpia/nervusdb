#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="${BENCH_OUT_DIR:-artifacts/bench}"
mkdir -p "$OUT_DIR"

NODES="${BENCH_NODES:-50000}"
DEGREE="${BENCH_DEGREE:-8}"
ITERS="${BENCH_ITERS:-2000}"
INPUT_JSON="${BENCH_COMPARE_INPUT_JSON:-}"
PERF_GATE_JSON="${BENCH_PERF_GATE_JSON:-}"
PERF_WINDOW_JSON="${BENCH_PERF_WINDOW_JSON:-}"
TS="$(date +%Y%m%d-%H%M%S)"
NERVUS_JSON="$OUT_DIR/nervusdb-${TS}.json"
REPORT_MD="$OUT_DIR/benchmark-report-${TS}.md"

if [[ -n "$INPUT_JSON" ]]; then
  echo "[bench-compare] using provided NervusDB benchmark JSON: $INPUT_JSON"
  cp "$INPUT_JSON" "$NERVUS_JSON"
else
  echo "[bench-compare] running NervusDB benchmark"
  NERVUS_LINE="$({
    cargo run --example bench_v2 -p nervusdb-storage --release -- \
      --nodes "$NODES" --degree "$DEGREE" --iters "$ITERS";
  } | tail -n 1)"

  echo "$NERVUS_LINE" > "$NERVUS_JSON"
fi

if [[ -z "$PERF_GATE_JSON" ]]; then
  PERF_GATE_JSON="$(find artifacts/perf -name 'perf-slo-gate-*.json' -print 2>/dev/null | sort | tail -n 1 || true)"
fi
if [[ -z "$PERF_WINDOW_JSON" ]]; then
  PERF_WINDOW_JSON="artifacts/perf/perf-slo-window.json"
fi

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

NERVUS_JSON="$NERVUS_JSON" REPORT_MD="$REPORT_MD" NEO4J_STATUS="$NEO4J_STATUS" MEMGRAPH_STATUS="$MEMGRAPH_STATUS" PERF_GATE_JSON="$PERF_GATE_JSON" PERF_WINDOW_JSON="$PERF_WINDOW_JSON" python - <<'PY'
import json
import os
from pathlib import Path
from datetime import datetime, timezone

raw = json.loads(Path(os.environ["NERVUS_JSON"]).read_text())
neo = os.environ["NEO4J_STATUS"]
mem = os.environ["MEMGRAPH_STATUS"]
perf_gate_path = Path(os.environ["PERF_GATE_JSON"]) if os.environ["PERF_GATE_JSON"] else None
perf_window_path = Path(os.environ["PERF_WINDOW_JSON"]) if os.environ["PERF_WINDOW_JSON"] else None

perf_gate = None
if perf_gate_path and perf_gate_path.exists():
    perf_gate = json.loads(perf_gate_path.read_text())

perf_window = None
if perf_window_path and perf_window_path.exists():
    perf_window = json.loads(perf_window_path.read_text())

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
lines.append("## Beta Release Gate Binding")
lines.append("")
if perf_gate:
    gate_pass = "PASS" if perf_gate.get("pass") else "FAIL"
    metrics = perf_gate.get("metrics", {})
    lines.append(f"- Perf SLO gate: **{gate_pass}**")
    lines.append(f"- Read query P99: {metrics.get('read_query_p99_ms', 'n/a'):.6f} ms" if isinstance(metrics.get('read_query_p99_ms'), (int, float)) else "- Read query P99: n/a")
    lines.append(f"- Write transaction P99: {metrics.get('write_txn_p99_ms', 'n/a'):.6f} ms" if isinstance(metrics.get('write_txn_p99_ms'), (int, float)) else "- Write transaction P99: n/a")
    lines.append(f"- Vector search P99: {metrics.get('vector_search_p99_ms', 'n/a'):.6f} ms" if isinstance(metrics.get('vector_search_p99_ms'), (int, float)) else "- Vector search P99: n/a")
    lines.append(f"- Vector recall@k: {metrics.get('vector_recall_at_k', 'n/a'):.6f}" if isinstance(metrics.get('vector_recall_at_k'), (int, float)) else "- Vector recall@k: n/a")
else:
    lines.append("- Perf SLO gate: **not-linked**")
    lines.append("- Note: no perf gate JSON found for this compare run.")

if perf_window:
    window_pass = "PASS" if perf_window.get("window_passed") else "FAIL"
    lines.append(
        f"- Perf window: **{window_pass}** ({perf_window.get('consecutive_days', 'n/a')} / {perf_window.get('required_days', 'n/a')} days as of {perf_window.get('as_of_date', 'n/a')})"
    )
else:
    lines.append("- Perf window: **not-linked**")
    lines.append("- Note: no perf window JSON found for this compare run.")
lines.append("")
lines.append("## Next Actions")
lines.append("")
lines.append("1. Add deterministic import dataset generation.")
lines.append("2. Add unified query set (1-hop/2-hop/aggregation/path).")
lines.append("3. Keep benchmark evidence paired with perf gate/window artifacts for release-readiness reviews.")

Path(os.environ["REPORT_MD"]).write_text("\n".join(lines) + "\n")
PY

echo "[bench-compare] wrote: $NERVUS_JSON"
echo "[bench-compare] wrote: $REPORT_MD"

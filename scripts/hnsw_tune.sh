#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="${HNSW_OUT_DIR:-artifacts/hnsw}"
mkdir -p "$OUT_DIR"

NODES="${HNSW_TUNE_NODES:-2000}"
DIM="${HNSW_TUNE_DIM:-16}"
QUERIES="${HNSW_TUNE_QUERIES:-100}"
K="${HNSW_TUNE_K:-10}"
M_LIST="${HNSW_TUNE_M_LIST:-8,16,32}"
EF_CONSTRUCTION_LIST="${HNSW_TUNE_EF_CONSTRUCTION_LIST:-100,200}"
EF_SEARCH_LIST="${HNSW_TUNE_EF_SEARCH_LIST:-64,128,256}"

TS="$(date +%Y%m%d-%H%M%S)"
NDJSON_FILE="$OUT_DIR/hnsw-tune-${TS}.ndjson"
REPORT_MD="$OUT_DIR/hnsw-tune-${TS}.md"

: > "$NDJSON_FILE"

split_csv() {
  echo "$1" | tr ',' ' '
}

for m in $(split_csv "$M_LIST"); do
  for efc in $(split_csv "$EF_CONSTRUCTION_LIST"); do
    for efs in $(split_csv "$EF_SEARCH_LIST"); do
      echo "[hnsw-tune] running m=$m ef_construction=$efc ef_search=$efs"
      line="$({
        cargo run --example hnsw_tune -p nervusdb-storage --release -- \
          --nodes "$NODES" --dim "$DIM" --queries "$QUERIES" --k "$K" \
          --m "$m" --ef-construction "$efc" --ef-search "$efs";
      } | tail -n 1)"
      echo "$line" >> "$NDJSON_FILE"
    done
  done
done

NDJSON_FILE="$NDJSON_FILE" REPORT_MD="$REPORT_MD" NODES="$NODES" DIM="$DIM" QUERIES="$QUERIES" K="$K" python - <<'PY'
import json
import os
from pathlib import Path
from datetime import datetime, timezone

ndjson = Path(os.environ["NDJSON_FILE"])
report = Path(os.environ["REPORT_MD"])


def score(item):
    return (-item["recall_at_k"], item["p95_us"], item["p99_us"])

rows = [json.loads(line) for line in ndjson.read_text().splitlines() if line.strip()]
rows.sort(key=score)
best = rows[0] if rows else None

lines = []
lines.append("# HNSW Tuning Report")
lines.append("")
lines.append(f"- Generated at: {datetime.now(timezone.utc).isoformat()}")
lines.append(f"- Nodes: {os.environ['NODES']}")
lines.append(f"- Dim: {os.environ['DIM']}")
lines.append(f"- Queries: {os.environ['QUERIES']}")
lines.append(f"- K: {os.environ['K']}")
lines.append("")

if best:
    lines.append("## Recommended Defaults")
    lines.append("")
    lines.append(
        f"- Suggested params: M={best['m']}, efConstruction={best['ef_construction']}, efSearch={best['ef_search']}"
    )
    lines.append(
        f"- Metrics: recall@{best['k']}={best['recall_at_k']:.4f}, p95={best['p95_us']:.2f}us, p99={best['p99_us']:.2f}us"
    )
    lines.append("")

lines.append("## Sweep Results")
lines.append("")
lines.append("| M | efConstruction | efSearch | recall@k | avg us | p95 us | p99 us | memory proxy bytes |")
lines.append("|---:|---:|---:|---:|---:|---:|---:|---:|")
for row in rows:
    lines.append(
        f"| {row['m']} | {row['ef_construction']} | {row['ef_search']} | {row['recall_at_k']:.4f} | {row['avg_us']:.2f} | {row['p95_us']:.2f} | {row['p99_us']:.2f} | {row['memory_proxy_bytes']} |"
    )

report.write_text("\n".join(lines) + "\n")
PY

echo "[hnsw-tune] wrote: $NDJSON_FILE"
echo "[hnsw-tune] wrote: $REPORT_MD"

if [[ "${HNSW_SYNC_DOCS:-1}" == "1" ]]; then
  mkdir -p docs/perf/v2
  cp "$NDJSON_FILE" "docs/perf/v2/hnsw-tune-${TS}.ndjson"
  cp "$REPORT_MD" "docs/perf/v2/hnsw-tune-${TS}.md"
  cp "$REPORT_MD" "docs/perf/v2/hnsw-default-recommendation.md"
  echo "[hnsw-tune] synced docs/perf/v2 artifacts"
fi

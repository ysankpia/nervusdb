#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CONCURRENCY_JSON=""
HNSW_JSON=""
OUT_DIR="${PERF_OUT_DIR:-artifacts/perf}"
AS_OF_DATE="$(date -u +%Y-%m-%d)"

READ_P99_MS="${READ_P99_MS:-120}"
WRITE_P99_MS="${WRITE_P99_MS:-180}"
VECTOR_P99_MS="${VECTOR_P99_MS:-220}"
VECTOR_RECALL_MIN="${VECTOR_RECALL_MIN:-0.95}"

usage() {
  cat <<'USAGE'
Usage:
  scripts/perf_slo_gate.sh [options]

Options:
  --concurrency-json FILE     Input JSON from bench_v2/concurrency profile
  --hnsw-json FILE            Input JSON from hnsw single-run benchmark
  --out-dir DIR               Output directory (default: artifacts/perf)
  --as-of-date YYYY-MM-DD     Report date in UTC (default: today)
  -h, --help                  Show this help

Environment thresholds:
  READ_P99_MS        default 120
  WRITE_P99_MS       default 180
  VECTOR_P99_MS      default 220
  VECTOR_RECALL_MIN  default 0.95
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --concurrency-json)
      shift
      CONCURRENCY_JSON="${1:-}"
      ;;
    --hnsw-json)
      shift
      HNSW_JSON="${1:-}"
      ;;
    --out-dir)
      shift
      OUT_DIR="${1:-}"
      ;;
    --as-of-date)
      shift
      AS_OF_DATE="${1:-}"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "[perf-slo-gate] error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if [ -z "$CONCURRENCY_JSON" ] || [ -z "$HNSW_JSON" ]; then
  echo "[perf-slo-gate] error: --concurrency-json and --hnsw-json are required" >&2
  exit 2
fi

if ! [[ "$AS_OF_DATE" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
  echo "[perf-slo-gate] error: invalid --as-of-date format: $AS_OF_DATE" >&2
  exit 2
fi

mkdir -p "$OUT_DIR"

JSON_OUT="${OUT_DIR}/perf-slo-gate-${AS_OF_DATE}.json"
MD_OUT="${OUT_DIR}/perf-slo-gate-${AS_OF_DATE}.md"

CONCURRENCY_JSON="$CONCURRENCY_JSON" \
HNSW_JSON="$HNSW_JSON" \
JSON_OUT="$JSON_OUT" \
MD_OUT="$MD_OUT" \
AS_OF_DATE="$AS_OF_DATE" \
READ_P99_MS="$READ_P99_MS" \
WRITE_P99_MS="$WRITE_P99_MS" \
VECTOR_P99_MS="$VECTOR_P99_MS" \
VECTOR_RECALL_MIN="$VECTOR_RECALL_MIN" \
python - <<'PY'
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path


def load_json(path: str):
    p = Path(path)
    if not p.exists():
        return None, f"missing_file:{path}"
    try:
        return json.loads(p.read_text()), ""
    except Exception as exc:
        return None, f"invalid_json:{exc}"


def read_numeric(obj, key):
    if obj is None:
        return None, "unavailable"
    if key not in obj:
        return None, f"missing_field:{key}"
    val = obj[key]
    if isinstance(val, (int, float)):
        return float(val), ""
    return None, f"invalid_type:{key}"


concurrency, c_err = load_json(os.environ["CONCURRENCY_JSON"])
hnsw, h_err = load_json(os.environ["HNSW_JSON"])

read_p99_ms, read_reason = read_numeric(concurrency, "read_query_p99_ms")
write_p99_ms, write_reason = read_numeric(concurrency, "write_txn_p99_ms")

vector_p99_us, vector_us_reason = read_numeric(hnsw, "p99_us")
vector_p99_ms = None if vector_p99_us is None else vector_p99_us / 1000.0
vector_p99_reason = vector_us_reason

vector_recall, vector_recall_reason = read_numeric(hnsw, "recall_at_k")

if vector_p99_ms is None and hnsw is not None and "vector_search_p99_ms" in hnsw:
    val = hnsw["vector_search_p99_ms"]
    if isinstance(val, (int, float)):
        vector_p99_ms = float(val)
        vector_p99_reason = ""

read_limit = float(os.environ["READ_P99_MS"])
write_limit = float(os.environ["WRITE_P99_MS"])
vector_limit = float(os.environ["VECTOR_P99_MS"])
vector_recall_min = float(os.environ["VECTOR_RECALL_MIN"])

def pass_le(observed, threshold):
    return observed is not None and observed <= threshold

def pass_ge(observed, threshold):
    return observed is not None and observed >= threshold

checks = {
    "read_query_p99_ms": {
        "pass": pass_le(read_p99_ms, read_limit),
        "observed": read_p99_ms,
        "threshold": read_limit,
        "op": "<=",
        "reason": read_reason,
    },
    "write_txn_p99_ms": {
        "pass": pass_le(write_p99_ms, write_limit),
        "observed": write_p99_ms,
        "threshold": write_limit,
        "op": "<=",
        "reason": write_reason,
    },
    "vector_search_p99_ms": {
        "pass": pass_le(vector_p99_ms, vector_limit),
        "observed": vector_p99_ms,
        "threshold": vector_limit,
        "op": "<=",
        "reason": vector_p99_reason,
    },
    "vector_recall_at_k": {
        "pass": pass_ge(vector_recall, vector_recall_min),
        "observed": vector_recall,
        "threshold": vector_recall_min,
        "op": ">=",
        "reason": vector_recall_reason,
    },
}

overall_pass = all(item["pass"] for item in checks.values())
generated_at = datetime.now(timezone.utc).isoformat()

payload = {
    "as_of_date": os.environ["AS_OF_DATE"],
    "generated_at": generated_at,
    "pass": overall_pass,
    "inputs": {
        "concurrency_json": os.environ["CONCURRENCY_JSON"],
        "hnsw_json": os.environ["HNSW_JSON"],
    },
    "thresholds": {
        "read_query_p99_ms": read_limit,
        "write_txn_p99_ms": write_limit,
        "vector_search_p99_ms": vector_limit,
        "vector_recall_at_k_min": vector_recall_min,
    },
    "metrics": {
        "read_query_p99_ms": read_p99_ms,
        "write_txn_p99_ms": write_p99_ms,
        "vector_search_p99_ms": vector_p99_ms,
        "vector_recall_at_k": vector_recall,
    },
    "checks": checks,
    "input_errors": [err for err in [c_err, h_err] if err],
}

Path(os.environ["JSON_OUT"]).write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n")

lines = []
lines.append("# Perf SLO Gate")
lines.append("")
lines.append(f"- Generated at: {generated_at}")
lines.append(f"- As of date: {os.environ['AS_OF_DATE']}")
lines.append(f"- Overall pass: {'true' if overall_pass else 'false'}")
lines.append("")
lines.append("| Check | Status | Observed | Threshold | Reason |")
lines.append("|---|---|---:|---:|---|")
for key, item in checks.items():
    status = "PASS" if item["pass"] else "FAIL"
    observed = "null" if item["observed"] is None else f"{item['observed']:.6f}"
    threshold = f"{item['op']} {item['threshold']:.6f}"
    reason = item["reason"] or "-"
    lines.append(f"| {key} | {status} | {observed} | {threshold} | {reason} |")

if payload["input_errors"]:
    lines.append("")
    lines.append("## Input Errors")
    lines.append("")
    for err in payload["input_errors"]:
        lines.append(f"- {err}")

Path(os.environ["MD_OUT"]).write_text("\n".join(lines) + "\n")

sys.exit(0 if overall_pass else 1)
PY

echo "[perf-slo-gate] wrote: $JSON_OUT"
echo "[perf-slo-gate] wrote: $MD_OUT"

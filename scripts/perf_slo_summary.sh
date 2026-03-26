#!/usr/bin/env bash
set -euo pipefail

AS_OF_DATE="$(date -u +%F)"
GATE_JSON=""
CONCURRENCY_JSON=""
HNSW_JSON=""
HNSW_LOG=""

usage() {
  cat <<'USAGE'
Usage:
  scripts/perf_slo_summary.sh [options]

Options:
  --date YYYY-MM-DD         Report date shown in summary (default: today UTC)
  --gate-json FILE          perf_slo_gate JSON file
  --concurrency-json FILE   bench_v2 JSON file
  --hnsw-json FILE          hnsw_tune JSON file
  --hnsw-log FILE           hnsw_tune log file
  -h, --help                Show this help
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --date)
      shift
      AS_OF_DATE="${1:-}"
      ;;
    --gate-json)
      shift
      GATE_JSON="${1:-}"
      ;;
    --concurrency-json)
      shift
      CONCURRENCY_JSON="${1:-}"
      ;;
    --hnsw-json)
      shift
      HNSW_JSON="${1:-}"
      ;;
    --hnsw-log)
      shift
      HNSW_LOG="${1:-}"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "[perf-slo-summary] error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if ! [[ "$AS_OF_DATE" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
  echo "[perf-slo-summary] error: invalid --date format: $AS_OF_DATE" >&2
  exit 2
fi

python - "$AS_OF_DATE" "$GATE_JSON" "$CONCURRENCY_JSON" "$HNSW_JSON" "$HNSW_LOG" <<'PY'
import json
import sys
from pathlib import Path


def load_json(path_text: str):
    if not path_text:
        return None
    path = Path(path_text)
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text())
    except Exception:
        return None


def extract_overall(payload):
    if not isinstance(payload, dict):
        return None
    overall = payload.get("pass")
    if isinstance(overall, bool):
        return overall
    status = payload.get("overall_status")
    if isinstance(status, str):
        lowered = status.strip().lower()
        if lowered in {"pass", "passed", "success", "true"}:
            return True
        if lowered in {"fail", "failed", "error", "false"}:
            return False
    return None


def extract_check(payload, key):
    if not isinstance(payload, dict):
        return None
    checks = payload.get("checks")
    if not isinstance(checks, dict):
        return None
    raw = checks.get(key)
    if not isinstance(raw, dict):
        return None

    observed = raw.get("observed")
    threshold = raw.get("threshold")
    op = raw.get("op")

    if observed is None:
        observed = raw.get("actual_ms")
    if observed is None and key == "vector_recall_at_k":
        observed = raw.get("actual")

    if threshold is None:
        threshold = raw.get("threshold_ms")
    if threshold is None and key == "vector_recall_at_k":
        threshold = raw.get("threshold_min")

    if op is None:
        op = ">=" if key == "vector_recall_at_k" else "<="

    return {
        "observed": observed,
        "threshold": threshold,
        "op": op,
    }


def fmt_num(value):
    if isinstance(value, (int, float)):
        return f"{float(value):.6f}"
    return "n/a"


as_of_date, gate_path, concurrency_path, hnsw_path, hnsw_log_path = sys.argv[1:]
gate = load_json(gate_path)
hnsw = load_json(hnsw_path)

lines = [f"## Perf SLO Nightly Summary ({as_of_date} UTC)"]

keys = [
    ("read_query_p99_ms", "Read p99", "ms"),
    ("write_txn_p99_ms", "Write p99", "ms"),
    ("vector_search_p99_ms", "Vector p99", "ms"),
    ("vector_recall_at_k", "Recall@k", ""),
]

overall = extract_overall(gate)
checks = {key: extract_check(gate, key) for key, _, _ in keys}

if overall is not None and all(item is not None for item in checks.values()):
    lines.append(f"- Overall: {'PASS' if overall else 'FAIL'}")
    for key, label, unit in keys:
        item = checks[key]
        observed = fmt_num(item["observed"])
        threshold = fmt_num(item["threshold"])
        if unit:
            lines.append(
                f"- {label}: {observed} {unit} / threshold {item['op']} {threshold} {unit}"
            )
        else:
            lines.append(
                f"- {label}: {observed} / threshold {item['op']} {threshold}"
            )
else:
    lines.append("- Overall: failed before gate report generation")
    lines.append(
        f"- concurrency JSON: {'present' if concurrency_path and Path(concurrency_path).exists() else 'missing'}"
    )
    lines.append(
        f"- hnsw JSON: {'present' if hnsw_path and Path(hnsw_path).exists() else 'missing'}"
    )
    if hnsw is not None:
        lines.append(f"- hnsw JSON payload: {json.dumps(hnsw, ensure_ascii=False, sort_keys=True)}")
    if hnsw_log_path and Path(hnsw_log_path).exists():
        log_lines = Path(hnsw_log_path).read_text(errors="replace").splitlines()
        lines.append("")
        lines.append("### hnsw_tune log tail")
        lines.extend(log_lines[-25:])

print("\n".join(lines))
PY

#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

REPORT_DIR="${TCK_REPORT_DIR:-artifacts/tck}"
JSON_FILE="${TCK_RATE_JSON_FILE:-$REPORT_DIR/tier3-rate.json}"
THRESHOLD="${TCK_MIN_PASS_RATE:-95}"

if [[ ! -f "$JSON_FILE" ]]; then
  bash scripts/tck_full_rate.sh
fi

rate="$(grep -E '"pass_rate"' "$JSON_FILE" | head -n1 | sed -E 's/.*: ([0-9]+(\.[0-9]+)?).*/\1/')"

if [[ -z "$rate" ]]; then
  echo "[beta-gate] failed: cannot read pass_rate from $JSON_FILE" >&2
  exit 2
fi

ok="$(awk -v r="$rate" -v t="$THRESHOLD" 'BEGIN { if (r + 0 >= t + 0) print 1; else print 0 }')"

echo "[beta-gate] TCK pass rate=${rate}% threshold=${THRESHOLD}%"

if [[ "$ok" != "1" ]]; then
  echo "[beta-gate] BLOCKED: pass rate below threshold"
  exit 1
fi

echo "[beta-gate] PASSED"


#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

REPORT_DIR="${TCK_REPORT_DIR:-artifacts/tck}"
LOG_FILE="${TCK_FULL_LOG_FILE:-$REPORT_DIR/tier3-full.log}"
JSON_FILE="${TCK_RATE_JSON_FILE:-$REPORT_DIR/tier3-rate.json}"
MD_FILE="${TCK_RATE_MD_FILE:-$REPORT_DIR/tier3-rate.md}"
ALLOW_FAIL="${TCK_ALLOW_FAIL:-1}"

mkdir -p "$REPORT_DIR"

if [[ ! -f "$LOG_FILE" ]]; then
  echo "[tck-full-rate] no full log found, running tier3 first"
  TCK_ALLOW_FAIL="$ALLOW_FAIL" TCK_REPORT_DIR="$REPORT_DIR" bash scripts/tck_tier_gate.sh tier3
fi

summary_line="$(grep -E '^[0-9]+ scenarios \([0-9]+ passed, [0-9]+ skipped, [0-9]+ failed\)' "$LOG_FILE" | tail -n1 || true)"

if [[ -z "$summary_line" ]]; then
  echo "[tck-full-rate] failed: cannot parse scenarios summary from $LOG_FILE" >&2
  exit 2
fi

total="$(echo "$summary_line" | sed -E 's/^([0-9]+) scenarios.*$/\1/')"
passed="$(echo "$summary_line" | sed -E 's/^[0-9]+ scenarios \(([0-9]+) passed, ([0-9]+) skipped, ([0-9]+) failed\)$/\1/')"
skipped="$(echo "$summary_line" | sed -E 's/^[0-9]+ scenarios \(([0-9]+) passed, ([0-9]+) skipped, ([0-9]+) failed\)$/\2/')"
failed="$(echo "$summary_line" | sed -E 's/^[0-9]+ scenarios \(([0-9]+) passed, ([0-9]+) skipped, ([0-9]+) failed\)$/\3/')"

rate="$(awk -v p="$passed" -v t="$total" 'BEGIN { if (t == 0) {print "0.00"} else {printf "%.2f", (p * 100.0) / t} }')"
generated_at="$(date -u +'%Y-%m-%dT%H:%M:%SZ')"

cat >"$JSON_FILE" <<JSON
{
  "generated_at": "$generated_at",
  "log_file": "$LOG_FILE",
  "scenarios": {
    "total": $total,
    "passed": $passed,
    "skipped": $skipped,
    "failed": $failed
  },
  "pass_rate": $rate
}
JSON

cat >"$MD_FILE" <<MD
# TCK Tier-3 Full Pass Rate

- Generated at: $generated_at
- Source log: $LOG_FILE

| Metric | Value |
|---|---:|
| Total scenarios | $total |
| Passed scenarios | $passed |
| Skipped scenarios | $skipped |
| Failed scenarios | $failed |
| Pass rate | ${rate}% |
MD

echo "[tck-full-rate] wrote $JSON_FILE"
echo "[tck-full-rate] wrote $MD_FILE"


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

clean_log="$(mktemp)"
trap 'rm -f "$clean_log"' EXIT
tr -d '\000' < "$LOG_FILE" > "$clean_log"

summary_line="$(grep -aE '^[0-9]+ scenarios \(' "$clean_log" | tail -n1 || true)"

parse_from_summary() {
  local line="$1"
  if [[ "$line" =~ ^([0-9]+)[[:space:]]+scenarios[[:space:]]+\(([0-9]+)[[:space:]]+passed,[[:space:]]+([0-9]+)[[:space:]]+skipped,[[:space:]]+([0-9]+)[[:space:]]+failed\)$ ]]; then
    echo "${BASH_REMATCH[1]} ${BASH_REMATCH[2]} ${BASH_REMATCH[3]} ${BASH_REMATCH[4]} summary"
    return 0
  fi
  if [[ "$line" =~ ^([0-9]+)[[:space:]]+scenarios[[:space:]]+\(([0-9]+)[[:space:]]+passed,[[:space:]]+([0-9]+)[[:space:]]+skipped\)$ ]]; then
    echo "${BASH_REMATCH[1]} ${BASH_REMATCH[2]} ${BASH_REMATCH[3]} 0 summary"
    return 0
  fi
  if [[ "$line" =~ ^([0-9]+)[[:space:]]+scenarios[[:space:]]+\(([0-9]+)[[:space:]]+passed,[[:space:]]+([0-9]+)[[:space:]]+failed\)$ ]]; then
    echo "${BASH_REMATCH[1]} ${BASH_REMATCH[2]} 0 ${BASH_REMATCH[3]} summary"
    return 0
  fi
  if [[ "$line" =~ ^([0-9]+)[[:space:]]+scenarios[[:space:]]+\(([0-9]+)[[:space:]]+passed\)$ ]]; then
    echo "${BASH_REMATCH[1]} ${BASH_REMATCH[2]} 0 0 summary"
    return 0
  fi
  return 1
}

parse_from_partial_log() {
  local total failed skipped passed
  total="$(grep -aE '^[[:space:]]+Scenario( Outline)?:' "$clean_log" | wc -l | tr -d ' ')"
  failed="$(grep -aE '^[[:space:]]+Step failed:' "$clean_log" | wc -l | tr -d ' ')"
  skipped="$(grep -aE '^[[:space:]]+Step skipped:' "$clean_log" | wc -l | tr -d ' ')"
  passed=$(( total - failed - skipped ))
  if (( passed < 0 )); then
    passed=0
  fi
  echo "$total $passed $skipped $failed partial"
}

if parsed="$(parse_from_summary "$summary_line" 2>/dev/null)"; then
  read -r total passed skipped failed mode <<<"$parsed"
else
  echo "[tck-full-rate] summary line not found; falling back to partial-log estimation"
  read -r total passed skipped failed mode <<<"$(parse_from_partial_log)"
fi

rate="$(awk -v p="$passed" -v t="$total" 'BEGIN { if (t == 0) {print "0.00"} else {printf "%.2f", (p * 100.0) / t} }')"
generated_at="$(date -u +'%Y-%m-%dT%H:%M:%SZ')"

cat >"$JSON_FILE" <<JSON
{
  "generated_at": "$generated_at",
  "log_file": "$LOG_FILE",
  "mode": "$mode",
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
- Mode: $mode

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

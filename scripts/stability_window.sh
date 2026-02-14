#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

REPORT_DIR="${TCK_REPORT_DIR:-artifacts/tck}"
DAYS="${STABILITY_DAYS:-7}"
THRESHOLD="${TCK_MIN_PASS_RATE:-95}"

if ! [[ "$DAYS" =~ ^[0-9]+$ ]] || [[ "$DAYS" -le 0 ]]; then
  echo "[stability-window] invalid STABILITY_DAYS: $DAYS" >&2
  exit 2
fi

RATE_FILES=()
while IFS= read -r file; do
  RATE_FILES+=("$file")
done < <(find "$REPORT_DIR" -maxdepth 1 -type f -name 'tier3-rate-????-??-??.json' | sort -r)

if [[ "${#RATE_FILES[@]}" -lt "$DAYS" ]]; then
  echo "[stability-window] BLOCKED: requires $DAYS daily snapshots, found ${#RATE_FILES[@]} in $REPORT_DIR"
  exit 1
fi

echo "[stability-window] evaluating latest $DAYS snapshots (threshold=${THRESHOLD}%, failed=0)"
printf "%-12s %-9s %-7s %-7s %s\n" "date" "pass_rate" "failed" "status" "file"

ok_all=1
for ((i=0; i<DAYS; i++)); do
  file="${RATE_FILES[$i]}"
  base="$(basename "$file")"
  date_part="${base#tier3-rate-}"
  date_part="${date_part%.json}"

  rate="$(grep -E '"pass_rate"' "$file" | head -n1 | sed -E 's/.*: ([0-9]+(\.[0-9]+)?).*/\1/')"
  failed="$(grep -E '"failed"' "$file" | head -n1 | sed -E 's/.*: ([0-9]+).*/\1/')"

  if [[ -z "$rate" || -z "$failed" ]]; then
    echo "[stability-window] parse error in $file" >&2
    exit 2
  fi

  rate_ok="$(awk -v r="$rate" -v t="$THRESHOLD" 'BEGIN { if (r + 0 >= t + 0) print 1; else print 0 }')"
  failed_ok=0
  if [[ "$failed" == "0" ]]; then
    failed_ok=1
  fi

  status="PASS"
  if [[ "$rate_ok" != "1" || "$failed_ok" != "1" ]]; then
    status="BLOCKED"
    ok_all=0
  fi

  printf "%-12s %-9s %-7s %-7s %s\n" "$date_part" "${rate}%" "$failed" "$status" "$file"
done

if [[ "$ok_all" != "1" ]]; then
  echo "[stability-window] BLOCKED: not all daily snapshots satisfy gate"
  exit 1
fi

echo "[stability-window] PASSED"

#!/usr/bin/env bash
set -euo pipefail

LOG_FILE="${1:-}"
OUT_FILE="${2:-}"

if [[ -z "$LOG_FILE" ]]; then
  echo "usage: scripts/tck_failure_cluster.sh <log-file> [out-file]" >&2
  exit 2
fi

if [[ ! -f "$LOG_FILE" ]]; then
  echo "log file not found: $LOG_FILE" >&2
  exit 2
fi

if [[ -z "${OUT_FILE}" ]]; then
  OUT_FILE="${LOG_FILE%.log}.cluster.md"
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

FEATURE_FILE="$TMP_DIR/feature_failures.txt"
ERROR_FILE="$TMP_DIR/error_patterns.txt"
SUMMARY_FILE="$TMP_DIR/summary.txt"

awk '
/^Feature: /{feature=substr($0,10)}
/Step failed:/{fail[feature]++}
END{
  for (f in fail) {
    printf "%d\t%s\n", fail[f], f
  }
}
' "$LOG_FILE" | sort -nr > "$FEATURE_FILE"

grep -o "Step panicked\\. Captured output: .*" "$LOG_FILE" \
  | sed 's/Step panicked\\. Captured output: //' \
  | sed 's/[0-9][0-9]*/N/g' \
  | sort | uniq -c | sort -nr > "$ERROR_FILE" || true

grep -E "^\[Summary\]|^[0-9]+ features|^[0-9]+ scenarios|^[0-9]+ steps|parsing error" "$LOG_FILE" > "$SUMMARY_FILE" || true

{
  echo "# TCK Failure Cluster Report"
  echo ""
  echo "- Generated at: $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  echo "- Source log: $LOG_FILE"
  echo ""
  echo "## Summary"
  if [[ -s "$SUMMARY_FILE" ]]; then
    echo '```text'
    cat "$SUMMARY_FILE"
    echo '```'
  else
    echo "No summary block found in log."
  fi
  echo ""
  echo "## Top failing features"
  if [[ -s "$FEATURE_FILE" ]]; then
    echo "| Failures | Feature |"
    echo "|---:|---|"
    head -n 40 "$FEATURE_FILE" | awk -F '\t' '{printf "| %s | %s |\n", $1, $2}'
  else
    echo "No step failures found."
  fi
  echo ""
  echo "## Top error signatures"
  if [[ -s "$ERROR_FILE" ]]; then
    echo "| Count | Error Pattern |"
    echo "|---:|---|"
    head -n 40 "$ERROR_FILE" | awk '{count=$1; $1=""; sub(/^ +/,""); printf "| %s | %s |\n", count, $0}'
  else
    echo "No panicked output patterns found."
  fi
} > "$OUT_FILE"

echo "wrote cluster report: $OUT_FILE"

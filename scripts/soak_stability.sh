#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

MINUTES="${SOAK_MINUTES:-60}"
ITERS_PER_MIN="${SOAK_ITERS_PER_MIN:-30}"
ITERS=$((MINUTES * ITERS_PER_MIN))
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

SOAK_OUT_DIR="${SOAK_OUT_DIR:-artifacts/soak}"
mkdir -p "$SOAK_OUT_DIR"
TS="$(date +%Y%m%d-%H%M%S)"
LOG_FILE="$SOAK_OUT_DIR/soak-${TS}.log"
SUMMARY_FILE="$SOAK_OUT_DIR/soak-summary-${TS}.json"
HISTORY_FILE="$SOAK_OUT_DIR/history.jsonl"

echo "[soak] minutes=$MINUTES iterations=$ITERS" | tee "$LOG_FILE"
START_EPOCH="$(date +%s)"
set +e
cargo run -p nervusdb-storage --bin nervusdb-v2-crash-test -- \
  driver "$TMP_DIR/soak-db" \
  --iterations "$ITERS" --min-ms 2 --max-ms 20 --batch 200 --node-pool 200 --rel-pool 16 \
  --verify-retries 50 --verify-backoff-ms 20 | tee -a "$LOG_FILE"
RC=$?
set -e
END_EPOCH="$(date +%s)"
DURATION_SEC=$((END_EPOCH - START_EPOCH))

STATUS="passed"
if [[ "$RC" -ne 0 ]]; then
  STATUS="failed"
fi

cat > "$SUMMARY_FILE" <<JSON
{"timestamp":"$(date -u +'%Y-%m-%dT%H:%M:%SZ')","minutes":$MINUTES,"iters_per_min":$ITERS_PER_MIN,"iterations":$ITERS,"duration_sec":$DURATION_SEC,"status":"$STATUS"}
JSON
cat "$SUMMARY_FILE" >> "$HISTORY_FILE"

echo "[soak] summary: $SUMMARY_FILE" | tee -a "$LOG_FILE"
echo "[soak] history: $HISTORY_FILE" | tee -a "$LOG_FILE"

if [[ "$RC" -ne 0 ]]; then
  echo "[soak] run failed" | tee -a "$LOG_FILE" >&2
  exit "$RC"
fi

echo "[soak] completed" | tee -a "$LOG_FILE"

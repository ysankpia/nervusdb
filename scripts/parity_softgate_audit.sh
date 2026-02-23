#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

DATE_UTC="${BINDING_PARITY_DATE:-$(date -u +%F)}"
ART_DIR="artifacts/tck"
LOG_FILE="${ART_DIR}/parity-softgate-audit-${DATE_UTC}.log"

mkdir -p "$ART_DIR"
: > "$LOG_FILE"

targets=(
  "examples-test/nervusdb-rust-test/tests/test_capabilities.rs"
  "examples-test/nervusdb-node-test/src/test-capabilities.ts"
  "examples-test/nervusdb-python-test/test_capabilities.py"
)

patterns=(
  "catch_unwind\\s*\\("
  "may not be implemented"
  "limitation observed"
  "\\(note:"
  "unsupported:"
  "query\\(\\) accepted write"
  "write-via-query behavior documented"
  "DELETE connected node succeeded"
)

echo "[softgate] start $(date -u +%Y-%m-%dT%H:%M:%SZ)" | tee -a "$LOG_FILE"
echo "[softgate] targets: ${targets[*]}" | tee -a "$LOG_FILE"

violations=0
for pattern in "${patterns[@]}"; do
  echo "[softgate] pattern: ${pattern}" | tee -a "$LOG_FILE"
  if rg -n --pcre2 "$pattern" "${targets[@]}" | tee -a "$LOG_FILE"; then
    violations=$((violations + 1))
  fi
done

if [[ $violations -gt 0 ]]; then
  echo "[softgate] BLOCKED: found ${violations} soft-gate pattern groups" | tee -a "$LOG_FILE"
  exit 1
fi

echo "[softgate] PASSED: no soft-gate pattern found" | tee -a "$LOG_FILE"

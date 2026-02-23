#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

DATE_UTC="${BINDING_PARITY_DATE:-$(date -u +%F)}"
ART_DIR="artifacts/tck"
LOG_FILE="${ART_DIR}/binding-parity-gate-${DATE_UTC}.log"
JSON_FILE="${ART_DIR}/binding-parity-gate-${DATE_UTC}.json"
MD_FILE="${ART_DIR}/binding-parity-gate-${DATE_UTC}.md"

mkdir -p "$ART_DIR"
: > "$LOG_FILE"

run_step() {
  local name="$1"
  shift
  echo "[binding-parity] ${name} start $(date -u +%Y-%m-%dT%H:%M:%SZ)" | tee -a "$LOG_FILE"
  set +e
  "$@" 2>&1 | tee -a "$LOG_FILE"
  local rc=${PIPESTATUS[0]}
  set -e
  if [[ $rc -eq 0 ]]; then
    echo "[binding-parity] ${name} pass" | tee -a "$LOG_FILE"
  else
    echo "[binding-parity] ${name} fail rc=${rc}" | tee -a "$LOG_FILE"
  fi
  return $rc
}

all_passed=true
examples_status="success"
examples_rc=0
softgate_status="success"
softgate_rc=0
binding_status="success"
binding_rc=0
contract_status="success"
contract_rc=0

set +e
run_step "examples-test" bash examples-test/run_all.sh
examples_rc=$?
set -e
if [[ $examples_rc -ne 0 ]]; then
  examples_status="failed"
  all_passed=false
fi

set +e
run_step "parity-softgate-audit" bash scripts/parity_softgate_audit.sh
softgate_rc=$?
set -e
if [[ $softgate_rc -ne 0 ]]; then
  softgate_status="failed"
  all_passed=false
fi

set +e
run_step "binding-smoke" bash scripts/binding_smoke.sh
binding_rc=$?
set -e
if [[ $binding_rc -ne 0 ]]; then
  binding_status="failed"
  all_passed=false
fi

set +e
run_step "contract-smoke" bash scripts/contract_smoke.sh
contract_rc=$?
set -e
if [[ $contract_rc -ne 0 ]]; then
  contract_status="failed"
  all_passed=false
fi

cat > "$JSON_FILE" <<JSON
{
  "date": "${DATE_UTC}",
  "generated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "all_passed": ${all_passed},
  "checks": {
    "examples_test": { "status": "${examples_status}", "exit_code": ${examples_rc} },
    "parity_softgate_audit": { "status": "${softgate_status}", "exit_code": ${softgate_rc} },
    "binding_smoke": { "status": "${binding_status}", "exit_code": ${binding_rc} },
    "contract_smoke": { "status": "${contract_status}", "exit_code": ${contract_rc} }
  }
}
JSON

cat > "$MD_FILE" <<MD
# Binding Parity Gate (${DATE_UTC})

- generated_at: $(date -u +%Y-%m-%dT%H:%M:%SZ)
- all_passed: ${all_passed}

| Check | Status | Exit Code |
|---|---|---:|
| examples-test/run_all.sh | ${examples_status} | ${examples_rc} |
| scripts/parity_softgate_audit.sh | ${softgate_status} | ${softgate_rc} |
| scripts/binding_smoke.sh | ${binding_status} | ${binding_rc} |
| scripts/contract_smoke.sh | ${contract_status} | ${contract_rc} |

Artifacts:
- ${JSON_FILE}
- ${MD_FILE}
- ${LOG_FILE}
MD

if [[ "$all_passed" != "true" ]]; then
  echo "[binding-parity] BLOCKED: parity gate failed" >&2
  exit 1
fi

echo "[binding-parity] PASSED"

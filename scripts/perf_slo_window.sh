#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

REPORT_DIR="${PERF_REPORT_DIR:-artifacts/perf}"
REQUIRED_DAYS="${PERF_SLO_DAYS:-7}"
AS_OF_DATE="$(date -u +%Y-%m-%d)"
GITHUB_REPO="${PERF_SLO_GITHUB_REPO:-}"
GITHUB_TOKEN_ENV="${PERF_SLO_GITHUB_TOKEN_ENV:-GITHUB_TOKEN}"
NIGHTLY_STATUS_FILE=""

usage() {
  cat <<'USAGE'
Usage:
  scripts/perf_slo_window.sh [options]

Options:
  --date YYYY-MM-DD               As-of date in UTC (default: today)
  --github-repo owner/repo        GitHub repository for workflow backfill
  --github-token-env ENV_NAME     Token env var for GitHub API (default: GITHUB_TOKEN)
  --nightly-status-file FILE      Optional mock status JSON for fixture tests
  -h, --help                      Show this help

Environment:
  PERF_REPORT_DIR       Output/report directory (default: artifacts/perf)
  PERF_SLO_DAYS         Required consecutive days (default: 7)
USAGE
}

shift_date() {
  local base="$1"
  local offset="$2"

  if date -u -d "${base} ${offset} day" +%Y-%m-%d >/dev/null 2>&1; then
    date -u -d "${base} ${offset} day" +%Y-%m-%d
    return
  fi

  if [[ "$offset" =~ ^- ]]; then
    date -u -j -v"${offset}"d -f "%Y-%m-%d" "$base" +%Y-%m-%d
  else
    date -u -j -v+"${offset}"d -f "%Y-%m-%d" "$base" +%Y-%m-%d
  fi
}

infer_github_repo() {
  local remote
  local candidate

  remote="$(git config --get remote.origin.url || true)"
  if [ -z "$remote" ]; then
    return 1
  fi

  candidate="${remote%.git}"
  candidate="${candidate#git@github.com:}"
  candidate="${candidate#https://github.com/}"
  if [[ "$candidate" == */* && "$candidate" != *:* ]]; then
    echo "$candidate"
    return 0
  fi
  return 1
}

api_get_json() {
  local url="$1"
  local out="$2"
  if [ -n "$GITHUB_TOKEN_VALUE" ]; then
    curl -fsSL \
      -H "Accept: application/vnd.github+json" \
      -H "Authorization: Bearer ${GITHUB_TOKEN_VALUE}" \
      "$url" \
      -o "$out"
  else
    curl -fsSL \
      -H "Accept: application/vnd.github+json" \
      "$url" \
      -o "$out"
  fi
}

while [ $# -gt 0 ]; do
  case "$1" in
    --date)
      shift
      AS_OF_DATE="${1:-}"
      ;;
    --github-repo)
      shift
      GITHUB_REPO="${1:-}"
      ;;
    --github-token-env)
      shift
      GITHUB_TOKEN_ENV="${1:-}"
      ;;
    --nightly-status-file)
      shift
      NIGHTLY_STATUS_FILE="${1:-}"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "[perf-slo-window] error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if ! command -v jq >/dev/null 2>&1; then
  echo "[perf-slo-window] error: jq not found in PATH" >&2
  exit 2
fi

if ! [[ "$REQUIRED_DAYS" =~ ^[0-9]+$ ]] || [ "$REQUIRED_DAYS" -le 0 ]; then
  echo "[perf-slo-window] invalid PERF_SLO_DAYS: $REQUIRED_DAYS" >&2
  exit 2
fi

if ! [[ "$AS_OF_DATE" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
  echo "[perf-slo-window] invalid --date format: $AS_OF_DATE (expected YYYY-MM-DD)" >&2
  exit 2
fi

mkdir -p "$REPORT_DIR"

if [ -z "$GITHUB_REPO" ]; then
  GITHUB_REPO="$(infer_github_repo || true)"
fi
GITHUB_TOKEN_VALUE="${!GITHUB_TOKEN_ENV:-}"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

RUNS_FILE="${tmp_dir}/perf-slo-nightly.runs.json"
HAS_GITHUB_DATA=0
if [ -z "$NIGHTLY_STATUS_FILE" ] && [ -n "$GITHUB_REPO" ]; then
  url="https://api.github.com/repos/${GITHUB_REPO}/actions/workflows/perf-slo-nightly.yml/runs?branch=main&status=completed&per_page=100"
  if api_get_json "$url" "$RUNS_FILE"; then
    HAS_GITHUB_DATA=1
  else
    echo "[perf-slo-window] warning: failed to fetch perf-slo-nightly runs" >&2
  fi
fi

daily_tmp="${tmp_dir}/daily.ndjson"
: >"$daily_tmp"

for ((offset=REQUIRED_DAYS-1; offset>=0; offset--)); do
  day="$(shift_date "$AS_OF_DATE" "-${offset}")"
  report_file="${REPORT_DIR}/perf-slo-gate-${day}.json"

  day_pass=0
  reason="missing_status"
  source="none"
  run_id=0

  if [ -f "$report_file" ]; then
    source="report"
    if jq -e '.pass == true' "$report_file" >/dev/null; then
      day_pass=1
      reason="pass"
    else
      reason="gate_failed"
    fi
  elif [ -n "$NIGHTLY_STATUS_FILE" ] && [ -f "$NIGHTLY_STATUS_FILE" ]; then
    source="mock_status"
    if jq -e --arg day "$day" 'has($day)' "$NIGHTLY_STATUS_FILE" >/dev/null; then
      if jq -e --arg day "$day" '.[$day] == true' "$NIGHTLY_STATUS_FILE" >/dev/null; then
        day_pass=1
        reason="pass"
      else
        reason="workflow_failed"
      fi
    else
      reason="missing_status"
    fi
  elif [ "$HAS_GITHUB_DATA" -eq 1 ]; then
    source="github_workflow"
    run="$(jq -c --arg day "$day" '
      [ .workflow_runs[]? | select(.created_at | startswith($day)) ]
      | sort_by(.created_at)
      | last // empty
    ' "$RUNS_FILE")"
    if [ -n "$run" ] && [ "$run" != "null" ]; then
      run_id="$(printf '%s\n' "$run" | jq -r '.id // 0')"
      conclusion="$(printf '%s\n' "$run" | jq -r '.conclusion // ""')"
      if [ "$conclusion" = "success" ]; then
        day_pass=1
        reason="pass"
      else
        reason="workflow_${conclusion:-failed}"
      fi
    else
      reason="missing_workflow_run"
    fi
  fi

  jq -n \
    --arg day "$day" \
    --arg source "$source" \
    --arg reason "$reason" \
    --argjson run_id "$run_id" \
    --argjson pass "$([ "$day_pass" -eq 1 ] && echo true || echo false)" \
    '{date: $day, pass: $pass, reason: $reason, source: $source, run_id: $run_id}' \
    >>"$daily_tmp"
done

daily_json="${tmp_dir}/daily.json"
jq -s '.' "$daily_tmp" >"$daily_json"

consecutive_days=0
while IFS= read -r pass; do
  if [ "$pass" = "true" ]; then
    consecutive_days=$((consecutive_days + 1))
  else
    break
  fi
done < <(jq -r 'reverse | .[] | .pass' "$daily_json")

window_passed=false
if [ "$consecutive_days" -ge "$REQUIRED_DAYS" ]; then
  window_passed=true
fi

JSON_OUT="${REPORT_DIR}/perf-slo-window.json"
MD_OUT="${REPORT_DIR}/perf-slo-window.md"
GENERATED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

jq -n \
  --arg generated_at "$GENERATED_AT" \
  --arg as_of_date "$AS_OF_DATE" \
  --argjson required_days "$REQUIRED_DAYS" \
  --argjson consecutive_days "$consecutive_days" \
  --argjson window_passed "$window_passed" \
  --slurpfile daily "$daily_json" \
  '{
    generated_at: $generated_at,
    as_of_date: $as_of_date,
    required_days: $required_days,
    consecutive_days: $consecutive_days,
    window_passed: $window_passed,
    daily: $daily[0]
  }' >"$JSON_OUT"

{
  echo "# Perf SLO Window"
  echo
  echo "- Generated at: ${GENERATED_AT}"
  echo "- As of date: ${AS_OF_DATE}"
  echo "- Required days: ${REQUIRED_DAYS}"
  echo "- Consecutive passing days: ${consecutive_days}"
  echo "- Window passed: ${window_passed}"
  echo
  echo "| Date | Status | Source | Reason | Run ID |"
  echo "|---|---|---|---|---:|"
  jq -r '.daily[] | "| \(.date) | \((if .pass then "PASS" else "FAIL" end)) | \(.source) | \(.reason) | \(.run_id) |"' "$JSON_OUT"
} >"$MD_OUT"

echo "[perf-slo-window] wrote: $JSON_OUT"
echo "[perf-slo-window] wrote: $MD_OUT"

if [ "$window_passed" = "true" ]; then
  exit 0
fi
exit 1

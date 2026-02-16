#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

REPORT_DIR="${TCK_REPORT_DIR:-artifacts/tck}"
DAYS="${STABILITY_DAYS:-7}"
THRESHOLD="${TCK_MIN_PASS_RATE:-95}"
MODE="strict"
AS_OF_DATE="$(date -u +%Y-%m-%d)"
GITHUB_REPO="${STABILITY_GITHUB_REPO:-}"
GITHUB_TOKEN_ENV="GITHUB_TOKEN"
NIGHTLY_STATUS_FILE=""

usage() {
  cat <<'USAGE'
Usage:
  scripts/stability_window.sh [options]

Options:
  --mode strict|tier3-only        Stability mode (default: strict)
  --date YYYY-MM-DD               As-of date in UTC (default: today)
  --github-repo owner/repo        GitHub repo used for workflow status backfill
  --github-token-env ENV_NAME     Token env var for GitHub API (default: GITHUB_TOKEN)
  --nightly-status-file FILE      Optional mock nightly status JSON for fixture tests
  -h, --help                      Show this help

Environment:
  TCK_REPORT_DIR      Output directory (default: artifacts/tck)
  STABILITY_DAYS      Required consecutive days (default: 7)
  TCK_MIN_PASS_RATE   Tier-3 minimum pass rate (default: 95)
USAGE
}

bool_to_json() {
  if [ "${1}" -eq 1 ]; then
    echo true
  else
    echo false
  fi
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

date_to_epoch_utc() {
  local day="$1"
  local time_part="$2"

  if date -u -d "${day} ${time_part}" +%s >/dev/null 2>&1; then
    date -u -d "${day} ${time_part}" +%s
    return
  fi

  date -u -j -f "%Y-%m-%d %H:%M:%S" "${day} ${time_part}" +%s
}

iso_to_epoch_utc() {
  local iso="$1"

  if date -u -d "$iso" +%s >/dev/null 2>&1; then
    date -u -d "$iso" +%s
    return
  fi

  date -u -j -f "%Y-%m-%dT%H:%M:%SZ" "$iso" +%s
}

infer_github_repo() {
  local remote
  local candidate

  remote="$(git config --get remote.origin.url || true)"
  if [ -z "$remote" ]; then
    return 1
  fi

  candidate="$remote"
  candidate="${candidate%.git}"
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
    --mode)
      shift
      MODE="${1:-}"
      if [ -z "$MODE" ]; then
        echo "[stability-window] error: --mode requires a value" >&2
        exit 2
      fi
      ;;
    --date)
      shift
      AS_OF_DATE="${1:-}"
      if [ -z "$AS_OF_DATE" ]; then
        echo "[stability-window] error: --date requires a value" >&2
        exit 2
      fi
      ;;
    --github-repo)
      shift
      GITHUB_REPO="${1:-}"
      if [ -z "$GITHUB_REPO" ]; then
        echo "[stability-window] error: --github-repo requires a value" >&2
        exit 2
      fi
      ;;
    --github-token-env)
      shift
      GITHUB_TOKEN_ENV="${1:-}"
      if [ -z "$GITHUB_TOKEN_ENV" ]; then
        echo "[stability-window] error: --github-token-env requires a value" >&2
        exit 2
      fi
      ;;
    --nightly-status-file)
      shift
      NIGHTLY_STATUS_FILE="${1:-}"
      if [ -z "$NIGHTLY_STATUS_FILE" ]; then
        echo "[stability-window] error: --nightly-status-file requires a value" >&2
        exit 2
      fi
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "[stability-window] error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if ! [[ "$DAYS" =~ ^[0-9]+$ ]] || [ "$DAYS" -le 0 ]; then
  echo "[stability-window] invalid STABILITY_DAYS: $DAYS" >&2
  exit 2
fi

if ! [[ "$AS_OF_DATE" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
  echo "[stability-window] invalid --date format: $AS_OF_DATE (expected YYYY-MM-DD)" >&2
  exit 2
fi

if [ "$MODE" != "strict" ] && [ "$MODE" != "tier3-only" ]; then
  echo "[stability-window] unsupported --mode: $MODE" >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "[stability-window] error: jq not found in PATH" >&2
  exit 2
fi

mkdir -p "$REPORT_DIR"

WF_FILES=(
  "tck-nightly.yml"
  "benchmark-nightly.yml"
  "chaos-nightly.yml"
  "soak-nightly.yml"
  "fuzz-nightly.yml"
)
WF_KEYS=(
  "tck_nightly"
  "benchmark_nightly"
  "chaos_nightly"
  "soak_nightly"
  "fuzz_nightly"
)
WF_FRESHNESS=(
  86400
  604800
  604800
  604800
  604800
)
CI_WORKFLOW_FILE="ci-daily-snapshot.yml"

if [ -z "$GITHUB_REPO" ]; then
  GITHUB_REPO="$(infer_github_repo || true)"
fi

GITHUB_TOKEN_VALUE="${!GITHUB_TOKEN_ENV:-}"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

HAS_GITHUB_DATA=0
if [ "$MODE" = "strict" ] && [ -n "$GITHUB_REPO" ] && [ -z "$NIGHTLY_STATUS_FILE" ]; then
  HAS_GITHUB_DATA=1
  workflows_to_fetch=(
    "${CI_WORKFLOW_FILE}"
    "tck-nightly.yml"
    "benchmark-nightly.yml"
    "chaos-nightly.yml"
    "soak-nightly.yml"
    "fuzz-nightly.yml"
  )

  for wf in "${workflows_to_fetch[@]}"; do
    out="${tmp_dir}/${wf}.runs.json"
    url="https://api.github.com/repos/${GITHUB_REPO}/actions/workflows/${wf}/runs?branch=main&status=completed&per_page=100"
    if ! api_get_json "$url" "$out"; then
      HAS_GITHUB_DATA=0
      echo "[stability-window] warning: failed to fetch workflow runs for ${wf}" >&2
      break
    fi
  done
fi

backfill_ci_daily_file() {
  local day="$1"
  local ci_file="$2"
  local runs_file
  local latest_run
  local conclusion
  local all_passed
  local run_id
  local generated_at

  if [ -f "$ci_file" ]; then
    return 0
  fi
  if [ "$HAS_GITHUB_DATA" -ne 1 ]; then
    return 1
  fi

  runs_file="${tmp_dir}/${CI_WORKFLOW_FILE}.runs.json"
  if [ ! -f "$runs_file" ]; then
    return 1
  fi

  latest_run="$(jq -c --arg day "$day" '
    [ .workflow_runs[]? | select(.created_at | startswith($day)) ]
    | sort_by(.created_at)
    | last // empty
  ' "$runs_file")"

  if [ -z "$latest_run" ] || [ "$latest_run" = "null" ]; then
    return 1
  fi

  conclusion="$(printf '%s\n' "$latest_run" | jq -r '.conclusion // ""')"
  run_id="$(printf '%s\n' "$latest_run" | jq -r '.id // 0')"
  all_passed=0
  if [ "$conclusion" = "success" ]; then
    all_passed=1
  fi
  generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

  jq -n \
    --arg date "$day" \
    --arg workflow "$CI_WORKFLOW_FILE" \
    --arg source "github_workflow_run" \
    --arg conclusion "$conclusion" \
    --arg generated_at "$generated_at" \
    --argjson run_id "$run_id" \
    --argjson all_passed "$(bool_to_json "$all_passed")" \
    '{
      date: $date,
      workflow: $workflow,
      source: $source,
      generated_at: $generated_at,
      run_id: $run_id,
      conclusion: $conclusion,
      all_passed: $all_passed
    }' >"$ci_file"

  return 0
}

fetch_tier3_rate_from_artifact() {
  local day="$1"
  local target_file="$2"
  local runs_file
  local run_id
  local artifacts_file
  local artifact_id
  local zip_file
  local extract_dir
  local source_file
  local url

  if [ -f "$target_file" ]; then
    return 0
  fi
  if [ "$HAS_GITHUB_DATA" -ne 1 ]; then
    return 1
  fi

  runs_file="${tmp_dir}/tck-nightly.yml.runs.json"
  if [ ! -f "$runs_file" ]; then
    return 1
  fi

  run_id="$(jq -r --arg day "$day" '
    [ .workflow_runs[]? | select(.created_at | startswith($day)) ]
    | sort_by(.created_at)
    | last.id // empty
  ' "$runs_file")"

  if [ -z "$run_id" ]; then
    return 1
  fi

  artifacts_file="${tmp_dir}/run-${run_id}-artifacts.json"
  url="https://api.github.com/repos/${GITHUB_REPO}/actions/runs/${run_id}/artifacts?per_page=100"
  if ! api_get_json "$url" "$artifacts_file"; then
    return 1
  fi

  artifact_id="$(jq -r '
    [ .artifacts[]? |
      select((.name == "tck-nightly-artifacts" or .name == "beta-gate-artifacts") and (.expired == false))
    ]
    | sort_by(.created_at)
    | last.id // empty
  ' "$artifacts_file")"

  if [ -z "$artifact_id" ]; then
    return 1
  fi

  zip_file="${tmp_dir}/artifact-${artifact_id}.zip"
  url="https://api.github.com/repos/${GITHUB_REPO}/actions/artifacts/${artifact_id}/zip"
  if [ -n "$GITHUB_TOKEN_VALUE" ]; then
    curl -fsSL \
      -H "Accept: application/vnd.github+json" \
      -H "Authorization: Bearer ${GITHUB_TOKEN_VALUE}" \
      "$url" \
      -o "$zip_file"
  else
    curl -fsSL \
      -H "Accept: application/vnd.github+json" \
      "$url" \
      -o "$zip_file"
  fi

  extract_dir="${tmp_dir}/artifact-${artifact_id}"
  mkdir -p "$extract_dir"
  unzip -oq "$zip_file" -d "$extract_dir"

  source_file="$(find "$extract_dir" -type f \( -name 'tier3-rate.json' -o -name 'tier3-rate-*.json' \) | head -n1 || true)"
  if [ -z "$source_file" ]; then
    return 1
  fi

  cp "$source_file" "$target_file"
  return 0
}

dates=()
for ((offset = DAYS - 1; offset >= 0; offset--)); do
  dates+=("$(shift_date "$AS_OF_DATE" "-${offset}")")
done

for day in "${dates[@]}"; do
  tier3_file="${REPORT_DIR}/tier3-rate-${day}.json"
  ci_file="${REPORT_DIR}/ci-daily-${day}.json"

  if [ "$MODE" = "strict" ] && [ ! -f "$tier3_file" ]; then
    fetch_tier3_rate_from_artifact "$day" "$tier3_file" || true
  fi

  if [ "$MODE" = "strict" ] && [ ! -f "$ci_file" ]; then
    backfill_ci_daily_file "$day" "$ci_file" || true
  fi
done

daily_entries=()
consecutive_days=0
window_all_pass=1

for day in "${dates[@]}"; do
  day_end_epoch="$(date_to_epoch_utc "$day" "23:59:59")"

  tier3_file="${REPORT_DIR}/tier3-rate-${day}.json"
  tier3_pass=0
  tier3_reason=""
  tier3_rate=""
  tier3_failed=""

  if [ -f "$tier3_file" ]; then
    tier3_rate="$(jq -r '.pass_rate // empty' "$tier3_file" 2>/dev/null || true)"
    tier3_failed="$(jq -r '.scenarios.failed // .failed // empty' "$tier3_file" 2>/dev/null || true)"

    if [[ "$tier3_rate" =~ ^[0-9]+([.][0-9]+)?$ ]] && [[ "$tier3_failed" =~ ^[0-9]+$ ]]; then
      rate_ok="$(awk -v r="$tier3_rate" -v t="$THRESHOLD" 'BEGIN { if (r + 0 >= t + 0) print 1; else print 0 }')"
      if [ "$rate_ok" = "1" ] && [ "$tier3_failed" = "0" ]; then
        tier3_pass=1
      else
        tier3_reason="threshold_or_failed"
      fi
    else
      tier3_reason="parse_error"
    fi
  else
    tier3_reason="missing_tier3_rate"
  fi

  ci_file="${REPORT_DIR}/ci-daily-${day}.json"
  ci_pass=0
  ci_reason=""
  ci_all_passed=""

  if [ "$MODE" = "strict" ]; then
    if [ -f "$ci_file" ]; then
      ci_all_passed="$(jq -r '.all_passed // empty' "$ci_file" 2>/dev/null || true)"
      if [ "$ci_all_passed" = "true" ]; then
        ci_pass=1
      else
        ci_reason="ci_daily_failed"
      fi
    else
      ci_reason="missing_ci_daily"
    fi
  else
    ci_pass=1
  fi

  nightly_pass=1
  nightly_entries=()

  if [ "$MODE" = "strict" ]; then
    for idx in "${!WF_FILES[@]}"; do
      wf="${WF_FILES[$idx]}"
      wf_key="${WF_KEYS[$idx]}"
      wf_freshness="${WF_FRESHNESS[$idx]}"

      wf_pass=0
      wf_reason=""
      used_mock=0

      if [ -n "$NIGHTLY_STATUS_FILE" ] && [ -f "$NIGHTLY_STATUS_FILE" ]; then
        mock_val="$(jq -r --arg day "$day" --arg wf "$wf" '
          if (has($day) and (.[$day] | type == "object") and (.[$day] | has($wf))) then
            .[$day][$wf]
          else
            "MISSING"
          end
        ' "$NIGHTLY_STATUS_FILE" 2>/dev/null || echo "MISSING")"

        if [ "$mock_val" = "true" ]; then
          wf_pass=1
          wf_reason="mock_success"
          used_mock=1
        elif [ "$mock_val" = "false" ]; then
          wf_pass=0
          wf_reason="mock_failed"
          used_mock=1
        fi
      fi

      if [ "$used_mock" -ne 1 ]; then
        if [ "$HAS_GITHUB_DATA" -ne 1 ] || [ -z "$GITHUB_REPO" ]; then
          wf_reason="github_data_unavailable"
        else
          wf_runs_file="${tmp_dir}/${wf}.runs.json"
          if [ ! -f "$wf_runs_file" ]; then
            wf_reason="missing_workflow_runs"
          else
            latest_success_run="$(jq -c --argjson cutoff "$day_end_epoch" '
              [ .workflow_runs[]?
                | select((.created_at | fromdateiso8601) <= $cutoff)
                | select(.conclusion == "success")
              ]
              | sort_by(.created_at)
              | last // empty
            ' "$wf_runs_file")"

            if [ -z "$latest_success_run" ] || [ "$latest_success_run" = "null" ]; then
              latest_run="$(jq -c --argjson cutoff "$day_end_epoch" '
                [ .workflow_runs[]? | select((.created_at | fromdateiso8601) <= $cutoff) ]
                | sort_by(.created_at)
                | last // empty
              ' "$wf_runs_file")"

              if [ -z "$latest_run" ] || [ "$latest_run" = "null" ]; then
                wf_reason="no_run_before_day_end"
              else
                latest_conclusion="$(printf '%s\n' "$latest_run" | jq -r '.conclusion // "unknown"')"
                wf_reason="no_success_latest_${latest_conclusion}"
              fi
            else
              latest_created_at="$(printf '%s\n' "$latest_success_run" | jq -r '.created_at // ""')"
              if [ -z "$latest_created_at" ]; then
                wf_reason="missing_created_at"
              else
                latest_epoch="$(iso_to_epoch_utc "$latest_created_at")"
                age_sec=$((day_end_epoch - latest_epoch))

                if [ "$age_sec" -le "$wf_freshness" ]; then
                  wf_pass=1
                  wf_reason="success"
                else
                  wf_reason="stale_${age_sec}s"
                fi
              fi
            fi
          fi
        fi
      fi

      if [ "$wf_pass" -ne 1 ]; then
        nightly_pass=0
      fi

      nightly_entries+=("$(jq -n \
        --arg workflow "$wf" \
        --arg key "$wf_key" \
        --arg reason "$wf_reason" \
        --argjson freshness_seconds "$wf_freshness" \
        --argjson pass "$(bool_to_json "$wf_pass")" \
        '{
          key: $key,
          workflow: $workflow,
          pass: $pass,
          freshness_seconds: $freshness_seconds,
          reason: $reason
        }')")
    done
  fi

  nightly_entries_json="[]"
  if [ "${#nightly_entries[@]}" -gt 0 ]; then
    nightly_entries_json="$(printf '%s\n' "${nightly_entries[@]}" | jq -s '.')"
  fi

  day_pass=1
  if [ "$tier3_pass" -ne 1 ] || [ "$ci_pass" -ne 1 ] || [ "$nightly_pass" -ne 1 ]; then
    day_pass=0
  fi

  if [ "$day_pass" -eq 1 ]; then
    consecutive_days=$((consecutive_days + 1))
  else
    consecutive_days=0
    window_all_pass=0
  fi

  tier3_rate_json="null"
  if [[ "$tier3_rate" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    tier3_rate_json="$tier3_rate"
  fi

  tier3_failed_json="null"
  if [[ "$tier3_failed" =~ ^[0-9]+$ ]]; then
    tier3_failed_json="$tier3_failed"
  fi

  daily_entries+=("$(jq -n \
    --arg date "$day" \
    --arg tier3_file "$tier3_file" \
    --arg tier3_reason "$tier3_reason" \
    --arg ci_file "$ci_file" \
    --arg ci_reason "$ci_reason" \
    --argjson tier3_pass "$(bool_to_json "$tier3_pass")" \
    --argjson tier3_rate "$tier3_rate_json" \
    --argjson tier3_failed "$tier3_failed_json" \
    --argjson ci_pass "$(bool_to_json "$ci_pass")" \
    --argjson nightly_pass "$(bool_to_json "$nightly_pass")" \
    --argjson workflows "$nightly_entries_json" \
    --argjson pass "$(bool_to_json "$day_pass")" \
    '{
      date: $date,
      tier3: {
        file: $tier3_file,
        pass: $tier3_pass,
        pass_rate: $tier3_rate,
        failed: $tier3_failed,
        reason: $tier3_reason
      },
      ci_daily: {
        file: $ci_file,
        pass: $ci_pass,
        reason: $ci_reason
      },
      nightly: {
        pass: $nightly_pass,
        workflows: $workflows
      },
      pass: $pass
    }')")
done

daily_json="$(printf '%s\n' "${daily_entries[@]}" | jq -s '.')"
all_checks_pass="$window_all_pass"
window_passed=0
if [ "$consecutive_days" -ge "$DAYS" ] && [ "$all_checks_pass" -eq 1 ]; then
  window_passed=1
fi

generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
window_json_file="${REPORT_DIR}/stability-window.json"
window_md_file="${REPORT_DIR}/stability-window.md"
daily_json_file="${REPORT_DIR}/stability-daily-${AS_OF_DATE}.json"

jq -n \
  --arg generated_at "$generated_at" \
  --arg as_of_date "$AS_OF_DATE" \
  --arg mode "$MODE" \
  --arg github_repo "$GITHUB_REPO" \
  --argjson days "$DAYS" \
  --argjson threshold "$THRESHOLD" \
  --argjson consecutive_days "$consecutive_days" \
  --argjson all_checks_pass "$(bool_to_json "$all_checks_pass")" \
  --argjson window_passed "$(bool_to_json "$window_passed")" \
  --argjson daily "$daily_json" \
  '{
    generated_at: $generated_at,
    as_of_date: $as_of_date,
    mode: $mode,
    github_repo: $github_repo,
    required_days: $days,
    threshold: $threshold,
    consecutive_days: $consecutive_days,
    all_checks_pass: $all_checks_pass,
    window_passed: $window_passed,
    daily: $daily
  }' >"$window_json_file"

printf '%s\n' "$daily_json" | jq '.[-1]' >"$daily_json_file"

{
  echo "# Stability Window"
  echo
  echo "- Generated at: ${generated_at}"
  echo "- As of date: ${AS_OF_DATE}"
  echo "- Mode: ${MODE}"
  echo "- Required days: ${DAYS}"
  echo "- Consecutive passing days: ${consecutive_days}"
  echo "- All checks pass (window): $(bool_to_json "$all_checks_pass")"
  echo "- Window passed: $(bool_to_json "$window_passed")"
  echo
  echo "| Date | Tier3 | CI Daily | Nightly | Result |"
  echo "|---|---|---|---|---|"
  printf '%s\n' "$daily_json" | jq -r '.[] |
    "| \(.date) | " +
    (if .tier3.pass then "PASS" else "BLOCKED" end) +
    " | " +
    (if .ci_daily.pass then "PASS" else "BLOCKED" end) +
    " | " +
    (if .nightly.pass then "PASS" else "BLOCKED" end) +
    " | " +
    (if .pass then "PASS" else "BLOCKED" end) +
    " |"'
} >"$window_md_file"

echo "[stability-window] report_dir=${REPORT_DIR}"
echo "[stability-window] as_of_date=${AS_OF_DATE}"
echo "[stability-window] mode=${MODE}"
echo "[stability-window] consecutive_days=${consecutive_days}/${DAYS}"
echo "[stability-window] all_checks_pass=$(bool_to_json "$all_checks_pass")"
echo "[stability-window] wrote ${daily_json_file}"
echo "[stability-window] wrote ${window_json_file}"
echo "[stability-window] wrote ${window_md_file}"

if [ "$window_passed" -eq 1 ]; then
  echo "[stability-window] PASSED"
  exit 0
fi

echo "[stability-window] BLOCKED: strict stability window not yet satisfied"
exit 1

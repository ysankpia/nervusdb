#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${ROOT_DIR}/scripts/stability_window.sh"

if ! command -v jq >/dev/null 2>&1; then
  echo "[fixture] jq not found" >&2
  exit 2
fi

DAYS=(
  "2026-02-15"
  "2026-02-16"
  "2026-02-17"
  "2026-02-18"
  "2026-02-19"
  "2026-02-20"
  "2026-02-21"
)

assert_eq() {
  local expected="$1"
  local actual="$2"
  local message="$3"
  if [ "$expected" != "$actual" ]; then
    echo "[fixture] assert failed: ${message} (expected=${expected}, actual=${actual})" >&2
    exit 1
  fi
}

run_with_expected_rc() {
  local expected_rc="$1"
  shift

  set +e
  "$@"
  local rc=$?
  set -e

  if [ "$rc" -ne "$expected_rc" ]; then
    echo "[fixture] command exit mismatch: expected=${expected_rc}, actual=${rc}" >&2
    echo "[fixture] command: $*" >&2
    exit 1
  fi
}

write_tier3_file() {
  local report_dir="$1"
  local day="$2"
  local pass_rate="$3"
  local failed="$4"

  cat >"${report_dir}/tier3-rate-${day}.json" <<JSON
{
  "date": "${day}",
  "pass_rate": ${pass_rate},
  "scenarios": {
    "failed": ${failed}
  }
}
JSON
}

write_ci_file() {
  local report_dir="$1"
  local day="$2"
  local all_passed="$3"

  cat >"${report_dir}/ci-daily-${day}.json" <<JSON
{
  "date": "${day}",
  "all_passed": ${all_passed}
}
JSON
}

write_mock_nightly_status() {
  local out_file="$1"
  local days_json

  days_json="$(printf '%s\n' "${DAYS[@]}" | jq -R . | jq -s .)"
  jq -n --argjson days "$days_json" '
    reduce $days[] as $day (
      {};
      .[$day] = {
        "tck-nightly.yml": true,
        "benchmark-nightly.yml": true,
        "chaos-nightly.yml": true,
        "soak-nightly.yml": true,
        "fuzz-nightly.yml": true
      }
    )
  ' >"$out_file"
}

scenario_all_pass_7_days() {
  local tmp_dir
  local report_dir
  local nightly_file

  tmp_dir="$(mktemp -d)"
  report_dir="${tmp_dir}/report"
  nightly_file="${tmp_dir}/nightly.json"
  mkdir -p "$report_dir"

  for day in "${DAYS[@]}"; do
    write_tier3_file "$report_dir" "$day" "100" "0"
    write_ci_file "$report_dir" "$day" "true"
  done
  write_mock_nightly_status "$nightly_file"

  run_with_expected_rc 0 env \
    STABILITY_DAYS=7 \
    TCK_REPORT_DIR="$report_dir" \
    bash "$SCRIPT" --mode strict --date 2026-02-21 --nightly-status-file "$nightly_file"

  assert_eq "true" "$(jq -r '.window_passed' "${report_dir}/stability-window.json")" "all pass should satisfy window"
  assert_eq "7" "$(jq -r '.consecutive_days' "${report_dir}/stability-window.json")" "all pass consecutive days"
  rm -rf "$tmp_dir"
}

scenario_tier3_failure_resets_chain() {
  local tmp_dir
  local report_dir
  local nightly_file

  tmp_dir="$(mktemp -d)"
  report_dir="${tmp_dir}/report"
  nightly_file="${tmp_dir}/nightly.json"
  mkdir -p "$report_dir"

  for day in "${DAYS[@]}"; do
    if [ "$day" = "2026-02-18" ]; then
      write_tier3_file "$report_dir" "$day" "90" "1"
    else
      write_tier3_file "$report_dir" "$day" "100" "0"
    fi
    write_ci_file "$report_dir" "$day" "true"
  done
  write_mock_nightly_status "$nightly_file"

  run_with_expected_rc 1 env \
    STABILITY_DAYS=7 \
    TCK_REPORT_DIR="$report_dir" \
    bash "$SCRIPT" --mode strict --date 2026-02-21 --nightly-status-file "$nightly_file"

  assert_eq "threshold_or_failed" "$(jq -r '.daily[] | select(.date=="2026-02-18") | .tier3.reason' "${report_dir}/stability-window.json")" "tier3 failure reason"
  assert_eq "3" "$(jq -r '.consecutive_days' "${report_dir}/stability-window.json")" "consecutive reset after failing day"
  rm -rf "$tmp_dir"
}

scenario_missing_ci_daily_blocks_day() {
  local tmp_dir
  local report_dir
  local nightly_file

  tmp_dir="$(mktemp -d)"
  report_dir="${tmp_dir}/report"
  nightly_file="${tmp_dir}/nightly.json"
  mkdir -p "$report_dir"

  for day in "${DAYS[@]}"; do
    write_tier3_file "$report_dir" "$day" "100" "0"
    if [ "$day" != "2026-02-20" ]; then
      write_ci_file "$report_dir" "$day" "true"
    fi
  done
  write_mock_nightly_status "$nightly_file"

  run_with_expected_rc 1 env \
    STABILITY_DAYS=7 \
    TCK_REPORT_DIR="$report_dir" \
    bash "$SCRIPT" --mode strict --date 2026-02-21 --nightly-status-file "$nightly_file"

  assert_eq "missing_ci_daily" "$(jq -r '.daily[] | select(.date=="2026-02-20") | .ci_daily.reason' "${report_dir}/stability-window.json")" "missing ci daily reason"
  rm -rf "$tmp_dir"
}

write_fake_curl() {
  local out_path="$1"

  cat >"$out_path" <<'FAKECURL'
#!/usr/bin/env bash
set -euo pipefail

url=""
out_file=""
write_fmt=""
has_auth=0

args=("$@")
idx=0
while [ "$idx" -lt "${#args[@]}" ]; do
  arg="${args[$idx]}"
  case "$arg" in
    -H)
      idx=$((idx + 1))
      header="${args[$idx]:-}"
      if [[ "$header" == Authorization:* ]]; then
        has_auth=1
      fi
      ;;
    -o)
      idx=$((idx + 1))
      out_file="${args[$idx]:-}"
      ;;
    -w)
      idx=$((idx + 1))
      write_fmt="${args[$idx]:-}"
      ;;
    http*://*)
      url="$arg"
      ;;
  esac
  idx=$((idx + 1))
done

status_code="200"
if [[ "$url" == *"/actions/runs/"*"/artifacts"* ]] || [[ "$url" == *"/actions/artifacts/"*"/zip"* ]]; then
  if [ "$has_auth" -eq 1 ]; then
    status_code="403"
  else
    status_code="404"
  fi
fi

if [ -n "$write_fmt" ]; then
  printf '%s' "$status_code"
  exit 0
fi

if [[ "$url" == *"/actions/workflows/"*"/runs"* ]]; then
  payload='{"workflow_runs":[{"id":111,"created_at":"2026-02-16T12:00:00Z","conclusion":"success"}]}'
  if [ -n "$out_file" ]; then
    printf '%s\n' "$payload" >"$out_file"
  else
    printf '%s\n' "$payload"
  fi
  exit 0
fi

if [[ "$url" == *"/actions/runs/"*"/artifacts"* ]] || [[ "$url" == *"/actions/artifacts/"*"/zip"* ]]; then
  exit 22
fi

if [ -n "$out_file" ]; then
  printf '%s\n' '{}' >"$out_file"
else
  printf '%s\n' '{}'
fi
FAKECURL

  chmod +x "$out_path"
}

scenario_reason_paths_token_vs_no_token() {
  local tmp_dir
  local fake_bin
  local no_token_report
  local token_report

  tmp_dir="$(mktemp -d)"
  fake_bin="${tmp_dir}/fake-bin"
  mkdir -p "$fake_bin"
  write_fake_curl "${fake_bin}/curl"

  no_token_report="${tmp_dir}/report-no-token"
  mkdir -p "$no_token_report"
  write_ci_file "$no_token_report" "2026-02-16" "true"

  run_with_expected_rc 1 env -u GITHUB_TOKEN \
    PATH="${fake_bin}:$PATH" \
    STABILITY_DAYS=1 \
    TCK_REPORT_DIR="$no_token_report" \
    bash "$SCRIPT" --mode strict --date 2026-02-16 --github-repo LuQing-Studio/nervusdb

  assert_eq "artifact_not_found" "$(jq -r '.daily[-1].tier3.reason' "${no_token_report}/stability-window.json")" "no-token reason"

  token_report="${tmp_dir}/report-token"
  mkdir -p "$token_report"
  write_ci_file "$token_report" "2026-02-16" "true"

  run_with_expected_rc 1 env \
    PATH="${fake_bin}:$PATH" \
    GITHUB_TOKEN="fixture-token" \
    STABILITY_DAYS=1 \
    TCK_REPORT_DIR="$token_report" \
    bash "$SCRIPT" --mode strict --date 2026-02-16 --github-repo LuQing-Studio/nervusdb

  assert_eq "artifact_fetch_auth_failed" "$(jq -r '.daily[-1].tier3.reason' "${token_report}/stability-window.json")" "token reason"
  rm -rf "$tmp_dir"
}

scenario_empty_tier3_snapshot_reason() {
  local tmp_dir
  local report_dir
  local nightly_file

  tmp_dir="$(mktemp -d)"
  report_dir="${tmp_dir}/report"
  nightly_file="${tmp_dir}/nightly.json"
  mkdir -p "$report_dir"

  # 让最近一天是空快照，验证 reason 不再误标为 threshold_or_failed。
  for day in "${DAYS[@]}"; do
    write_ci_file "$report_dir" "$day" "true"
    if [ "$day" = "2026-02-21" ]; then
      cat >"${report_dir}/tier3-rate-${day}.json" <<JSON
{
  "date": "${day}",
  "mode": "partial",
  "pass_rate": 0.00,
  "scenarios": {
    "total": 0,
    "passed": 0,
    "skipped": 0,
    "failed": 0
  }
}
JSON
    else
      write_tier3_file "$report_dir" "$day" "100" "0"
    fi
  done
  write_mock_nightly_status "$nightly_file"

  run_with_expected_rc 1 env \
    STABILITY_DAYS=7 \
    TCK_REPORT_DIR="$report_dir" \
    bash "$SCRIPT" --mode strict --date 2026-02-21 --nightly-status-file "$nightly_file"

  assert_eq "empty_tier3_snapshot" "$(jq -r '.daily[] | select(.date=="2026-02-21") | .tier3.reason' "${report_dir}/stability-window.json")" "empty tier3 snapshot reason"
  rm -rf "$tmp_dir"
}

main() {
  scenario_all_pass_7_days
  scenario_tier3_failure_resets_chain
  scenario_missing_ci_daily_blocks_day
  scenario_reason_paths_token_vs_no_token
  scenario_empty_tier3_snapshot_reason
  echo "[fixture] stability_window fixtures passed"
}

main

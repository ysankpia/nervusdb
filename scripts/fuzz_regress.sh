#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR/fuzz"

ALLOW_SKIP="${FUZZ_ALLOW_SKIP:-0}"
FUZZ_RUNS="${FUZZ_RUNS:-1}"

handle_setup_failure() {
  local step="$1"
  if [[ "$ALLOW_SKIP" == "1" ]]; then
    echo "[fuzz-regress] WARN: ${step} failed; skip regressions (FUZZ_ALLOW_SKIP=1)"
    exit 0
  fi
  echo "[fuzz-regress] ERROR: ${step} failed"
  exit 1
}

if ! cargo fuzz --help >/dev/null 2>&1; then
  echo "[fuzz-regress] cargo-fuzz not found, installing..."
  cargo install cargo-fuzz --locked || handle_setup_failure "cargo-fuzz install"
fi

if ! RUSTUP_AUTO_INSTALL=0 cargo +nightly --version >/dev/null 2>&1; then
  if [[ "$ALLOW_SKIP" == "1" ]]; then
    echo "[fuzz-regress] WARN: nightly toolchain unavailable locally; skip regressions (FUZZ_ALLOW_SKIP=1)"
    exit 0
  fi
  echo "[fuzz-regress] nightly toolchain not ready, installing (profile=minimal)..."
  rustup toolchain install nightly --profile minimal || handle_setup_failure "nightly toolchain install"
fi

if ! RUSTUP_AUTO_INSTALL=0 cargo +nightly --version >/dev/null 2>&1; then
  handle_setup_failure "nightly toolchain verification"
fi

TARGETS=(query_prepare query_parse query_execute)

run_target_regressions() {
  local target="$1"
  local regress_dir="regressions/${target}"

  if [[ ! -d "$regress_dir" ]]; then
    echo "[fuzz-regress] no regression dir for $target, skipped"
    return 0
  fi

  local found=0
  while IFS= read -r -d '' sample; do
    found=1
    echo "[fuzz-regress] replay ${target} <- ${sample}"
    RUSTUP_AUTO_INSTALL=0 cargo +nightly fuzz run "$target" "$sample" -- -runs="$FUZZ_RUNS"
  done < <(find "$regress_dir" -type f -print0 | sort -z)

  if [[ "$found" -eq 0 ]]; then
    echo "[fuzz-regress] no regression samples for $target"
  fi
}

for target in "${TARGETS[@]}"; do
  run_target_regressions "$target"
done

echo "[fuzz-regress] all regression samples passed"

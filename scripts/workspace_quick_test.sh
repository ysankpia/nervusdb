#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

cargo test --workspace --exclude nervusdb-pyo3 --exclude nervusdb-v2 --all-targets
cargo test -p nervusdb-v2 --lib
cargo test -p nervusdb-v2 --bins

for test_file in nervusdb-v2/tests/*.rs; do
  test_name="$(basename "$test_file" .rs)"
  if [[ "$test_name" == "tck_harness" ]]; then
    continue
  fi
  cargo test -p nervusdb-v2 --test "$test_name"
done

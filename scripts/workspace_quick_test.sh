#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

cargo test --workspace --exclude nervusdb-pyo3 --exclude nervusdb --all-targets
cargo test -p nervusdb --lib
cargo test -p nervusdb --bins

for test_file in nervusdb/tests/*.rs; do
  test_name="$(basename "$test_file" .rs)"
  if [[ "$test_name" == "tck_harness" ]]; then
    continue
  fi
  cargo test -p nervusdb --test "$test_name"
done

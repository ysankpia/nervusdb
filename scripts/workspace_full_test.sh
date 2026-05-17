#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "[full] workspace clippy all targets except pyo3"
cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings

echo "[full] workspace tests except pyo3 and nervusdb integration fan-out"
cargo test --workspace --exclude nervusdb-pyo3 --exclude nervusdb --all-targets

echo "[full] nervusdb lib and bin tests"
cargo test -p nervusdb --lib
cargo test -p nervusdb --bins

echo "[full] nervusdb integration tests except TCK harness"
for test_file in nervusdb/tests/*.rs; do
  test_name="$(basename "$test_file" .rs)"
  if [[ "$test_name" == "tck_harness" ]]; then
    continue
  fi
  cargo test -p nervusdb --test "$test_name"
done

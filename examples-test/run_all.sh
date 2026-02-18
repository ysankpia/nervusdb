#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_ROOT"

echo "[examples-test] 1/3 Rust capability tests"
set +e
bash "$SCRIPT_DIR/nervusdb-rust-test/run_test.sh"
rc_rust=$?
set -e

echo "[examples-test] 2/3 Node capability tests"
cargo build --manifest-path "$REPO_ROOT/nervusdb-node/Cargo.toml" --release
npm --prefix "$SCRIPT_DIR/nervusdb-node-test" ci
set +e
npm --prefix "$SCRIPT_DIR/nervusdb-node-test" test
rc_node=$?
set -e

echo "[examples-test] 3/3 Python capability tests"
set +e
bash "$SCRIPT_DIR/nervusdb-python-test/run_test.sh"
rc_py=$?
set -e

echo
printf '[examples-test] summary: rust=%s node=%s python=%s\n' "$rc_rust" "$rc_node" "$rc_py"

if [ "$rc_rust" -ne 0 ] || [ "$rc_node" -ne 0 ] || [ "$rc_py" -ne 0 ]; then
  exit 1
fi

echo "[examples-test] all passed"

#!/usr/bin/env bash
set -euo pipefail

cargo fmt --all -- --check
cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings
bash scripts/workspace_quick_test.sh

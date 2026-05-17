#!/usr/bin/env bash
set -euo pipefail

cargo fmt --all -- --check
cargo clippy \
  -p nervusdb-api \
  -p nervusdb-storage \
  -p nervusdb-query \
  -p nervusdb \
  -p nervusdb-cli \
  --lib --bins \
  -- -W warnings
bash scripts/workspace_quick_test.sh

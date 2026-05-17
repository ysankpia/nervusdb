#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "[quick] core 0.1 Mini-Cypher acceptance"
cargo test -p nervusdb --test core_0_1_mini_cypher


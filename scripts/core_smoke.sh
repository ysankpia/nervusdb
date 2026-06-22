#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/nervusdb-core-smoke.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

DB_PATH="$TMP_DIR/core-smoke"

echo "[core-smoke] write social graph"
cargo run -p nervusdb-cli -- v2 write \
  --db "$DB_PATH" \
  --cypher "CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})" \
  >/tmp/nervusdb-core-smoke-write.json

echo "[core-smoke] query one-hop graph"
out="$(
  cargo run -p nervusdb-cli -- v2 query \
    --db "$DB_PATH" \
    --cypher "MATCH (a:Person)-[:KNOWS]->(b) WHERE a.name = 'Alice' RETURN b.name LIMIT 10"
)"

printf '%s\n' "$out"
printf '%s\n' "$out" | grep -q '"b.name":"Bob"'

echo "[core-smoke] fsck graph"
fsck_out="$(
  cargo run -p nervusdb-cli -- v2 fsck \
    --db "$DB_PATH" \
    --json
)"
printf '%s\n' "$fsck_out"
printf '%s\n' "$fsck_out" | grep -q '"ok": true'

echo "[core-smoke] agent memory scenario"
cargo test -p nervusdb --test core_0_1_agent_memory -- --nocapture

echo "[core-smoke] ok"

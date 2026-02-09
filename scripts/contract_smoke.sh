#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "[contract-smoke] Rust query API sanity"
cargo test -p nervusdb-v2 --test t52_query_api

echo "[contract-smoke] Python binding sanity"
cargo test -p nervusdb-pyo3

if [[ -f "nervusdb-node/Cargo.toml" ]]; then
  echo "[contract-smoke] Node contract sanity"
  cargo build --manifest-path nervusdb-node/Cargo.toml --release

  node_lib=""
  for ext in so dylib dll; do
    candidate="nervusdb-node/target/release/libnervusdb_node.${ext}"
    if [[ -f "$candidate" ]]; then
      node_lib="$candidate"
      break
    fi
  done

  if [[ -z "$node_lib" ]]; then
    echo "[contract-smoke] failed: Node addon artifact not found"
    exit 1
  fi

  addon_path="$ROOT_DIR/target/nervusdb_node_contract.node"
  mkdir -p target
  cp "$node_lib" "$addon_path"

  tmp_db="$(mktemp -d)/contract-smoke.ndb"
  node -e "
const addon = require(process.argv[1]);
const dbPath = process.argv[2];
const db = addon.Db.open(dbPath);
const executeWrite = db.executeWrite ?? db.execute_write;
if (!executeWrite) throw new Error('missing executeWrite API');
executeWrite.call(db, \"CREATE (n:Person {name:'Contract'})\");
const rows = db.query(\"MATCH (n:Person) RETURN n LIMIT 1\");
if (!rows.length) throw new Error('node contract result empty');
const value = rows[0].n;
if (!value || value.type !== 'node') throw new Error('node contract type mismatch');
db.close();
console.log('node-contract-smoke ok');
" "$addon_path" "$tmp_db"
else
  echo "[contract-smoke] Node scaffold not present, skipped"
fi

echo "[contract-smoke] done"

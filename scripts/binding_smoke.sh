#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "[binding-smoke] Python binding cargo tests"
cargo test -p nervusdb-pyo3

if [[ -f "nervusdb-node/Cargo.toml" ]]; then
  echo "[binding-smoke] Node binding cargo build"
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
    echo "[binding-smoke] failed: Node addon artifact not found"
    exit 1
  fi

  addon_path="$ROOT_DIR/target/nervusdb_node_smoke.node"
  mkdir -p target
  cp "$node_lib" "$addon_path"

  tmp_db="$(mktemp -d)/binding-smoke.ndb"
  echo "[binding-smoke] Node runtime smoke"
  node -e "
const addon = require(process.argv[1]);
const dbPath = process.argv[2];
const db = addon.Db.open(dbPath);
const executeWrite = db.executeWrite ?? db.execute_write;
if (!executeWrite) throw new Error('missing executeWrite API');
const query = db.query.bind(db);
executeWrite.call(db, \"CREATE (n:Person {name:'NodeSmoke'})\");
const rows = query(\"MATCH (n:Person) RETURN n LIMIT 1\");
if (!Array.isArray(rows) || rows.length === 0) throw new Error('empty result');
const beginWrite = db.beginWrite ?? db.begin_write;
if (!beginWrite) throw new Error('missing beginWrite API');
const txn = beginWrite.call(db);
const txnQuery = txn.query.bind(txn);
const txnCommit = txn.commit.bind(txn);
txnQuery(\"CREATE (:Person {name:'TxnSmoke'})\");
const affected = txnCommit();
if (affected <= 0) throw new Error('txn commit affected=0');
db.close();
console.log('node-binding-smoke ok');
" "$addon_path" "$tmp_db"
else
  echo "[binding-smoke] Node binding scaffold not present, skipped"
fi

echo "[binding-smoke] done"

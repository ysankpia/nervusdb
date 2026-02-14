#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$(mktemp -d)"
trap 'chmod -R u+w "$TMP_DIR" 2>/dev/null || true; rm -rf "$TMP_DIR"' EXIT

CHAOS_OUT_DIR="${CHAOS_OUT_DIR:-artifacts/chaos}"
mkdir -p "$CHAOS_OUT_DIR"
LOG_FILE="$CHAOS_OUT_DIR/chaos-$(date +%Y%m%d-%H%M%S).log"

run_expect_fail() {
  local name="$1"
  shift
  set +e
  "$@" >/dev/null 2>&1
  local rc=$?
  set -e
  if [[ "$rc" -eq 0 ]]; then
    echo "[chaos] expected failure but succeeded: $name" | tee -a "$LOG_FILE" >&2
    exit 1
  fi
  echo "[chaos] expected failure observed: $name (rc=$rc)" | tee -a "$LOG_FILE"
}

echo "[chaos] crash-test smoke" | tee -a "$LOG_FILE"
cargo run -p nervusdb-storage --bin nervusdb-v2-crash-test -- \
  driver "$TMP_DIR/crash-db" \
  --iterations 20 --min-ms 2 --max-ms 8 --batch 64 --node-pool 64 --rel-pool 8 \
  --verify-retries 20 --verify-backoff-ms 10 | tee -a "$LOG_FILE"

echo "[chaos] permission-denied simulation" | tee -a "$LOG_FILE"
mkdir -p "$TMP_DIR/readonly"
chmod 500 "$TMP_DIR/readonly"
run_expect_fail \
  "readonly-parent-write" \
  cargo run -p nervusdb-cli -- v2 write --db "$TMP_DIR/readonly/demo" --cypher "CREATE (n {name:'x'})"

# Make sure directory is mutable for cleanup and subsequent checks.
chmod 700 "$TMP_DIR/readonly"

echo "[chaos] no-such-directory simulation" | tee -a "$LOG_FILE"
run_expect_fail \
  "missing-parent-write" \
  cargo run -p nervusdb-cli -- v2 write --db "$TMP_DIR/missing/child/demo" --cypher "CREATE (n {name:'x'})"

echo "[chaos] invalid-db-file-type simulation" | tee -a "$LOG_FILE"
echo "not-a-directory" > "$TMP_DIR/not_dir"
run_expect_fail \
  "invalid-db-path-file" \
  cargo run -p nervusdb-cli -- v2 write --db "$TMP_DIR/not_dir/subdb" --cypher "CREATE (n {name:'x'})"

echo "[chaos] wal-recovery verification" | tee -a "$LOG_FILE"
DB_PATH="$TMP_DIR/recovery-demo"
cargo run -p nervusdb-cli -- v2 write --db "$DB_PATH" --cypher "CREATE (a {name:'recover'})-[:1]->(b {name:'ok'})" >/dev/null
QUERY_OUT="$(cargo run -q -p nervusdb-cli -- v2 query --db "$DB_PATH" --cypher "MATCH (a {name:'recover'})-[:1]->(b {name:'ok'}) RETURN a, b")"
if ! grep -q 'recover' <<<"$QUERY_OUT"; then
  echo "[chaos] wal recovery query failed" | tee -a "$LOG_FILE" >&2
  exit 1
fi

echo "[chaos] checks passed" | tee -a "$LOG_FILE"
echo "[chaos] log: $LOG_FILE"

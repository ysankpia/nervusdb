#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

EXAMPLE_DIR="$ROOT_DIR/examples/core-0.1"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/nervusdb-core-examples.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

run_example() {
  local name="$1"
  local dir="$EXAMPLE_DIR/$name"
  local work="$TMP_DIR/$name"
  local db_path="$work/db"
  local actual="$work/actual.ndjson"
  local actual_sorted="$work/actual.sorted.ndjson"
  local expected_sorted="$work/expected.sorted.ndjson"

  if [[ ! -d "$dir" ]]; then
    echo "[core-examples] missing example: $name" >&2
    exit 2
  fi

  if [[ ! -f "$dir/query.cypher" || ! -f "$dir/expected.ndjson" ]]; then
    echo "[core-examples] incomplete example fixture: $name" >&2
    exit 2
  fi

  mkdir -p "$work"

  local write_count=0
  local write_file
  for write_file in "$dir"/write-*.cypher; do
    if [[ ! -e "$write_file" ]]; then
      continue
    fi
    cargo run -q -p nervusdb-cli -- v2 write \
      --db "$db_path" \
      --file "$write_file" \
      >"$work/$(basename "$write_file").json"
    write_count=$((write_count + 1))
  done

  if [[ "$write_count" -eq 0 ]]; then
    echo "[core-examples] no write-*.cypher files for: $name" >&2
    exit 2
  fi

  cargo run -q -p nervusdb-cli -- v2 query \
    --db "$db_path" \
    --file "$dir/query.cypher" \
    >"$actual"

  LC_ALL=C sort "$dir/expected.ndjson" >"$expected_sorted"
  LC_ALL=C sort "$actual" >"$actual_sorted"

  if ! diff -u "$expected_sorted" "$actual_sorted"; then
    echo "[core-examples] example failed: $name" >&2
    echo "[core-examples] expected: $dir/expected.ndjson" >&2
    echo "[core-examples] actual: $actual" >&2
    exit 1
  fi

  if [[ "$name" == "10-import-then-query" ]]; then
    local reopen_actual="$work/actual-reopen.ndjson"
    local reopen_sorted="$work/actual-reopen.sorted.ndjson"

    cargo run -q -p nervusdb-cli -- v2 query \
      --db "$db_path" \
      --file "$dir/query.cypher" \
      >"$reopen_actual"
    LC_ALL=C sort "$reopen_actual" >"$reopen_sorted"

    if ! diff -u "$expected_sorted" "$reopen_sorted"; then
      echo "[core-examples] reopen query failed: $name" >&2
      echo "[core-examples] expected: $dir/expected.ndjson" >&2
      echo "[core-examples] actual: $reopen_actual" >&2
      exit 1
    fi
  fi

  echo "[core-examples] $name ok"
}

if [[ "$#" -gt 0 ]]; then
  for name in "$@"; do
    run_example "$name"
  done
else
  for dir in "$EXAMPLE_DIR"/*; do
    [[ -d "$dir" ]] || continue
    run_example "$(basename "$dir")"
  done
fi

echo "[core-examples] done"

# NervusDB CLI Reference

The CLI is a 0.1 support tool for local smoke, debug, query, write, and
file-driven import-style workflows. It is not a separate platform product.

## Run Locally

```bash
cargo run -p nervusdb-cli -- <command>
```

## Core Commands

```text
v2 query   Execute supported Mini-Cypher read queries.
v2 write   Execute supported Mini-Cypher write statements.
v2 repl    Local interactive debug session.
```

`v2 vacuum` is a maintenance command if present. It is not a 0.1 core stability
promise until the API surface doc promotes it.

## Database Path

`--db <path>` is a local database directory. The CLI must not document `.ndb`
and `.wal` as public files.

## Query

```bash
cargo run -p nervusdb-cli -- v2 query \
  --db /tmp/nervusdb-demo \
  --cypher "MATCH (n:Person) RETURN n.name LIMIT 10"
```

Options:

- `--db <path>`: database directory.
- `--cypher <query>`: query string. Mutually exclusive with `--file`.
- `--file <path>`: read query from a file.
- `--params-json <json>`: parameters as a JSON object. The 0.1 CLI path accepts
  scalar values only.
- `--format ndjson`: output newline-delimited JSON, one row per line.

Output is NDJSON. For example:

```json
{"n.name":"Alice"}
```

Stay inside `docs/reference/mini-cypher.md` for 0.1-supported reads.

## Write

```bash
cargo run -p nervusdb-cli -- v2 write \
  --db /tmp/nervusdb-demo \
  --cypher "CREATE (n:Person {name: 'Alice'})"
```

Supported 0.1 write usage is basic `CREATE`, stable `SET`, and stable `DELETE`
paths documented by Mini-Cypher tests.

Options:

- `--db <path>`: database directory.
- `--cypher <query>`: write statement. Mutually exclusive with `--file`.
- `--file <path>`: read one write statement from a file.
- `--params-json <json>`: parameters as a JSON object. The 0.1 CLI path accepts
  scalar values only.

Successful writes print a small JSON status object:

```json
{"count":1}
```

## Import Smoke

Import-style workflows are file-driven smoke/debug helpers for proving local
graph loading. Use existing `v2 write --file <path>` inputs, usually one write
statement per file. Do not add or document a stable `import` subcommand before
0.1.

## Environment

```bash
RUST_LOG=debug cargo run -p nervusdb-cli -- v2 query \
  --db /tmp/nervusdb-demo \
  --cypher "MATCH (n) RETURN n LIMIT 5"
```

## Exit Codes

- `0`: success.
- `1`: syntax, IO, storage, query, or other command failure.

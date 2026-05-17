# NervusDB CLI Reference

The CLI is a 0.1 support tool for local smoke, debug, query, write, and import
style workflows. It is not a separate platform product.

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

`v2 vacuum` and other maintenance-oriented commands may exist, but they are not
0.1 product promises until the API surface doc promotes them.

## Query

```bash
cargo run -p nervusdb-cli -- v2 query \
  --db /tmp/nervusdb-demo \
  --cypher "MATCH (n:Person) RETURN n.name LIMIT 10"
```

Options:

- `--db <path>`: database base path.
- `--cypher <query>`: query string.
- `--file <path>`: read query from a file.
- `--params-json <json>`: parameters as a JSON object.
- `--format <fmt>`: output format when supported.

Stay inside `docs/reference/mini-cypher.md` for 0.1-supported reads.

## Write

```bash
cargo run -p nervusdb-cli -- v2 write \
  --db /tmp/nervusdb-demo \
  --cypher "CREATE (n:Person {name: 'Alice'})"
```

Supported 0.1 write usage is basic `CREATE`, stable `SET`, and stable `DELETE`
paths documented by Mini-Cypher tests.

## Import Smoke

Import-style workflows are allowed as smoke/debug helpers for proving local graph
loading. They should not become a broad ETL product surface before 0.1.

## Environment

```bash
RUST_LOG=debug cargo run -p nervusdb-cli -- v2 query \
  --db /tmp/nervusdb-demo \
  --cypher "MATCH (n) RETURN n LIMIT 5"
```

## Exit Codes

- `0`: success.
- `1`: syntax, IO, storage, query, or other command failure.

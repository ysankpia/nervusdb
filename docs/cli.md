# NervusDB CLI Reference

## Installation

```bash
cargo install --path nervusdb-cli
# Or run directly:
cargo run -p nervusdb-cli -- <command>
```

## Commands

```
nervusdb v2 query   — Execute Cypher read queries
nervusdb v2 write   — Execute Cypher write queries
nervusdb v2 repl    — Interactive REPL session
nervusdb v2 vacuum  — Reclaim storage space
```

---

## v2 query

Execute a Cypher read query and print results as NDJSON.

```bash
nervusdb-cli v2 query --db <path> --cypher <query>
nervusdb-cli v2 query --db <path> --file <query-file>
```

| Option | Description |
|--------|-------------|
| `--db <path>` | Database base path (required) |
| `--cypher <query>` | Cypher query string |
| `--file <path>` | Read query from file (conflicts with `--cypher`) |
| `--params-json <json>` | Parameters as JSON object |
| `--format <fmt>` | Output format: `ndjson` (default) |

Output is NDJSON (one JSON object per line):

```bash
$ nervusdb-cli v2 query --db /tmp/demo \
    --cypher "MATCH (n:Person) RETURN n.name, n.age"
{"n.name":"Alice","n.age":30}
{"n.name":"Bob","n.age":25}
```

---

## v2 write

Execute a Cypher write query (`CREATE`, `MERGE`, `DELETE`, `SET`, `REMOVE`).

```bash
nervusdb-cli v2 write --db <path> --cypher <query>
nervusdb-cli v2 write --db <path> --file <query-file>
```

| Option | Description |
|--------|-------------|
| `--db <path>` | Database base path (required) |
| `--cypher <query>` | Cypher write statement |
| `--file <path>` | Read query from file (conflicts with `--cypher`) |
| `--params-json <json>` | Parameters as JSON object |

Output is a JSON count of affected entities:

```bash
$ nervusdb-cli v2 write --db /tmp/demo \
    --cypher "CREATE (n:Person {name: 'Alice', age: 30})"
{"count":1}
```

---

## v2 repl

Start an interactive REPL session.

```bash
nervusdb-cli v2 repl --db <path>
```

| Option | Description |
|--------|-------------|
| `--db <path>` | Database base path (required) |

---

## v2 vacuum

Reclaim storage space by rewriting the database file.

```bash
nervusdb-cli v2 vacuum --db <path>
```

| Option | Description |
|--------|-------------|
| `--db <path>` | Database base path (required) |

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `RUST_LOG` | Log level: `debug`, `info`, `warn`, `error` |

```bash
RUST_LOG=debug nervusdb-cli v2 query --db /tmp/demo --cypher "MATCH (n) RETURN n"
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error (syntax, I/O, etc.) |

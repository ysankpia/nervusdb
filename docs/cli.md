# NervusDB CLI Reference

## Table of Contents

1. [Installation](#installation)
2. [Command Overview](#command-overview)
3. [Global Options](#global-options)
4. [Commands](#commands)
   - [v2 write](#v2-write)
   - [v2 query](#v2-query)
   - [v2 backup](#v2-backup)
   - [v2 restore](#v2-restore)
   - [v2 info](#v2-info)
5. [Examples](#examples)

---

## Installation

### From Source

```bash
cargo install nervusdb-cli
```

### From Binary

Download from [GitHub Releases](https://github.com/LuQing-Studio/nervusdb/releases).

---

## Command Overview

```
nervusdb-cli <command> [options]
```

### Available Commands

| Command | Description |
|---------|-------------|
| `v2 write` | Execute Cypher write queries (CREATE, MERGE, DELETE, SET) |
| `v2 query` | Execute Cypher read queries (MATCH, RETURN) |
| `v2 backup` | Create a database backup |
| `v2 restore` | Restore database from backup |
| `v2 info` | Show database information |

---

## Global Options

| Option | Description |
|--------|-------------|
| `-h, --help` | Show help message |
| `--version` | Show version information |
| `--db <path>` | Database path (required for most commands) |
| `--cypher <query>` | Cypher query to execute |

---

## Commands

### v2 write

Execute Cypher write queries (CREATE, MERGE, DELETE, SET).

```bash
nervusdb-cli v2 write --db <path> --cypher <query>
```

#### Options

| Option | Description |
|--------|-------------|
| `--db <path>` | Database path (required) |
| `--cypher <query>` | Cypher query (required) |
| `-h, --help` | Show help |

#### Output

Returns JSON object with operation count:
```json
{"count": 1}
```

#### Examples

```bash
# Create a node
nervusdb-cli v2 write --db /tmp/graph --cypher "CREATE (n {name: 'Alice'})"

# Create nodes and relationship
nervusdb-cli v2 write --db /tmp/graph --cypher "CREATE (a {name: 'Alice'})-[:FRIEND]->(b {name: 'Bob'})"

# Delete nodes
nervusdb-cli v2 write --db /tmp/graph --cypher "MATCH (n) WHERE n.name = 'Bob' DELETE n"

# Update properties
nervusdb-cli v2 write --db /tmp/graph --cypher "MATCH (n {name: 'Alice'}) SET n.age = 31"
```

---

### v2 query

Execute Cypher read queries (MATCH, RETURN).

```bash
nervusdb-cli v2 query --db <path> --cypher <query>
```

#### Options

| Option | Description |
|--------|-------------|
| `--db <path>` | Database path (required) |
| `--cypher <query>` | Cypher query (required) |
| `--limit <n>` | Limit result count (default: 100) |
| `-h, --help` | Show help |

#### Output

NDJSON format (one JSON object per line):
```json
{"a":{"internal_node_id":1,"external_id":{"id":1}},"b":{"internal_node_id":2,"external_id":{"id":2}}}
{"a":{"internal_node_id":1},"b":{"internal_node_id":3}}
```

#### Examples

```bash
# Match all nodes
nervusdb-cli v2 query --db /tmp/graph --cypher "MATCH (n) RETURN n"

# Match with filter
nervusdb-cli v2 query --db /tmp/graph --cypher "MATCH (n) WHERE n.name = 'Alice' RETURN n"

# Match relationships
nervusdb-cli v2 query --db /tmp/graph --cypher "MATCH (a)-[:FRIEND]->(b) RETURN a.name, b.name"

# With limit
nervusdb-cli v2 query --db /tmp/graph --cypher "MATCH (n) RETURN n" --limit 10
```

---

### v2 backup

Create a database backup.

```bash
nervusdb-cli v2 backup --db <path> --output <path>
```

#### Options

| Option | Description |
|--------|-------------|
| `--db <path>` | Database path (required) |
| `--output <path>` | Backup output directory (required) |
| `-h, --help` | Show help |

#### Examples

```bash
# Create backup
nervusdb-cli v2 backup --db /tmp/graph --output /tmp/backup_2024_01_15
```

---

### v2 restore

Restore database from backup.

```bash
nervusdb-cli v2 restore --backup <path> --db <path>
```

#### Options

| Option | Description |
|--------|-------------|
| `--backup <path>` | Backup directory (required) |
| `--db <path>` | Target database path (required) |
| `-h, --help` | Show help |

#### Examples

```bash
# Restore from backup
nervusdb-cli v2 restore --backup /tmp/backup_2024_01_15 --db /tmp/restored_graph
```

---

### v2 info

Show database information.

```bash
nervusdb-cli v2 info --db <path>
```

#### Options

| Option | Description |
|--------|-------------|
| `--db <path>` | Database path (required) |
| `-h, --help` | Show help |

#### Output

JSON format with database metadata:
```json
{
  "version": "2.0.0",
  "node_count": 1000,
  "edge_count": 5000,
  "storage_size_bytes": 1048576
}
```

#### Examples

```bash
# Show database info
nervusdb-cli v2 info --db /tmp/graph
```

---

## Examples

### Complete Workflow

```bash
# 1. Create database and add data
nervusdb-cli v2 write --db /tmp/social --cypher "
  CREATE (alice:Person {name: 'Alice', age: 30})
"
nervusdb-cli v2 write --db /tmp/social --cypher "
  CREATE (bob:Person {name: 'Bob', age: 25})
"
nervusdb-cli v2 write --db /tmp/social --cypher "
  CREATE (alice)-[:FRIEND {since: 2020}]->(bob)
"

# 2. Query the data
nervusdb-cli v2 query --db /tmp/social --cypher "
  MATCH (a:Person)-[r:FRIEND]->(b:Person)
  WHERE a.name = 'Alice'
  RETURN a.name, b.name, r.since
"

# 3. Create backup
nervusdb-cli v2 backup --db /tmp/social --output /tmp/backups/social_$(date +%Y%m%d)

# 4. Check database info
nervusdb-cli v2 info --db /tmp/social
```

---

## Exit Codes

| Code | Description |
|------|-------------|
| 0 | Success |
| 1 | Error (invalid query, I/O error, etc.) |

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `RUST_LOG` | Set log level (debug, info, warn, error) |

Example:
```bash
RUST_LOG=debug nervusdb-cli v2 query --db /tmp/graph --cypher "MATCH (n) RETURN n"
```

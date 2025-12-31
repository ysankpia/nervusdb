# T202 Implementation Plan: Bulk Import Tool (CSV/JSONL)

## 1. Overview

Implement a command-line utility (`ndb-import`) to bulk load data from CSV/JSONL files into a new NervusDB database. This tool utilizes the specific `BulkLoader` API (T157) to bypass the WAL and achieve high-performance importing.

## 2. Requirements Analysis

### 2.1 Use Scenarios

- **Initial Data Load**: User has a large dataset in CSV/JSONL (e.g., from another DB dump) and wants to initialize NervusDB quickly.
- **Benchmark Data Generation**: Loading synthetic datasets for performance testing.

### 2.2 Functional Requirements

- **Input Formats**:
  - **CSV**: Header mapping support (Neo4j-style headers preferred for compatibility).
    - Nodes: `id:ID,:LABEL,name,age:int`
    - Edges: `:START_ID,:END_ID,:TYPE,since:int`
  - **JSONL**: One JSON object per line.
- **CLI**:
  - `ndb-import --nodes nodes.csv --edges edges.csv --output my.ndb`
- **Performance**:
  - Stream processing (don't load entire file into RAM, though `BulkLoader` currently buffers in RAM, we might need to verify if `BulkLoader` can handle streaming or if we are limited by RAM. _Correction_: `BulkLoader` in `nervusdb-v2-storage` uses `Vec<BulkNode>`, so it IS limited by RAM currently. For this task, we will accept RAM limitation but ensure parsing is efficient).

### 2.3 Limits & Constraints

- **New DB Only**: `BulkLoader` only supports creating fresh databases.
- **Memory Bound**: Current `BulkLoader` stores all nodes/edges in memory before commit. This task focuses on the _Tooling_ (CLI/Parsing), not rewriting `BulkLoader` to be disk-based (which would be a separate Storage task).

## 3. Design

### 3.1 CLI Structure

We will create a new binary target in `nervusdb-v2/src/bin/ndb-import.rs`.

```bash
ndb-import \
  --nodes users.csv --nodes posts.csv \
  --edges follows.csv \
  --format csv \
  --output ./graph.ndb
```

### 3.2 CSV Format Spec (Neo4j-ish)

Headers define type inference:

- `id:ID` -> ExternalId (u64)
- `:LABEL` -> Node Label
- `:START_ID` -> Edge Source (u64)
- `:END_ID` -> Edge Dest (u64)
- `:TYPE` -> Edge Type
- `name` -> String property
- `age:int` -> Int property
- `score:float` -> Float property
- `active:bool` -> Bool property

### 3.3 JSONL Format Spec

```json
// Node
{"id": 1, "label": "Person", "properties": {"name": "Alice", "age": 30}}
// Edge
{"src": 1, "dst": 2, "type": "KNOWS", "properties": {"since": 2021}}
```

## 4. Implementation Plan

### Step 1: Create CLI Skeleton (Risk: Low)

- Add `clap`, `csv`, `serde`, `serde_json` dependencies to `nervusdb-v2/Cargo.toml`.
- Create `nervusdb-v2/src/bin/ndb-import.rs`.

### Step 2: Implement CSV Parser (Risk: Medium)

- Implement header parsing logic to determine types.
- Stream CSV rows -> `BulkNode` / `BulkEdge`.

### Step 3: Implement JSONL Parser (Risk: Low)

- Stream JSONL lines -> `BulkNode` / `BulkEdge`.

### Step 4: Integration (Risk: Low)

- Feed parsed objects into `BulkLoader`.
- Handle errors and reporting.

## 5. Verification Plan

### 5.1 Automated Tests

- Integration test running `ndb-import` subprocess against sample CSV/JSONL files.
- Verify generated `.ndb` file content using `GraphEngine`.

### 6. Risk Assessment

- **Memory Usage**: `BulkLoader` buffers everything. Large CSVs will OOM.
  - _Mitigation_: Document this limitation clearly. Fixing `BulkLoader` to be disk-spilling is out of scope for T202.

# T203 Implementation Plan: HNSW Index Prototype (Vector Search)

## 1. Overview

Implement a **native, page-backed HNSW (Hierarchical Navigable Small World)** index to enable Vector Search capabilities in NervusDB. This is a "Prototype" implementation focusing on correct persistence and integration with the Pager, rather than extreme optimization (SIMD/Quantization) which belongs to future tasks.

## 2. Requirements Analysis

### 2.1 Functional Requirements

- **Vector Support**: Support fixed-dimension `f32` vectors (e.g., 128d, 384d, 768d).
- **Indexing**: Construct an HNSW graph allowing approximate nearest neighbor (ANN) search.
- **Persistence**: The index must reside in the `.ndb` file (Pager-backed), not in memory-only structures.
- **CRUD Integration**: Support adding vectors to the index. Deletion can be soft (tombstoned) or lazy.

### 2.2 Performance Goals (Prototype)

- **Scale**: Support 10k-100k vectors for the prototype.
- **Latency**: Single query < 50ms (disk-backed cold start might be higher, warm cache should be fast).
- **Recall**: > 95% recall @ K=10.

### 2.3 Constraints

- **Reuse**: Leverage existing `Pager` and `BTree` primitives where possible to minimize risk.
- **Safety**: Must be crash-safe (atomic commits via WAL integration, shared with main DB).

## 3. Design

### 3.1 Architecture Components

We will introduce two new internal structures within `nervusdb-storage/src/index/hnsw/`:

1.  **`VectorStore`**: storage for the raw vector data.
2.  **`HnswGraph`**: storage for the navigation graph layers.

### 3.2 Storage Layout

#### A. VectorStore (Flat Paged Array)

Since `InternalNodeId` is a `u32` and reasonably dense, we can store vectors in a flat sequence of pages to avoid B-Tree overhead for vector lookups.

- **Formula**: `Address(NodeId) = BasePage + (NodeId * VectorSize / PageSize)`
- **Persistence**: New page type or just raw data pages managed by a meta-page.
- **Fallback**: For MVP, we can just use the **existing `BTree`** mapping `NodeId -> BlobId (Vector)` if implementing a flat array is too complex for T203. _Decision: Use BTree mapping `NodeId -> Vector` for Component Reuse in MVP._

#### B. HnswGraph (B-Tree Backed)

The graph structure (adjacency lists) needs to be persistent.
HNSW structure: For each `NodeId` at `Level`, we have a list of `NeighborIds`.

We can store the graph edges in the existing **B-Tree** (T101) with a special key encoding:

- **Key**: `[IndexID (u32)][Tag: Graph=1][Level (u8)][NodeId (u32)]`
- **Value**: `BlobId` pointing to `Vec<NodeId>` (Neighbor List encoded).

_Rationale_: This avoids writing complex custom pager logic for linked lists. B-Tree handles layout, splitting, and defragmentation.

### 3.3 HNSW Algorithm Spec

- **Distance Metric**: Euclidean (L2) and Cosine (normalized L2).
- **Construction**: Iterative insertion.
- **Parameters**: `M` (max links per node), `ef_construction` (search breadth during build).

## 4. Implementation Plan

### Step 1: `VectorIndex` Trait & Mock (Risk: Low)

- FILE: `nervusdb-storage/src/index/vector.rs`
- Define trait `VectorIndex`: `insert(id, vector)`, `search(vector, k)`.
- Implement a simple `BruteForceIndex` (Linear Scan) to validate correctness of distance functions.

### Step 2: HNSW Logic (Memory-Based) (Risk: Medium)

- FILE: `nervusdb-storage/src/index/hnsw/logic.rs`
- Implement the HNSW algorithms (insert, search_layer) using generic `Graph` and `VectorStorage` traits.
- Unit test with in-memory storage.

### Step 3: Pager Integration (Persistence) (Risk: High)

- FILE: `nervusdb-storage/src/index/hnsw/persistent.rs`
- Implement `VectorStorage` using the existing `BTree`.
- Implement `Graph` storage using the existing `BTree`.
- This binds the abstract HNSW logic to the disk pager.

### Step 4: Cypher/API Integration (Risk: Medium)

- FILE: `nervusdb/src/lib.rs`
- Add `Db::create_vector_index(label, property, dim)`.
- Add `Db::query_vector(label, property, query, k)`.

## 5. Verification Plan

### 5.1 Automated Tests

- `tests/t203_hnsw.rs`:
  1. Generate 1,000 random vectors.
  2. Insert into DB.
  3. Query top 10 nearest neighbors.
  4. Compare recall against Brute Force text (Step 1).
  5. Restart DB (reopen) and verify index persistence.

### 5.2 Manual Verification

- Index build time logging.
- Check `.ndb` file size growth.

## 6. Future Improvements (Post-Prototype)

- SIMD optimization for distance calc.
- Dedicated `VectorPage` layout (removing B-Tree overhead for vector lookup).
- Quantization (PQ/SQ).

# NervusDB Archive - TypeScript Reference Implementations

This directory contains TypeScript implementations that have been archived as part of the "Rust Core First" architecture migration.

## Why These Files Were Archived

These files represent valuable reference implementations that were originally written in TypeScript but should be reimplemented in the Rust core (`nervusdb-core`) for the following reasons:

1. **Performance**: Rust implementations will be 10-100x faster, especially for graph algorithms
2. **Portability**: Rust core functions can be exposed to Python, C, WASM - not just Node.js
3. **Single Source of Truth**: Avoids maintaining duplicate implementations in TS and Rust
4. **SQLite Model**: Following the "embeddable database" pattern where all logic lives in the core

## Archived Modules

### ts-algorithms/
Graph algorithms (PageRank, Dijkstra, Louvain, etc.) - **Reference for Rust implementation**
- `centrality.ts` - PageRank, Betweenness, Closeness, Degree centrality
- `community.ts` - Louvain, Label Propagation
- `pathfinding.ts` - Dijkstra, A*, Floyd-Warshall, Bellman-Ford
- `similarity.ts` - Jaccard, Cosine, Adamic-Adar similarity

### ts-fulltext/
Full-text search engine - **Reference for Rust implementation**
- `analyzer.ts` - Tokenization, stemming, stopwords
- `invertedIndex.ts` - Inverted index structure
- `scorer.ts` - TF-IDF, BM25 scoring algorithms
- `query.ts` - Boolean queries, fuzzy search

### ts-spatial/
Spatial indexing - **Reference for Rust implementation**
- `rtree.ts` - R-Tree implementation
- `geometry.ts` - Haversine distance, geometric calculations
- `spatialQuery.ts` - Spatial query API

### ts-query-pattern/
TypeScript Cypher parser - **Duplicated in Rust, can be deleted**
This was a parallel implementation to `nervusdb-core/src/query/parser.rs`

### ts-temporal-memory/
Temporal memory system - **Mostly duplicated in Rust temporal.rs**

### ts-tests/
Unit and integration tests for the archived modules

## Migration Path

1. **Phase 1**: Archive TS implementations (this directory)
2. **Phase 2**: Implement in Rust core (`nervusdb-core/src/`)
3. **Phase 3**: Expose via FFI to Node.js, Python, C
4. **Phase 4**: Delete this archive after Rust implementation is complete

## Using as Reference

When implementing these features in Rust, refer to:
- Algorithm logic and edge cases
- Test cases (for porting to Rust tests)
- API design patterns

---
*Archived on: $(date -I)*
*Migration Goal: "Graph database SQLite" - Thin bindings, Fat Rust core*

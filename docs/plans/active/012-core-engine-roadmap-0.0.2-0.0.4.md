# 012 Core Engine Roadmap 0.0.2 To 0.0.4

## Status

Candidate Roadmap

## Purpose

Record likely post-0.0.1 directions without turning them into current scope.

This document is deliberately not an implementation plan. Future work still
requires ADRs, tests, and focused plans.

## 0.0.2 Candidate: Correctness And Minimal Indexing

Strong candidates:

- dangling-edge rejection on write commit
- tombstone cleanup for node properties, labels, adjacency, and edge properties
- minimal node property equality index for:

  ```cypher
  MATCH (n:Label) WHERE n.key = literal
  ```

Rules:

- Do not restore `create_index` or `lookup_index` as empty public hooks.
- If property indexes return, they need a new ADR defining key layout, value
  encoding, update/delete cleanup, rebuild/backfill, query planner routing, and
  tests.
- No range index in 0.0.2 unless equality index is already correct.

## 0.0.3 Candidate: Query And Performance Evidence

Possible work:

- benchmark-driven query improvements
- more complete projection/filter behavior inside Mini-Cypher boundaries
- small query features only if they have clear tests and do not revive full
  openCypher scope

Deferred unless proven by benchmark or real use:

- hot property cache
- multi-writer OCC
- independent edge IDs
- parallel edges

## 0.0.4 Candidate: Operations

Possible work:

- graph consistency checker
- backup/export workflow
- benchmark baseline reporting

Deferred:

- HNSW/vector/GraphRAG
- online hot backup unless Fjall provides a clear supported API
- fsck repair mode before a read-only checker exists

## Non-Negotiable Rules

- Release one small true thing at a time.
- Do not expose public APIs before the storage and query semantics exist.
- Do not add cache before benchmark evidence.
- Do not change edge identity without an ADR and migration story.
- Do not revive platform-era breadth as a default gate.

## Current Recommendation

Finish 0.0.1 release first. Then start 0.0.2 with correctness:

1. dangling-edge rejection
2. tombstone cleanup
3. property equality index ADR

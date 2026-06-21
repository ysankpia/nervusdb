# ADR 0002: Freeze Platform Expansion Before 0.1

## Status

Accepted, amended by ADR 0005 and the 2026-06-21 query pruning work.

## Context

NervusDB had grown multiple success definitions at once: full openCypher
compatibility, multi-language SDK parity, vector search, optimizer breadth,
industrial nightly gates, release windows, fuzzing, chaos, soak, perf, and
embedded storage correctness.

Those are not independent goals. Each one adds combinatorial surface area to the
query engine, storage model, CI loop, docs, bindings, and release process. The
result is a project that can look busy without getting closer to a credible 0.1
embedded database.

## Decision

Before 0.1, platform expansion is frozen. The default development and validation
path is the embedded Rust core:

- local database open/reopen
- node, relationship, label, and property persistence
- Fjall-backed committed persistence and crash/reopen smoke
- one-hop/two-hop traversal
- Mini-Cypher only
- CLI smoke/debug/import support

TCK, bindings, vector/HNSW, optimizer expansion, fuzz, chaos, soak, perf, and
stability/release windows remain available as manual or historical signals. They
do not run on a schedule and do not define product readiness before 0.1.

## Consequences

- Default CI is smaller and must stay tied to `scripts/check.sh`.
- Scheduled pressure workflows become manual-only.
- Root documentation and quick starts show Rust + CLI core paths only.
- Frozen advanced code does not have to be preserved. If it is executable in the
  main path and not part of Mini-Cypher 0.1, it should be rejected, archived, or
  deleted rather than treated as a future asset. New growth in frozen areas
  requires a new decision record.
- Future hard isolation can use features, workspace exclusions, or crate moves
  after the soft isolation is stable.

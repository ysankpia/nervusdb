# NervusDB Documentation Index

This is the navigation root for current project knowledge. If another document
conflicts with this map, trust this map first and update the stale document.

## Current 0.1 Line

- Product vision: `docs/product/vision.md`
- 0.1 scope boundary: `docs/product/scope-0.1.md`
- Architecture overview: `docs/architecture/overview.md`
- Workspace layers: `docs/architecture/workspace-layers.md`
- Coding standards: `docs/engineering/coding-standards.md`
- Testing strategy: `docs/engineering/testing-strategy.md`
- Branching and PR rules: `docs/engineering/branching-pr.md`
- Definition of done: `docs/engineering/definition-of-done.md`
- Local validation runbook: `docs/runbooks/local-validation.md`
- Scope reset decision: `docs/decisions/0001-reset-scope-to-sqlite-for-graphs.md`
- Platform freeze decision: `docs/decisions/0002-freeze-platform-expansion-before-0.1.md`
- Active plans: `docs/plans/active/`
- Current slimdown plan: `docs/plans/active/002-core-0.1-slimdown.md`
- Plan template: `docs/plans/template.md`
- Mini-Cypher reference: `docs/reference/mini-cypher.md`
- Glossary: `docs/reference/glossary.md`
- Bug ledger: `docs/bugs/index.md`

## Current Code References

- Public Rust API: `nervusdb/`
- Storage engine: `nervusdb-storage/`
- Query layer: `nervusdb-query/`
- Storage/API boundary traits: `nervusdb-api/`
- CLI: `nervusdb-cli/`

The active layer map is `docs/architecture/workspace-layers.md`. It is the
current source for what is core, experimental, and frozen.

## Historical Or Experimental References

These documents are useful evidence, but they are not the 0.1 product scope:

- Full openCypher and procedures: `docs/design/T300-cypher-full.md`,
  `docs/design/T320-procedures.md`, `docs/cypher-support.md`
- HNSW/vector work: `docs/design/T203-hnsw-index.md`,
  `docs/perf/v2/hnsw-default-recommendation.md`
- Multi-language binding parity: `docs/binding-parity.md`,
  `examples-test/capability-contract.yaml`
- Beta and industrial gates: `docs/ROADMAP.md`, `docs/tasks.md`,
  `docs/publishing.md`, `docs/beta-daily-template.md`
- Older architecture and planning material: `docs/archive/`, `docs/refactor/`,
  `docs/hypothetical-architecture/`

Historical documents may mention full TCK, multi-binding parity, vector search,
or SQLite-Beta release status. Those are not 0.1 requirements unless a current
document explicitly re-promotes them.

Default CI and local validation do not run historical or experimental gates.
Use those scripts manually only when touching their area.

## Update Rule

When behavior, API, data format, build, release, validation, or project scope
changes, update the relevant current document in the same PR.

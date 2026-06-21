# NervusDB Documentation Index

This is the current map for the SQLite-for-graphs 0.1 refactor. If a document is
not linked here, do not use it to infer current scope.

Current active direction: Fjall-backed local database directory storage for the
0.1 embedded Rust graph core.

## Product

- Direction contract: `docs/product/direction-contract.md`
- Vision: `docs/product/vision.md`
- 0.1 scope: `docs/product/scope-0.1.md`
- 0.1 user stories: `docs/product/user-stories-0.1.md`
- Non-goals: `docs/product/non-goals.md`

## Architecture

- Overview: `docs/architecture/overview.md`
- Crate boundaries: `docs/architecture/crate-boundaries.md`
- Storage model: `docs/architecture/storage-model.md`
- Query model: `docs/architecture/query-model.md`
- API surface: `docs/architecture/api-surface.md`
- Workspace layers: `docs/architecture/workspace-layers.md`

## Engineering

- Coding standards: `docs/engineering/coding-standards.md`
- Architecture invariants: `docs/engineering/architecture-invariants.md`
- Quality score: `docs/engineering/quality-score.md`
- Git workflow: `docs/engineering/git-workflow.md`
- Dependency policy: `docs/engineering/dependency-policy.md`
- Testing strategy: `docs/engineering/testing-strategy.md`
- Validation policy: `docs/engineering/validation-policy.md`
- Refactor policy: `docs/engineering/refactor-policy.md`
- Documentation policy: `docs/engineering/documentation-policy.md`
- Definition of done: `docs/engineering/definition-of-done.md`

## Runbooks And Reference

- Local setup: `docs/runbooks/local-setup.md`
- Local validation: `docs/runbooks/local-validation.md`
- Crash recovery validation: `docs/runbooks/crash-recovery-validation.md`
- Benchmark validation: `docs/runbooks/benchmark-validation.md`
- Release readiness: `docs/runbooks/release-readiness.md`
- Doc gardening: `docs/runbooks/doc-gardening.md`
- Mini-Cypher: `docs/reference/mini-cypher.md`
- Rust API: `docs/reference/rust-api.md`
- 0.1 examples: `docs/reference/examples-0.1.md`
- Storage format: `docs/reference/storage-format.md`
- CLI: `docs/reference/cli.md`
- Generated artifacts: `docs/reference/generated-artifacts.md`
- Codebase analysis: `docs/reference/codebase-analysis.md`
- Glossary: `docs/reference/glossary.md`

## Roadmap And Progress

- Roadmap: `docs/roadmap.md`
- Progress: `PROGRESS.md`

## Plans

- Active Fjall storage refactor: `docs/plans/active/010-fjall-storage-refactor.md`
- Active 0.0.1 release plan: `docs/plans/active/011-release-0.0.1-single-crate.md`
- Candidate core engine roadmap: `docs/plans/active/012-core-engine-roadmap-0.0.2-0.0.4.md`
- Active 0.0.2 write path plan: `docs/plans/active/013-write-path-and-bulk-import-0.0.2.md`
- Active plans: `docs/plans/active/`
- Completed plans: `docs/plans/completed/`
- Technical debt: `docs/plans/tech-debt.md`
- Plan template: `docs/plans/template.md`
- Decision records: `docs/decisions/`

## Bugs

- Bug ledger: `docs/bugs/index.md`

## Deleted Legacy Material

Platform-era archive docs and old fuzz targets were removed from the working
tree. Reviving any of that material requires a new ADR that updates product,
architecture, validation, and plan docs.

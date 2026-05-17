# Refactor Policy

Refactoring is allowed because this project is pre-0.1, but it must reduce
complexity instead of moving it around.

## Rules

- No feature expansion during refactor.
- No broad rewrite without an active plan under `docs/plans/active/`.
- Each plan must name the touched layer: storage, query, API, CLI, docs, CI, or
  archive.
- Split the work if it touches more than one core layer unless the active plan
  explicitly explains why a single change is safer.
- Update architecture or reference docs in the same change when boundaries or
  behavior move.
- Keep old platform-era material archived unless a new ADR revives it.

## First Principles Check

Before expanding scope, answer these questions in the plan:

- Does this help a Rust program open a local graph database?
- Does it improve persistence, recovery, traversal, or Mini-Cypher correctness?
- Can it be validated quickly in the default loop?
- If it is not core, why is it being done before 0.1?

If the answer is weak, do not do it.

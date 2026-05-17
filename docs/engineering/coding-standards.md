# Coding Standards

These rules exist to keep the 0.1 core small enough to finish.

## General Rules

- Prefer small, focused modules over growing central files.
- Add abstractions only when they remove real duplication or protect a concrete
  boundary that already exists.
- Keep error paths explicit. Do not swallow storage, IO, WAL, recovery, or query
  errors.
- Do not add dependencies for simple local logic.
- Public behavior changes must update product, architecture, reference, or
  runbook docs in the same change.
- Generated artifacts must document their regeneration command.

## Rust Rules

- Use `cargo fmt --all` formatting.
- Keep invariants near the code that enforces them.
- Storage, WAL, and file format changes require reopen or recovery tests.
- Query behavior changes require deterministic Mini-Cypher tests.
- API changes must be visible through the Rust facade before any binding-facing
  wrapper matters.

## Scope Discipline

The default answer to new full-Cypher, vector, SDK, optimizer, or industrial-gate
work is "not before 0.1" unless `docs/product/scope-0.1.md` and an ADR change
first.

Every change must either move the embedded graph core forward or isolate
non-core work more cleanly. Busy work in frozen surfaces is still busy work.

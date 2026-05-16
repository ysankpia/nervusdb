# Coding Standards

## General Rules

- Prefer small, focused modules over growing central files.
- Add abstractions only when they remove real duplication or protect a concrete
  boundary.
- Keep error paths explicit. Do not swallow storage, IO, or query errors.
- Public behavior changes must be documented in the same PR.
- Generated artifacts must document their regeneration command.

## Rust Rules

- Run `cargo fmt --all -- --check` before submitting.
- Run clippy with repository settings:

```bash
cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings
```

- Keep storage and query invariants close to the code that enforces them.
- Do not introduce new dependencies for simple local logic.
- File format and WAL changes require tests that reopen or recover data.

## Scope Discipline

The default answer to new full-Cypher, vector, SDK, or industrial-gate work is
"not before 0.1" unless `docs/product/scope-0.1.md` is changed first.

# Documentation Policy

Docs are part of the harness. They should make the next implementation step
clear, not preserve every old ambition in the current path.

## Current Docs

- `docs/index.md` is the default map.
- Product docs define what is in and out before 0.1.
- Architecture docs define crate and layer boundaries.
- Engineering docs define workflow and validation rules.
- Runbooks define exact commands.
- Reference docs define supported user-facing behavior.
- Active plans define current execution.

## Update Rules

- Product behavior changes update `docs/product/` and relevant reference docs.
- Crate, layer, or ownership changes update `docs/architecture/`.
- Validation, CI, or script changes update `docs/engineering/` or `docs/runbooks/`.
- Public CLI or query behavior changes update `docs/reference/`.
- Bug fixes add or update a record under `docs/bugs/` when the issue can recur.
- Quality score, technical debt, and architecture invariants update when a change
  reveals a quality gap, accepted debt, boundary violation, or new invariant.
- Doc-gardening pass after substantial work, bug fixes, architecture changes,
  validation changes, or long handoffs.

## Archive Rules

Historical docs live under `docs/archive/legacy-platform-era/`. Do not edit them
to describe current scope. If an archived idea should become current again,
write an ADR first and update product, architecture, validation, and plan docs.

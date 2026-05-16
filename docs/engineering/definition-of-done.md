# Definition Of Done

A change is done when:

- It matches `docs/product/scope-0.1.md`.
- Tests or regression guards cover behavior changes.
- Public API, storage format, build, validation, or operational changes update
  docs in the same PR.
- `bash scripts/check.sh` passes, or any skipped part is explicitly explained.
- Historical/experimental areas are not promoted into the 0.1 path by accident.
- `git status --short` contains only intentional changes before commit.

## Bug Fixes

Bug fixes must include a regression guard. Prefer a deterministic test. If that
is not practical, document the prevention guard in `docs/bugs/`.

## Storage And Recovery Changes

Storage, WAL, page format, and recovery changes are not done until reopen or
replay behavior is tested.

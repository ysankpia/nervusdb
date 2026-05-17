# ADR 0003: Fast Default Validation

## Status

Accepted

## Context

The old development loop could spend close to an hour proving unrelated
historical surfaces after small changes. That is not discipline. It makes
developers avoid validation or batch risky changes.

For a pre-0.1 embedded database, the default loop must prove the core path
quickly: formatting, core clippy, and the Mini-Cypher/core acceptance test. Slow
historical coverage still matters for targeted changes and release readiness,
but it must be explicit.

## Decision

`scripts/check.sh` is the default validation entry point. It must stay tied to
the 0.1 core and must not hide full workspace fan-out.

`scripts/workspace_full_test.sh` is manual. Git hooks, quick scripts,
pre-commit, pre-push, and default CI must not call it.

Scheduled full TCK, binding, vector, perf, fuzz, chaos, soak, stability, and
release-window pressure is not part of the default 0.1 loop.

## Consequences

- Ordinary docs and core query work gets fast feedback.
- Broad refactors and releases can still run full manual checks.
- A future ADR is required before slow gates become default again.
- Validation decisions must be documented in `docs/engineering/validation-policy.md`.

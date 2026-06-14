# Glossary

## Direction Contract

The product decision record at `docs/product/direction-contract.md` that defines
what NervusDB is, who it is for, what is in and out of scope, and acceptance
criteria for 0.1.

## Roadmap

The current-phase, now/next/later, milestone, and open-questions document at
`docs/roadmap.md`.

## PROGRESS.md

The live execution ledger at the repository root that tracks current objective,
active plan, done/next/blockers, validation log, and last checkpoint.

## Quality Score

The 0-5 assessment at `docs/engineering/quality-score.md` with evidence across
product/domain, architecture, validation, documentation, and maintainability.

## Architecture Invariants

Always-true rules about crate boundaries, data flow, and system properties,
recorded at `docs/engineering/architecture-invariants.md`.

## Technical Debt

The debt ledger at `docs/plans/tech-debt.md` that records active debt, deferred
cleanup, accepted debt, and retired items.

## Doc Gardening

The recurring maintenance pass defined in `docs/runbooks/doc-gardening.md` that
checks links, archives plans, verifies bug records, updates quality scores, and
removes stale instructions.

## SQLite For Graphs

The product direction for NervusDB 0.1: embedded, local-file, Rust-first graph
storage with crash recovery and a small query surface.

## Embedded Database

A library opened inside the application process. NervusDB 0.1 does not require a
server, daemon, cluster, or network API.

## Mini-Cypher

The deliberately small query subset kept on the 0.1 path. It is not full
openCypher compatibility.

## Core

The path that must work for 0.1: Rust API, local files, WAL recovery, graph
persistence, traversal, Mini-Cypher, and CLI smoke/debug/import support.

## Experimental

Code or scripts that may be useful later but are not allowed to define 0.1
success.

## Frozen

Areas where build and security maintenance are allowed but new capability work
requires a new ADR before 0.1.

## Historical Gate

A validation script or document from the Beta/full-platform phase that remains
available but is not part of the default 0.1 development loop.

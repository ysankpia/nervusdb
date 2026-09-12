# Testing

## Running

```bash
# The full gate — this is what CI runs
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Two of those are easy to skip and both have already failed CI once:
`rustdoc -D warnings` rejects unescaped angle brackets in doc comments
(`<expr>`, `<NodeId>` are parsed as HTML), and clippy's lint set moves with the
compiler, so a toolchain update can introduce a new lint that turns a green tree
red. When that happens, fix the code rather than pinning an older compiler.

The throughput-sensitive suites only mean anything when optimised:

```bash
cargo test --release --test batch_tx_tests
cargo test --release --test edge_locality_tests
```

## Current state

**151 test cases across 13 suites — 150 pass, 1 intentionally `#[ignore]`d** (a
child-process lock probe launched by its parent test).

| Suite                        | Cases | Covers                                                             |
| ---------------------------- | ----- | ------------------------------------------------------------------ |
| `integration_tests.rs`       | 26    | CRUD, ACID, concurrency, indexing, out-of-core stress              |
| `production_safety_tests.rs` | 25    | Exclusive lock, integrity, constraints, read-only writes, backup   |
| `cypher_advanced_tests.rs`   | 16    | Cypher 1.0 syntax closure, EXPLAIN                                 |
| `edge_locality_tests.rs`     | 9     | Weave equivalence, self-loops, false-spill elimination             |
| `robustness_tests.rs`        | 8     | File lock, auto-checkpoint, page CRC at scale, WAL replay, chunking |
| `batch_tx_tests.rs`          | 7     | Batch commits, single-fsync contract, throughput                   |
| `slotted_property_tests.rs`  | 7     | Page packing, slot reuse, compaction, density                      |
| `cli_tests.rs`               | 7     | REPL end-to-end, multi-line input, strict parsing                  |
| `analytics_tests.rs`         | 7     | PageRank, WCC, K-hop                                               |
| `steal_spill_tests.rs`       | 5     | Spilling, rollback pollution, checkpoint                           |
| `equivalence_tests.rs`       | 4     | v1.0.0 behaviour guardrails: query, transaction, API, format       |
| `studio_tests.rs`            | 4     | Browser workbench: endpoints, read-only, writer interleaving       |
| `zero_dependency_tests.rs`   | 3     | Enforces the empty dependency tree (with a negative control)       |

Run one suite:

```bash
cargo test --test production_safety_tests
cargo test --release --test batch_tx_tests -- --nocapture
```

## The house style: adversarial, not happy-path

A test that only exercises the happy path is treated here as incomplete. In
practice that means three things.

### 1. Cover the boundaries, every time

For anything you add or change, exercise:

- `N == 0` and `N == 1`;
- self-collision — source equals target (a self-loop), or an input that is also
  an output;
- duplicates and repeated calls (reentrancy);
- exactly-full capacity;
- smallest and largest valid values.

Never filter a boundary case out of a generator to make a test pass. If a case
you added breaks the code, the test is doing its job.

### 2. Verify failure atomicity, not just success

If step K of a multi-step operation fails, nothing from steps 1..K-1 may survive,
and the main database file must be **byte-identical** to before. The rollback
suites assert the file bytes directly, not just the in-memory counts.

### 3. Verify claims two independent ways

Any claim worth asserting is worth a second, independent route to the same
answer. Two forms are used in this codebase:

- **Conservation oracle** — the same quantity computed two different ways must
  agree. `integrity_check` measures node degree both by walking the on-disk chain
  pointers and by independently scanning the edge id space. Deriving both sides
  from one traversal would be self-confirmation, not a check.
- **Differential oracle** — the same input through the old and the new code path
  must produce identical results. `commit()` (batch weaving) versus
  `commit_unclustered()` (per-edge insertion) is asserted to produce identical
  graph structure and PageRank scores agreeing within 1e-12.

## Assertions must not depend on machine speed

This is the rule that was learned the hard way. A test asserted that batched
writes are ">20× faster than autocommit". It passed locally and failed on
`ubuntu-latest` at ~16×, because a cloud disk's fsync characteristics differ from
a local SSD — the assertion was measuring hardware, not the engine.

Assert the **mechanism** instead:

```
N autocommitted writes  ->  N fsyncs
1 batched transaction    ->  exactly 1 fsync
```

That holds on any hardware. Keep any speed ratio as a loose lower bound, never as
a fixed multiplier. Where a throughput floor is unavoidable, warm up first so
one-off allocation cost is not counted, and leave real headroom.

## What the suites pin down

Concrete examples of the style, so the intent is clear:

- **A real child process** attempts to open a database already held by the parent,
  and must report `DatabaseLocked`. The assertion also requires the child to have
  actually run its test (`running 1 test`), so a silently-not-run child cannot
  pass vacuously.
- **Deliberate corruption.** A node page is overwritten and the integrity check
  must report it — with a healthy-database negative control in the same suite, so
  a check that always fails cannot pass either. A second case corrupts _only_
  chain pointers, leaving counts self-consistent, which the degree-conservation
  oracle must still catch.
- **Cross-process lock probe**, **poisoned-lock recovery** (with a `catch_unwind`
  control proving plain `read().unwrap()` does panic), and **API semantic
  difference**: on a corrupt page `try_get_node` returns `Err` while `get_node`
  returns `None`, pinning the documented lossy behaviour.
- **Density**: 40,000 entities must fit in a fraction of the space the old
  one-page-per-entity layout needed, asserted as a compression ratio.

## Reporting a failure

If you are reporting a test or behaviour problem, include the exact command and
the raw output. See [CONTRIBUTING.md](../CONTRIBUTING.md) — this repository does
not accept external code, but a reproducible report is genuinely useful.

# Contributing to GraphLite-RS

Thanks for considering a contribution. This project has an unusual constraint
worth understanding before you start, because it affects what can be merged.

## Before you contribute: the CLA

GraphLite-RS is **dual-licensed** (AGPL-3.0 plus a commercial licence — see
[LICENSING.md](LICENSING.md)). That model only works if a single party holds
enough rights to relicense the whole work, so contributions are accepted under
the [Contributor License Agreement](CLA.md).

In practice:

- You **keep** the copyright to your work.
- You grant the project the right to license it under both AGPL and commercial
  terms.
- For your first pull request, include this line in the description:

  ```
  I have read the CLA and hereby agree to its terms.
  Signed-off-by: Your Name <you@example.com>
  ```

If you are not willing to sign, that is a reasonable position, but it means your
change can only be merged if the project drops dual licensing. Please open an
issue to discuss it rather than submitting a PR that cannot be accepted.

## Development setup

```bash
git clone https://github.com/ysankpia/graphlite
cd graphlite
cargo build --workspace
```

No services, no external database, no environment variables required. The
toolchain is stable Rust (1.98 or newer); the project has no dependencies beyond
`serde`, `serde_json`, `bincode`, `crc32fast` and `thiserror`, and new
dependencies need a strong justification (see "Architecture" below).

## The checks your change must pass

CI runs exactly these; run them locally before opening a pull request.

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

The last two are the ones people forget. `rustdoc -D warnings` rejects unescaped
angle brackets in doc comments, and clippy's lint set moves with the compiler —
if a new lint appears after a toolchain update, fix the code rather than pinning
an older compiler.

Please also run the throughput-sensitive suites in release mode, since their
assertions only mean anything when optimised:

```bash
cargo test --release --test batch_tx_tests
cargo test --release --test edge_locality_tests
```

## Architecture rules you must not break

[AGENTS.md](AGENTS.md) is the authoritative specification. The invariants that
most often trip people up:

- **DiskGraph is the single source of truth.** Never add an in-memory graph,
  table or node collection as primary state. Resident memory must stay bounded by
  the configured buffer pool, whatever the dataset size.
- **Two files only**: `{path}` and `{path}.wal`. No sidecar files, no auxiliary
  indexes on disk.
- **Fixed-size records.** `NodeRecord` is exactly 32 bytes, `EdgeRecord` exactly 64. Do not change their layout; property payloads live in slotted pages
  referenced by a 24-bit page / 8-bit slot pointer.
- **Never write `.unwrap()` or `.expect()` in library code.** Lock acquisition
  uses the poison-recovering accessors in `src/sync_ext.rs`.
- **Only one handle per database.** `open` takes an exclusive lock; a contended
  open must return `DatabaseLocked`, never proceed.
- No `todo!()` or `unimplemented!()`.

## Tests: adversarial, not happy-path

A test that only exercises the happy path is treated as incomplete. Dependent on
what you touch, cover:

- self-collision (source equals target, self-loops, an input that is also an
  output), duplicates, `N=0`, `N=1`, exactly-full buffers;
- failure atomicity: if step K fails, nothing from steps 1..K-1 survives, and the
  main database file is byte-identical;
- a second, independent way to verify any claim you make (for example, chain
  degree from walking pointers versus an independent scan of the edge id space).

Never filter a boundary case out of a generator to make a test pass. If a case
you added breaks the code, that is the test doing its job.

## Performance claims

Do **not** state a throughput number without a runnable scenario and its
measurement conditions. Add or extend a scenario under `benches/`, run it, and
quote the number together with the hardware, storage type, buffer-pool size and —
for node writes — the property payload, because removing properties roughly
doubles node throughput.

If you are correcting an existing number, say so in the pull request. The project
has had to correct unreproducible figures before, and being able to reproduce a
claim is a hard requirement here.

## Commit and pull request conventions

- Commit messages: `type(scope): summary` with a short imperative summary, e.g.
  `fix(storage): reject a second handle on the same file`.
- Explain the **root cause** in the body, not just the symptom you patched.
- Keep a pull request focused on one change. Separate refactors from behaviour
  changes so the latter can be reviewed on its own merits.
- Report any check you did not run, and why.

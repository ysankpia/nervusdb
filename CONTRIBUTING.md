# Contributing to NervusDB

**This project does not accept external code contributions.** It is
source-available and open for inspection, use and bug reports, but only the
maintainer commits code. This page explains why, and what is genuinely welcome
instead.

If you were about to open a pull request, please read the first section before
spending more time on it.

---

## Why code contributions are not accepted

NervusDB is **dual-licensed**: AGPL-3.0 for open use, plus a commercial
licence for cases AGPL does not permit (see [LICENSING.md](LICENSING.md)). That
model only holds together while a **single party owns the rights to the whole
work**.

The moment a patch from someone else is merged, that patch is theirs, licensed
to the project under AGPL terms alone. It could then no longer be included in a
commercial licence, and the dual-licence story would be broken for the entire
project — not just for that file.

The usual fix is a Contributor License Agreement. This project deliberately does
not use one, because it wants to avoid the friction and the legal surface that
comes with collecting signed agreements. The trade-off is simple: **no external
code is merged.**

This is a licensing decision, not a judgement about the quality of your work. A
patch can be excellent and still not mergeable here.

Pull requests from outside the project are closed automatically by a workflow. If
you have already opened one, that is why.

## What is welcome

### Bug reports — the most valuable thing you can send

Open an issue. A good report contains:

1. **What you ran**, verbatim (the exact command or code).
2. **What happened**, with the raw output pasted, not summarised.
3. **What you expected**, and why.
4. If you can, a **minimal reproduction** — the smallest graph and sequence that
   shows the problem.

Reports in this form get fixed. Reproducibility is the currency here: a claim
without a reproduction is a guess, and the project has had to correct
unreproducible figures before, so it is strict about this.

### Corrections to the documented numbers

The benchmarks in `benches/` print their own configuration, so any figure in the
README can be checked against them. If you can show that a documented number is
wrong, or does not hold on your hardware, that is a real contribution — open an
issue with the command you ran and the output.

### Design discussion and limitations

If you hit a limitation in [ROADMAP.md](ROADMAP.md), or disagree with a design
decision, open an issue. Knowing which constraints actually bite in practice is
useful, and it shapes what gets built next.

### Security reports

Please report privately rather than in a public issue — email
**luhuizhx@gmail.com** with the details and a reproduction.

## If you want to change the code yourself

The AGPL gives you the right to modify and run this software. You do not need
permission for that, and you do not need to send anything back unless you operate
a modified version as a network service (AGPL §13).

Practically:

- **Keep it private / internal** — no obligation to publish anything.
- **Modify it and run it only for yourself** — no obligation.
- **Modify it and offer it as a network service** — you must offer your users the
  source of your modified version.
- **Distribute it, or embed it in something closed-source** — this is what the
  commercial licence is for; see [LICENSING.md](LICENSING.md).

A personal fork for your own experiments is fine and expected. It just does not
flow back into this repository.

## If the maintainer invites a specific change

Rare, but possible: the maintainer may ask you to prepare a specific patch, or
reopen a pull request to review and merge it. In that case the
[CLA](CLA.md) applies and must be agreed to before the change is merged.

Outside that invitation, please use an issue rather than a pull request.

---

## For reference: the checks this repository enforces

Useful if you are submitting an invited change, or if you simply want your own
fork to match. CI runs exactly these:

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

Also run the throughput-sensitive suites in release mode, since their assertions
only mean anything when optimised:

```bash
cargo test --release --test batch_tx_tests
cargo test --release --test edge_locality_tests
```

## Architecture rules

[AGENTS.md](AGENTS.md) is the authoritative specification for anyone working in
this codebase. The invariants that matter most:

- **DiskGraph is the single source of truth.** Never add an in-memory graph,
  table or node collection as primary state. Resident memory must stay bounded by
  the configured buffer pool, whatever the dataset size.
- **Two files only**: `{path}` and `{path}.wal`. No sidecar files.
- **Fixed-size records.** `NodeRecord` is exactly 32 bytes, `EdgeRecord` exactly 64. Property payloads live in slotted pages referenced by a 24-bit page /
  8-bit slot pointer.
- **Never write `.unwrap()` or `.expect()` in library code.** Lock acquisition
  uses the poison-recovering accessors in `src/sync_ext.rs`.
- **Only one handle per database.** A contended `open` returns
  `DatabaseLocked`, never proceeds.
- No `todo!()` or `unimplemented!()`.

## Tests and performance claims

If you are preparing an invited change, two house rules apply.

**Tests must be adversarial, not happy-path.** Cover self-collision, duplicates,
`N=0`, `N=1`, exactly-full buffers; verify failure atomicity (if step K fails,
nothing from steps 1..K-1 survives and the main file is byte-identical); and
verify any claim a second, independent way. Never filter a boundary case out of a
generator to make a test pass.

**Never state a throughput number without a runnable scenario and its
conditions.** Add or extend a scenario under `benches/`, run it, and quote the
number with the hardware, storage, buffer-pool size and — for node writes — the
property payload, because removing properties roughly doubles node throughput.

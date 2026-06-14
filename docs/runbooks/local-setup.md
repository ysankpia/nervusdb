# Local Setup

## Prerequisites

- Rust toolchain (stable). Install via [rustup](https://rustup.rs/):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

- Git configuration:
  ```bash
  git config --global user.name "Your Name"
  git config --global user.email "your.email@example.com"
  ```

## Clone And Build

```bash
git clone git@github.com:ysankpia/nervusdb.git
cd nervusdb
cargo build --workspace
```

## First Validation

```bash
bash scripts/check.sh
```

This runs formatting, core-crate clippy, and the Mini-Cypher quick test. It
should complete within minutes on a modern machine.

## IDE Setup

The workspace uses `rustfmt` and `clippy` as the primary formatting and lint
tools. There is no mandatory IDE. VS Code with `rust-analyzer` is a common
choice. Configuration lives in `.vscode/`.

## Git Hooks

Optional fast pre-commit hooks:

```bash
make install-hooks
```

This installs `scripts/git-hooks/pre-commit` and `scripts/git-hooks/pre-push`,
both of which run `bash scripts/check.sh`.

## Directory Layout

```
nervusdb/           — root workspace
├── nervusdb/       — public Rust facade
├── nervusdb-api/   — graph traits
├── nervusdb-storage/ — page store, WAL, recovery
├── nervusdb-query/ — Mini-Cypher parser/planner/executor
├── nervusdb-cli/   — local CLI tool
├── nervusdb-pyo3/  — Python bindings (experimental)
├── nervusdb-capi/  — C API (experimental)
├── nervusdb-node/  — Node.js bindings (experimental)
├── examples/       — 0.1 core examples
├── examples-test/  — binding capability tests (experimental)
├── scripts/        — validation scripts
├── docs/           — documentation
└── target/         — build output (gitignored)
```

## Further Reading

- Local validation runbook: `docs/runbooks/local-validation.md`
- Validation policy: `docs/engineering/validation-policy.md`
- Quick start guide: `README.md`

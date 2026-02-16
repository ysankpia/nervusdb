# Publishing Guide (v2)

This repository maintains v2 Rust crates, CLI, and bindings (Python + Node.js).

## GitHub Release (Recommended)

1. Update `CHANGELOG.md`
2. Tag and push

```bash
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin main vX.Y.Z
```

3. Create a GitHub Release (optional)

## crates.io (Optional)

Dry-run before publishing:

```bash
cargo publish -p nervusdb-api --dry-run
cargo publish -p nervusdb-storage --dry-run
cargo publish -p nervusdb-query --dry-run
cargo publish -p nervusdb --dry-run
cargo publish -p nervusdb-cli --dry-run
```

Publish order matters due to inter-crate dependencies:
`nervusdb-api` -> `nervusdb-storage` -> `nervusdb-query` -> `nervusdb` -> `nervusdb-cli`.

## Python (PyPI)

```bash
maturin build -m nervusdb-pyo3/Cargo.toml --release
maturin publish -m nervusdb-pyo3/Cargo.toml
```

## Node.js (npm)

```bash
cargo build --manifest-path nervusdb-node/Cargo.toml --release
cd nervusdb-node && npm publish
```

## Pre-Release Checklist

- [ ] All CI gates green (fmt, clippy, tests, bindings)
- [ ] TCK pass rate meets threshold
- [ ] CHANGELOG.md updated
- [ ] Version bumped in all Cargo.toml files

# Publishing Guide (v2)

This repository maintains v2 Rust crates, CLI, and bindings (Python + Node.js).

## Beta Release Gate Summary

Beta release is allowed only when all three gate families are green at the same time:

- TCK Tier-3 full pass rate >= 95%
- 7 consecutive days of stable CI + nightly
- 7 consecutive days of perf SLO nightly passing

Current documented trunk status:

- TCK Tier-3: **100%** (3 897 / 3 897)
- Stability window: **7 / 7** passed
- Perf SLO window: **7 / 7** passed

Use the template in [docs/beta-daily-template.md](beta-daily-template.md) for daily or release-readiness updates.

## GitHub Release (Recommended)

1. Update `CHANGELOG.md`
2. Confirm the latest daily report is attached or linked
3. Tag and push

```bash
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin main vX.Y.Z
```

4. Create a GitHub Release (optional)

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
- [ ] TCK Tier-3 full pass rate meets threshold (>= 95%)
- [ ] Stability window report is 7 / 7
- [ ] Perf SLO window report is 7 / 7
- [ ] Latest daily status report updated from the Beta daily template
- [ ] CHANGELOG.md updated
- [ ] Version bumped in all Cargo.toml files

# Releasing

Publishing is irreversible: a crate version can never be overwritten, a released
version cannot be deleted, and a name that has been published cannot be cleanly
retracted. The workflow is built around that one fact.

`.github/workflows/release.yml` has four jobs:

| Job                | Trigger                                     | What it does                                                                                              |
| ------------------ | ------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `verify`           | tag push or dispatch                        | re-runs the full gate on the tagged commit; checks the tag matches the manifest; `cargo package --locked` |
| `build-binaries`   | tag push or dispatch                        | builds the four platform targets (artifacts only)                                                         |
| `publish-binaries` | `workflow_dispatch` with `confirm: publish` | uploads all wheels and the npm package                                                                    |
| `publish-crate`    | `workflow_dispatch` with `confirm: publish` | `cargo publish`                                                                                           |

**A tag alone never publishes.** Pushing `v0.1.0` runs `verify` and `build-binaries`,
which produce nothing outward-facing. Publishing needs an explicit dispatch:

```bash
gh workflow run release.yml --repo ysankpia/nervusdb \
  -f tag=v0.1.0 -f confirm=publish
```

`verify` re-runs the gate rather than trusting CI, because a tag can point at a commit
that never passed — and this is the last checkpoint before a version is permanent.

## Why the jobs are split this way

`cargo publish` is its own job on its own runner. crates.io accepts exactly **one**
upload per version and rejects a second attempt permanently, so running it inside the
four-way platform matrix would upload successfully from whichever job ran first and
fail the other three.

`build-binaries` uses four **native** runners rather than cross-compiling. These
bindings link against the platform C runtime, and a binary built for the wrong libc
installs cleanly and then fails at import — on a user's machine, not in CI.

Platform matrix:

| Runner             | Target                      | npm artifact      |
| ------------------ | --------------------------- | ----------------- |
| `macos-14`         | `aarch64-apple-darwin`      | `darwin-arm64`    |
| `macos-15-intel`   | `x86_64-apple-darwin`       | `darwin-x64`      |
| `ubuntu-latest`    | `x86_64-unknown-linux-gnu`  | `linux-x64-gnu`   |
| `ubuntu-24.04-arm` | `aarch64-unknown-linux-gnu` | `linux-arm64-gnu` |

`macos-13` is **not** offered for public repositories; the Intel macOS runners are
`macos-15-intel` and `macos-26-intel`.

`abi3-py38` is set in `bindings/python/Cargo.toml`, so one wheel per platform covers
Python 3.8+. That is why the matrix is platforms only, not platforms × Python
versions.

## What the npm package contains

All four `.node` binaries ship in **one** package. Each is ~1.3 MB stripped, so ~5 MB
unpacked — against 26 MB for `better-sqlite3`, the de-facto standard for this. One
package also avoids the version-drift trap of the main-package-plus-subpackages
layout, where five packages must stay in lockstep.

The `files` field is `["index.js", "index.d.ts", "README.md", "*.node"]`. The `*.node`
glob matters: listing only `nervusdb.node` silently produces a tarball **with no
binary**, because `napi build --platform` names them `nervusdb.<triple>.node`.

Binaries are stripped before upload (`strip -x`). Verify after stripping that the
module still loads — stripping the napi exports would produce a package that installs
and then throws on `require`.

## Prerequisites

Both exist in the repository already except the secrets:

| Requirement                  | State                                                    |
| ---------------------------- | -------------------------------------------------------- |
| `release` GitHub Environment | exists; **add required reviewers** or it is only a label |
| `CARGO_REGISTRY_TOKEN`       | **not set**                                              |
| `PYPI_API_TOKEN`             | **not set**                                              |
| `NPM_TOKEN`                  | **not set**                                              |

Check with `gh secret list --repo ysankpia/nervusdb` and
`gh api repos/ysankpia/nervusdb/environments`.

## Registry names

All three carry the same name, `nervusdb`:

| Registry  | Status                                                             |
| --------- | ------------------------------------------------------------------ |
| crates.io | owned by this project; older unrelated `0.0.x` releases are yanked |
| PyPI      | free at the time of writing                                        |
| npm       | free at the time of writing                                        |

The Python _distribution_ name and the _import_ name are separate concepts: the
package is `nervusdb` and it is imported as `import nervusdb`. They happen to match
now; they need not, as with `beautifulsoup4` → `import bs4`.

`tests/zero_dependency_tests.rs` rejects any manifest that would publish under a name
known to belong to another project, naming the owner in the message.

## Before the first release

1. Run the full gate locally ([AGENTS.md](../AGENTS.md) §3).
2. Run the real-dataset acceptance and compare the red lines
   ([testing.md](testing.md) lists them; [benchmarks.md](benchmarks.md) explains why
   the exact totals, not the rates, are the check).
3. Confirm every document states the version being released — the guards in
   `zero_dependency_tests.rs` cover the test counts, the manifest versions, and the
   format version.

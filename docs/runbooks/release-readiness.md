# Release Readiness Runbook

This is for current 0.x / 0.1-core readiness only. It is not a revival of the old
platform release window.

Per ADR 0006, the public release artifact is one crate: `nervusdb`.

## Required Evidence

- `bash scripts/check.sh` passes.
- `bash scripts/core_smoke.sh` passes.
- `bash scripts/core_examples.sh` passes.
- Crash recovery evidence exists for the current storage model.
- `docs/reference/mini-cypher.md` matches the core acceptance tests.
- Ten realistic examples are documented in `docs/reference/examples-0.1.md`.
- Storage format and compatibility expectations are documented.
- Manual benchmark evidence exists for the chosen release candidate.
- Fsck / freeze smoke evidence exists when preparing `v0.0.5` or later.
- `cargo publish -p nervusdb --dry-run` passes without requiring users to depend
  on internal implementation crates.

## Not Required By Default

- Full openCypher TCK pass rate.
- Binding parity gates.
- Vector or HNSW benchmarks.
- Scheduled chaos, soak, fuzz, perf, or stability windows.

Those checks may be useful for targeted changes, but they are not release
blockers for the embedded graph 0.1 line unless a future ADR changes that rule.

## Publish Shape

Do not publish `nervusdb-api`, `nervusdb-storage`, or `nervusdb-query` as public
crates. They are internal engineering boundaries unless a future ADR gives
one of them a real external audience.

Expected user install:

```toml
[dependencies]
nervusdb = "0.0.7"
```

## Release Procedure

Use this procedure for normal 0.x releases. The tag must point at the release
preparation commit, not at the later progress-record commit.

### 1. Prepare The Release Commit

Update the public package version and local wrapper versions together:

```text
nervusdb/Cargo.toml
nervusdb-cli/Cargo.toml
nervusdb-api/Cargo.toml
nervusdb-storage/Cargo.toml
nervusdb-query/Cargo.toml
Cargo.lock
```

Only `nervusdb` is published, but the wrapper crates still need matching local
versions so workspace checks do not drift.

Add or update:

```text
docs/releases/vX.Y.Z.md
PROGRESS.md
docs/roadmap.md
docs/index.md
any active/completed plan state touched by the release
```

For `Cargo.lock`, avoid unrelated dependency-resolution churn. The expected
lockfile diff is only the workspace package version bump unless a dependency
was intentionally changed.

Create the release-prep commit:

```bash
git add Cargo.lock PROGRESS.md docs nervusdb*/Cargo.toml nervusdb/src
git diff --cached --check
git commit -m "chore(release): prepare vX.Y.Z"
```

### 2. Local Validation

Run the normal release evidence before pushing:

```bash
cargo fmt --all -- --check
cargo check -p nervusdb --examples
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --test core_0_1_mini_cypher
cargo test -p nervusdb-cli
bash scripts/check.sh
bash scripts/core_examples.sh
bash scripts/core_crash_recovery.sh
cargo test --workspace
```

Run targeted benchmark validation when the release changes performance:

```bash
bash scripts/cross_db_bench.sh --system nervusdb --medium
```

or for the core benchmark:

```bash
bash scripts/core_bench.sh --small
```

### 3. Publish Dry-Run

Run a clean dry-run after committing the release-prep commit:

```bash
cargo publish -p nervusdb --dry-run --registry crates-io
```

Important behavior:

- `cargo publish --dry-run` packages `nervusdb`, extracts it under
  `target/package/nervusdb-X.Y.Z`, and compiles that package as if it came from
  crates.io.
- This can take noticeably longer than `cargo test` because it does not simply
  reuse the workspace test compile shape.
- Warnings that local wrapper crate patches were not used are expected. The
  wrapper crates are `publish = false` and are not dependencies of the public
  package.
- If the working tree is dirty, Cargo refuses the dry-run. Commit first; do not
  treat `--allow-dirty` as release evidence.

If a dry-run is interrupted, clean up leftover compile processes before
restarting:

```bash
ps -eo pid,ppid,command | rg 'cargo publish|target/package/nervusdb|rustc' || true
pkill -f 'cargo publish -p nervusdb' 2>/dev/null || true
```

If orphan `rustc` processes remain with parent pid `1`, kill only those specific
PIDs after checking the command line. Do not kill unrelated user builds.

### 4. Push Main And Wait For CI

Push the release-prep commit:

```bash
git push origin main
```

Find and watch the GitHub Actions run for that commit:

```bash
gh run list --branch main --limit 5 \
  --json databaseId,headSha,status,conclusion,displayTitle,url

gh run watch <run-id> --exit-status
```

Do not tag until the release-prep commit has a successful CI run.

### 5. Tag And GitHub Release

Create and push an annotated tag:

```bash
git tag -a vX.Y.Z -m "NervusDB vX.Y.Z"
git push origin vX.Y.Z
```

Create the GitHub release from the release notes:

```bash
gh release create vX.Y.Z \
  --verify-tag \
  --title "NervusDB vX.Y.Z" \
  --notes-file docs/releases/vX.Y.Z.md \
  --latest=true
```

Verify it:

```bash
gh release view vX.Y.Z \
  --json tagName,name,url,publishedAt,isDraft,isPrerelease,targetCommitish
```

### 6. Publish To Crates.io

Publish the single public crate:

```bash
cargo publish -p nervusdb --registry crates-io
```

This also verifies the packaged crate before upload, so it may compile again and
take time. Let it finish. Do not interrupt it unless it is clearly stuck on a
network error.

Confirm crates.io sees the version:

```bash
cargo search nervusdb --limit 5 --registry crates-io
```

The expected first line should show:

```text
nervusdb = "X.Y.Z"
```

### 7. Record Publication

After tag, GitHub release, and crates.io publish are complete, update
`PROGRESS.md` with:

```text
- tag
- GitHub release URL
- crates.io URL
- CI run id
- cargo search confirmation
```

Commit and push that record separately:

```bash
git add PROGRESS.md
git commit -m "docs(progress): record vX.Y.Z publication"
git push origin main
```

It is normal for this follow-up commit to trigger another CI run. The release tag
must remain on the release-prep commit.

## Cargo Registry And Network Notes

Cargo may be configured globally or locally to replace crates.io with a mirror.
If publishing fails while downloading `config.json` or an index, first check
local and global Cargo config:

```bash
find .cargo "$HOME/.cargo" -maxdepth 2 -type f \
  \( -name 'config' -o -name 'config.toml' -o -name 'credentials' -o -name 'credentials.toml' \) \
  -print
```

If a mirror or VPN/proxy is broken, fix the global network/Cargo configuration
before publishing. Do not edit project files just to work around a temporary
registry outage.

For dry-run only, a temporary `CARGO_HOME` can bypass global mirror config, but
that path will rebuild more dependencies from an empty cache and can be slow:

```bash
tmp_home="$(mktemp -d)"
CARGO_HOME="$tmp_home" cargo publish -p nervusdb --dry-run --registry crates-io
rm -rf "$tmp_home"
```

Use this only to diagnose Cargo configuration problems. The normal release path
should use the user's configured Cargo environment.

# Pull request

> **External contributions are refused by policy.** If you are not the
> maintainer, a member or a collaborator, this pull request will be closed
> automatically by CI. That is a licensing decision (dual AGPL + commercial),
> not a judgement on your work — please open an issue with a reproduction
> instead. See CONTRIBUTING.md.

## What and why

<!-- One or two sentences on what changes and the problem it solves. -->

Closes #

## Root cause

<!--
Required for a fix. Explain what actually allowed the bug, not just where you
patched it. If you tested a hypothesis and it turned out wrong, mention that —
it saves the next person the same detour.
-->

## Storage or behavioural impact

<!--
Delete whichever do not apply.

- [ ] Storage format changed (`DB_PAGE_VERSION` bump) — migration is dump/re-import
- [ ] Behaviour changed for an existing query or API
- [ ] Public API added or changed
- [ ] None of the above
-->

If any box above is ticked, describe the impact and update `CHANGELOG.md`.

## Verification

<!-- Paste the actual output, do not summarise it. -->

```
$ cargo fmt --all -- --check
$ cargo check --workspace --all-targets
$ cargo clippy --workspace --all-targets -- -D warnings
$ cargo test --workspace
$ RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

```
$ cargo test --release --test batch_tx_tests
$ cargo test --release --test edge_locality_tests
```

## Checklist

- [ ] Full CI gate from AGENTS.md §3.1 run locally, output pasted above
- [ ] Release-mode throughput suites run
- [ ] `CHANGELOG.md` updated under `[Unreleased]` (if user-visible)
- [ ] Docs updated in the same change (README / AGENTS / ROADMAP as applicable)
- [ ] New tests cover the edges, not just the happy path
- [ ] No `unwrap()` / `expect()` introduced in library code
- [ ] Any performance number quoted comes with its measurement conditions
- [ ] Nothing was verified by weakening or deleting an existing assertion

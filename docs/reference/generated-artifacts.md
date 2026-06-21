# Generated Artifacts

## Build Artifacts

| Artifact | Location | Regeneration | Status |
|---|---|---|---|
| Rust build output | `target/` | `cargo build` | Gitignored |
| Cargo lock | `Cargo.lock` | `cargo generate-lockfile` | Tracked in git |
| TypeScript build info | `*.tsbuildinfo` | `tsc` if JS tooling is restored | Gitignored |

## Runtime / Test Artifacts

| Artifact | Location | Regeneration | Status |
|---|---|---|---|
| Database directories | caller-selected temp dirs such as `/tmp/nervusdb-demo` | Application creates on `Db::open` | Use temp dirs for tests |
| Legacy database files | `*.synapsedb`, `*.nervusdb`, `*.redb` | Historical test runs | Gitignored |
| Memory snapshots | `memory-snapshots/` | Heap profiling | Gitignored |
| TCK logs | `/tck_*.log`, `/tck_*.txt`, `/tck_results.*` | Manual TCK runs | Gitignored |

## Documentation / Generated Output

| Artifact | Location | Regeneration | Status |
|---|---|---|---|
| Rustdoc | `target/doc/` | `cargo doc` | Not tracked; docs live in `docs/` |
| Repomix output | `/repomix-output.*` | Manual repomix runs | Gitignored |

## Agent / Task Runner State

| Artifact | Location | Regeneration | Status |
|---|---|---|---|
| Agent state | `.agents/`, `.beads/`, `.claude/` | Agent tooling | Gitignored |

## Policy

- All generated artifacts must be in `.gitignore` or explicitly documented here.
- Regeneration commands must be documented when the artifact affects build,
  validation, or deployment.
- Do not commit generated artifacts to the repository unless they are required
  by the build system (e.g. `Cargo.lock`).
- If a regeneration command changes, update this document and the relevant
  runbook or engineering doc.

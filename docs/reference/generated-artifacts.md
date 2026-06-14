# Generated Artifacts

## Build Artifacts

| Artifact | Location | Regeneration | Status |
|---|---|---|---|
| Rust build output | `target/` | `cargo build` | Gitignored |
| Cargo lock | `Cargo.lock` | `cargo generate-lockfile` | Tracked in git |
| npm modules | `nervusdb-node/node_modules/` | `npm install` | Gitignored |
| npm dist | `nervusdb-node/dist/` | `npm run build` | Gitignored |
| TypeScript build info | `*.tsbuildinfo` | `tsc` | Gitignored |

## Runtime / Test Artifacts

| Artifact | Location | Regeneration | Status |
|---|---|---|---|
| Database files | `*.ndb`, `*.wal` | Application creates on `Db::open` | Gitignored (patterns in `.gitignore`) |
| Database files | `*.synapsedb`, `*.nervusdb`, `*.redb` | Test runs | Gitignored |
| Test coverage | `bindings/node/coverage/`, `.nyc_output/` | `npm test` | Gitignored |
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

# NervusDB v2

**Rust-native, crash-safe embedded property graph database.**

> Current mode: **SQLite-Beta convergence** (`TCK>=95% -> 7-day stability -> SLO gates`).
> Feature claims are gated by CI/TCK evidence, not intent.

[![CI](https://github.com/LuQing-Studio/nervusdb/actions/workflows/ci.yml/badge.svg)](https://github.com/LuQing-Studio/nervusdb/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

## Quick Start

### CLI

```bash
# write
cargo run -p nervusdb-cli -- v2 write --db /tmp/demo --cypher "CREATE (a {name: 'Alice'})-[:1]->(b {name: 'Bob'})"

# query (NDJSON)
cargo run -p nervusdb-cli -- v2 query --db /tmp/demo --cypher "MATCH (a)-[:1]->(b) RETURN a, b LIMIT 10"
```

### Rust

```rust
use nervusdb::Db;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Db::open("/tmp/demo")?;
    db.execute("CREATE (n:Person {name: 'Alice'})", None)?;
    let rows = db.query("MATCH (n:Person) RETURN n", None)?;
    println!("rows={}", rows.len());
    Ok(())
}
```

### Python (PyO3 binding, local develop mode)

```bash
pip install maturin
maturin develop -m nervusdb-pyo3/Cargo.toml

python - <<'PY'
import nervusdb

db = nervusdb.open('/tmp/demo-py')
db.execute_write("CREATE (n:Person {name: 'Alice'})")
for row in db.query_stream("MATCH (n:Person) RETURN n LIMIT 1"):
    print(row)
db.close()
PY
```

> Note: write statements (for example `CREATE/MERGE/DELETE/SET`) must go through
> `execute_write(...)` or a write transaction. Running them with `query(...)`
> raises `ExecutionError`.

### Node (N-API binding)

```bash
cargo build --manifest-path nervusdb-node/Cargo.toml --release
npm --prefix examples/ts-local ci
npm --prefix examples/ts-local run smoke
```

## What Is Considered “Supported”

- Contract source: `docs/reference/cypher_support.md`
- Task source: `docs/tasks.md`
- Roadmap source: `docs/ROADMAP_2.0.md`
- Done criteria: `docs/memos/DONE.md`

If CI/TCK gate does not pass, the feature is considered **not supported**.

## Tiered TCK & Beta Gates

```bash
make tck-tier0   # smoke
make tck-tier1   # clauses whitelist
make tck-tier2   # expressions whitelist
make tck-tier3   # full run (typically nightly)

# tier3 pass-rate report + beta threshold gate
TCK_FULL_LOG_FILE=tck_latest.log bash scripts/tck_full_rate.sh
TCK_MIN_PASS_RATE=95 bash scripts/beta_gate.sh
```

Nightly artifacts are published from `.github/workflows/tck-nightly.yml` to `artifacts/tck/` (`tier3-full.log` + `tier3-cluster.md`).

Current Beta line also requires `artifacts/tck/tier3-rate.json` pass-rate evidence.

## Bindings Status

- Python: `nervusdb-pyo3` (PyO3) with typed objects (`Node/Relationship/Path`) and typed exceptions
- Node: `nervusdb-node` N-API binding（build + runtime smoke + contract smoke）

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -W warnings
bash scripts/workspace_quick_test.sh
bash scripts/binding_smoke.sh
bash scripts/contract_smoke.sh
```

## License

[Apache-2.0](LICENSE)

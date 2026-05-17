# NervusDB 0.1 Examples

These examples are executable 0.1 evidence. They use only the public CLI and
the Mini-Cypher surface documented in `docs/reference/mini-cypher.md`.

Run all examples:

```bash
bash scripts/core_examples.sh
```

Run one example:

```bash
bash scripts/core_examples.sh 01-social
```

The script creates a fresh temporary database per example, runs each
`write-*.cypher` file through `v2 write --file`, runs `query.cypher` through
`v2 query --file`, and compares the NDJSON output with `expected.ndjson`.

## Example Map

| Story | Example | Graph shape | Query proof |
|---|---|---|---|
| Social graph | `examples/core-0.1/01-social` | `Person -KNOWS-> Person` | Alice's outgoing neighbor is Bob. |
| Dependency graph | `examples/core-0.1/02-dependency` | `Package -DEPENDS_ON-> Package` | `app` depends on `serde`. |
| File / module graph | `examples/core-0.1/03-file-module` | `File -IMPORTS-> File` | `src/main.rs` imports `src/db.rs`. |
| Local knowledge graph | `examples/core-0.1/04-knowledge` | `Note -LINKS_TO-> Note` | `Graph Storage` links to `WAL Recovery`. |
| Parent / child hierarchy | `examples/core-0.1/05-hierarchy` | `TreeNode -PARENT_OF-> TreeNode` | `root` returns `left` as a child. |
| Tag graph | `examples/core-0.1/06-tags` | `Item -TAGGED_AS-> Tag` | `work` returns `Notebook`. |
| Ownership graph | `examples/core-0.1/07-ownership` | `Owner -OWNS-> Asset` | `Team A` owns `db-file`. |
| Package relationship graph | `examples/core-0.1/08-crates` | `Crate -USES-> Crate` | `nervusdb-cli` uses `nervusdb`. |
| Recommendation seed | `examples/core-0.1/09-recommendation` | `User -LIKES-> Item -IN_CATEGORY-> Category` | `Ada` reaches `Databases` by two hops. |
| Import then query smoke | `examples/core-0.1/10-import-then-query` | `Service -CALLS-> Service` | File-driven writes load service calls and reopen cleanly. |

## Boundaries

These fixtures intentionally avoid full Cypher features, broad ETL behavior,
procedures, aggregation, variable-length paths, and binding SDKs. Their job is
to prove that local file-backed graph writes and one-hop or two-hop reads work
through the 0.1 CLI path.

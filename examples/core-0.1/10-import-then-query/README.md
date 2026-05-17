# 10 Import Then Query Smoke

Graph shape: file-driven `Service -CALLS-> Service`.

This example simulates a small import by loading two write files through
`v2 write --file`, then queries a representative relationship twice to prove
the local load is readable after reopening the database.

```bash
bash scripts/core_examples.sh 10-import-then-query
```

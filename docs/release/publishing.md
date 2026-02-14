# 发布指南（v2 / Rust-only）

这仓库现在只维护 v2 Rust crates + CLI。绑定与 v1 已归档到 `_legacy_v1_archive/`，不要照着旧发布文档瞎折腾。

## GitHub Release（推荐）

1. 更新 `CHANGELOG.md`（只写 v2）
2. 打 tag 并 push
3. 创建 GitHub Release（可选）

```bash
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin main vX.Y.Z
```

## crates.io（可选）

如果你要发布 crates（例如 `nervusdb` / `nervusdb-query` / `nervusdb-cli`），先在本地 dry-run：

```bash
cargo publish -p nervusdb --dry-run
cargo publish -p nervusdb-query --dry-run
cargo publish -p nervusdb-cli --dry-run
```

历史的 npm/PyPI/UniFFI 发布流程（v1 时代）见 `_legacy_v1_archive/docs/release/publishing-v1.md`。

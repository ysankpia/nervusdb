# 变更日志（v2）

本文件只记录 **v2（`.ndb + .wal`）** 的可验证变更。

v1/redb 与旧绑定的历史记录见 `_legacy_v1_archive/CHANGELOG_v1.md`。

## 未发布

- 代码质量：修复 Clippy 警告（`collapsible_if`、`type_complexity`、`too_many_arguments`）。
- v1 全量退役：从 workspace/CI 移除并归档到 `_legacy_v1_archive/`（仓库事实：现在只维护 v2 Rust crates + CLI）。
- v2 查询执行修正：`MATCH (a)-[:<u32>*min..max]->(b)` 在 `prepare()` 路径真正下发为 `MatchOutVarLen`，并对缺省 `*` 施加默认 hop 上限（避免无限遍历）。
- 文档去谎言化：`README.md` / `docs/spec.md` / `docs/reference/cypher_support.md` 对齐 v2 现状与边界。

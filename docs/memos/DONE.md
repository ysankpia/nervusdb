# NervusDB 完成标准（Definition of Done）

你卡住的根因不是技术，而是没有“终点线”。这个文件就是终点线：满足这些条目就算完成，不再无限扩展。

## 1) MVP 是什么？

**MVP = v2（`.ndb + .wal`，Rust-first）**：`nervusdb-v2-storage` + `nervusdb-v2-query` + `nervusdb-cli` 跑通最小用户路径，并通过 crash gate。

现状与边界以：

- `docs/spec.md`
- `docs/reference/cypher_support.md`

为准。

## 2) 完成标准（满足即停止）

当以下条目全部满足，就认为项目“完成”，后续不再新增功能（只允许必要的文档修正）。

### 2.1 用户路径（5 分钟上手）

- [ ] `README.md` 顶部示例：复制粘贴可运行（建议用 CLI）
- [ ] `docs/reference/cypher_support.md` 与实际实现一致（不吹牛、不写反）
- [ ] 至少一个可运行示例（建议 CLI），并在 README 链接到它

### 2.2 工程门禁（不靠运气）

- [ ] CI 全绿（主工作流）
- [ ] crash-gate（以 CI 配置为准）可通过

### 2.3 范围冻结（最关键）

- [ ] 明确写死 v2 的当前阶段边界（M3/alpha）：超出范围必须 fail-fast

## 3) 不做什么（写死，防止复发）

- 不再追求“完整 Cypher/兼容 Neo4j 全量语法”
- 不再引入新存储格式/新 API 破坏性重构
- 不再为了“看起来干净”反复大改范围；已经归档的 v1 代码只保留历史，不再维护

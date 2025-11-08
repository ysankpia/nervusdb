# NervusDB

嵌入式三元组数据库，定位在本地/边缘环境下的知识管理、链式联想与轻量推理。核心以 TypeScript 实现，默认提供 QueryBuilder、Cypher、GraphQL、Gremlin 多语言入口，并可启用 Rust Native 后端以获得更高性能。

## 当前状态

- 最新 npm 版本：`@nervusdb/core@0.1.4`（2025-11-08 发布，包含 Temporal Memory 与 Native Core 增强）
- Node.js ≥ 18，推荐 20/22；依赖 `pnpm` 作为包管理器
- CI 步骤：`pnpm typecheck && pnpm lint && pnpm test && pnpm build`
- 架构与技术决策全部迁移至 `docs/architecture/`，其余文档目录已清理

## 安装

```bash
# 首选
pnpm add @nervusdb/core

# 其他
npm install @nervusdb/core
bun add @nervusdb/core
```

可选：在仓库内执行 `pnpm install && pnpm build`，再使用 `npm i -g .` 安装 CLI（生成 `nervusdb` 命令）。

## 快速上手

```ts
import { NervusDB } from '@nervusdb/core';

const db = await NervusDB.open('demo.nervusdb', {
  temporal: true,
  enableLock: true,
});

await db.insertFact({
  subject: 'alice',
  predicate: 'knows',
  object: 'bob',
  properties: { since: 2021 },
});

const result = await db
  .query()
  .anchor('alice')
  .out('knows')
  .withProperty('since', (v) => v >= 2020)
  .all();

console.log(result);
await db.close();
```

CLI 示例：

```bash
nervusdb bench demo.nervusdb 200 lsm
nervusdb stats demo.nervusdb
```

## 核心特性

- 六序索引 + WAL v2，支持快照查询和批量事务
- Temporal Memory：默认开启时间线存储与抽取管线
- QueryBuilder + Cypher/GraphQL/Gremlin 解析，共享执行计划
- 属性/全文/空间索引与图算法插件，可通过 `src/plugins/*` 扩展
- CLI 工具链覆盖统计、压实、修复、快照治理等日常运维场景

## 架构文档

所有 ADR、架构和质量决策均收敛到 `docs/architecture/`。每份 ADR 记录编号、背景、决策、影响与跟进，可直接阅读对应 Markdown 文件。

## 开发与测试

```bash
pnpm install
pnpm typecheck
pnpm lint
pnpm test           # 完整 vitest 套件
pnpm build          # 产出 dist/
pnpm bench:baseline # 核心基准（需构建完成）
```

性能/系统测试可能耗时较长，可用以下命令跳过重型用例：

```bash
pnpm test -- --exclude "**/incremental_flush_performance.test.ts" \
             --exclude "**/lazy_loading_performance.test.ts" \
             --exclude "**/disk_centric_performance.test.ts"
```

## 发布流程（npm）

1. `pnpm version <patch|minor|major> --no-git-tag-version`
2. `git commit -am "chore: bump version to X.Y.Z" && git tag vX.Y.Z`
3. `pnpm typecheck && pnpm lint && pnpm test && pnpm build`
4. `npm pack --dry-run` 检查 tarball，仅包含 dist + README + LICENSE
5. `npm publish --access public`
6. `git push origin main --tags`

## 贡献

- Issue/PR 仍遵循 GitHub 流程；所有代码必须关联 Milestone 或 Backlog Issue
- pre-commit 与 pre-push 已启用 husky，禁止跳过
- 讨论与架构升级请在 `docs/architecture/` 创建或更新 ADR

## 许可证

[Apache-2.0](LICENSE)

# 项目规则（统一版）

本文件为 SynapseDB 仓库的统一“项目规则”与“协作说明”，整合原有的 `CLAUDE.md` 与 `AGENTS.md`，面向所有参与本仓库协作的开发者与智能体（包括不同供应商的代码助手）。除非特殊说明，本文件优先级高于历史分散文档；若与源码实现不一致，以源码与测试为准，并在后续修订本文件。

- 语言与注释：全仓库统一使用中文（代码注释、提交信息与文档）。
- 读者对象：人类开发者与智能体（AI 代码助手）。
- 范围：代码协作规则、智能体行为规范、构建测试流程、架构与 API 摘要、外部工具（MCP）调用规范、提交与安全要求。

---

## 1. 角色与协作方式（统一给智能体）

- 角色定位（四位一体）：
  - 技术架构师：从系统视角把握整体架构与演进路径。
  - 全栈专家：覆盖前端/后端/数据库/运维与自动化脚本。
  - 技术导师：不仅“给答案”，更“教方法”和最佳实践。
  - 技术伙伴：与开发者协同推进，尊重现有约束与风格。

- 思维与表达：
  - 系统性分析 → 明确目标、输入、约束、交付物。
  - 前瞻性评估 → 可扩展性、维护性与风险点标注。
  - 只用中文回答与注释；命名与术语中文优先。
  - 给出多方案对比与推荐理由；解释原理与常见陷阱。

- 互动与过程：
  - 先计划后执行：复杂任务先产出 6~10 步的可执行计划。
  - 分阶段交付：每阶段产出可验证结果；必要时请求澄清。
  - 变更最小：在现有代码风格内实现；避免无关改动与过度设计。
  - 授人以渔：附带思路、迁移建议与扩展思考。

---

## 2. 外部工具（MCP）调用规范

为保证可追溯与一致输出，不同智能体在调用外部服务时需遵循：

- 单轮单工具：一次对话回合最多调用 1 种外部服务；确需多种时串行，并说明理由与预期产出。
- 最小必要：收敛关键词/结果数/时间窗；避免噪声与过度抓取。
- 可追溯性：答复末尾追加“工具调用简报”（工具名、输入摘要、参数、时间与来源）。
- 离线优先：可离线完成则不外呼；外呼遵守 robots/ToS 与隐私要求。
- 失败回退：首选失败→替代服务→无法外呼给出保守答案并标注不确定性。

工具与触发：
- Sequential Thinking（规划分解）：需要路线图/执行计划/风险分解时触发；输出仅含计划与里程碑，不暴露中间推理细节。
- Context7（官方文档/SDK）：查阅 API/版本差异/配置；流程为 resolve-library-id → get-library-docs；输出引用库 ID/版本与关键片段定位。
- DuckDuckGo（网页搜索）：需要最新网页信息或权威来源入口时触发；使用精准关键词与站点限定；结果数量 ≤35，优先官方与权威站点。
- Serena（代码检索/符号级编辑）：用于大规模代码语义检索与变更建议（如环境已接入时）。

速率与降级（要点）：
- 429 或限流：退避 20 秒，降低结果数；必要时切换备选服务。
- 不上传敏感信息；默认只读访问；必要时先征得授权。

---

## 3. 项目总览（架构与能力）

- 技术栈：TypeScript + Node.js（建议 Node 18+）
- 模型定位：SPO（三元组）原生的轻量嵌入式“类人脑”知识库，支持链式联想与属性存储
- 存储形态：
  - 单文件主库：`<name>.synapsedb`
  - 分页索引目录：`<name>.synapsedb.pages/`
  - 增量写前日志：`<name>.synapsedb.wal`
- 已实现：
  - WAL v2 批次提交（BEGIN/COMMIT/ABORT）与崩溃恢复
  - 六序索引 + 分页化磁盘索引（可 Brotli 压缩）
  - 增量/整序 compaction、页面级 GC、热度统计与半衰衰减
  - 读一致性（epoch-pin）、读者注册与尊重读者的治理流程

目录结构与关键文件（节选）：
- `src/index.ts` 顶层导出与连接工具
- `src/synapseDb.ts` 数据库主 API（open/addFact/find/flush/...）
- `src/query/queryBuilder.ts` 联想查询（follow/followReverse/all/anchor）
- `src/storage/*` 字典/三元组/属性/分页索引/WAL/读者登记等
- 设计文档：`docs/SynapseDB设计文档.md`
- 测试：`tests/`（Vitest 全面覆盖）

---

## 4. 对外 API 摘要

- 连接工具：
  - `ensureConnectionOptions(options)`：校验并补全端口
  - `buildConnectionUri(options)`：生成稳定 URI
  - `sanitizeConnectionOptions(options)`：口令仅保留末四位，其余打码
- `class SynapseDB`：
  - `open(path, { indexDirectory?, pageSize?, rebuildIndexes?, compression?, enableLock?, registerReader? })`
  - `addFact(fact, { subjectProperties?, objectProperties?, edgeProperties? })`
  - `find(criteria, { anchor? })` → `QueryBuilder`
  - `deleteFact(fact)`、`listFacts()`、`flush()`、`close()`
  - 事务批次：`beginBatch()`、`commitBatch()`、`abortBatch()`
  - 实用：`getNodeId`/`getNodeValue`/`getNodeProperties`/`getEdgeProperties`
- `class QueryBuilder`：`follow()`、`followReverse()`、`all()`、`where()`、`limit()`、`anchor()`

查询与索引：
- 任意 `subject/predicate/object` 组合起查，按前缀最优选择六序索引（如 `s+p → SPO`）。
- 链式联想支持正反向跳转；同跳以 `sid:pid:oid` 去重并推进前沿。
- 读一致性：链式查询期间固定 manifest `epoch`（epoch-pin）。

---

## 5. 存储格式与持久化

- 主数据文件：魔数 `SYNAPSEDB`、版本 `2`、头部 64B；区段含 `dictionary/triples/indexes(staging)/properties`
- 分页索引目录：
  - 页文件：`SPO|SOP|POS|PSO|OSP|OPS.idxpage`
  - 清单：`index-manifest.json`（`pageSize/compression/lookups/tombstones/epoch/orphans`）
  - 元数据：`hotness.json`、`readers.json`
  - 压缩：`{ codec: 'none' | 'brotli', level?: 1~11 }`
- WAL v2：追加写；崩溃后由重放器恢复并在校验失败处尾部安全截断。
- 刷新：`db.flush()` 持久化并增量合并分页索引，然后重置 WAL。

---

## 6. 构建、测试与本地开发

常用命令（见 `package.json`）：
- 安装依赖：`pnpm install`
- 开发监听：`pnpm dev`
- 构建：`pnpm build`
- 类型检查：`pnpm typecheck`
- 规范检查：`pnpm lint` / `pnpm lint:fix`
- 单测：`pnpm test` / `pnpm test:watch`
- 覆盖率：`pnpm test:coverage`（V8，报告位于 `coverage/`）
- 维护/治理：`pnpm db:*` 系列工具（check/repair/compact/auto-compact/gc/hot/stats/dump/txids）

测试规范与门槛：
- 位置：`tests/**/*.test.ts`
- 主题覆盖：持久化/WAL/索引选择/联想查询/compaction/GC/修复/崩溃注入/锁等
- 覆盖率门槛（见 `vitest.config.ts`）：Statements ≥80%，Branches ≥75%，Functions ≥80%，Lines ≥80%

---

## 7. 编码风格与命名

- 统一格式：`.prettierrc`（单引号、分号、尾随逗号、宽度 100、缩进 2）
- Lint：ESLint flat config（`eslint.config.js`），启用 `@typescript-eslint` 与 Prettier
- 命名：文件短横线风格；类型/常量语义化；模块导出用动词函数或名词化类型
- 路径别名：`@` → `src/`（源码与测试共用）
- 错误处理：使用 `src/utils/fault.ts` 中的自定义错误类型
- JSDoc：所有公共 API 必须具备充分注释

---

## 8. 提交、PR 与分支

- 提交信息：遵循 Conventional Commits
  - 示例：`feat: 分页索引支持 brotli 压缩`、`fix: 修复 WAL 重放边界条件`、`docs: 更新使用示例`
- 提交流程：提交前通过 `pnpm typecheck && pnpm lint:core && pnpm test(:coverage) && pnpm build`
- PR 要求：描述影响面与验证方式；涉及外部 API/脚本变更需同步更新文档与示例
- 分支策略：`main` 为稳定分支；特性分支采用 `feat/<topic>`；缺陷修复采用 `fix/<issue>`

---

## 9. 安全与配置

- 禁止提交：真实凭据、生产 URI、私钥等敏感数据
- 配置传递：仅通过环境变量；必要时提供 `.env.example`
- 外部依赖：在 PR 中说明鉴权策略、回滚思路与最小权限
- 运维建议：
  - 批量治理前先 `--dry-run` 获取统计，优先增量模式与 TopK 精确重写
  - 治理后运行 `db:gc --respect-readers` 清理 `orphans`

---

## 10. 使用示例（最小可运行）

```ts
import { SynapseDB } from '@/synapseDb';

const db = await SynapseDB.open('brain.synapsedb');
db.addFact({ subject: 'file:/src/user.ts', predicate: 'DEFINES', object: 'class:User' });
db.addFact({ subject: 'class:User', predicate: 'HAS_METHOD', object: 'method:login' });

const authors = db
  .find({ object: 'method:login' })
  .followReverse('HAS_METHOD')
  .followReverse('DEFINES')
  .all();

await db.flush();
```

---

## 11. 智能体执行准则（落地细则）

- 计划管理：
  - 复杂任务使用“计划 → 执行 → 校验”的节奏；计划 6~10 步，每步一句话。
  - 使用内置 Plan 工具（若可用）更新进度；每阶段产出可验证结果。

- 变更准入：
  - 仅修改与任务相关的文件；避免顺手修复无关问题。
  - 不随意更名文件/变量；保持最小差异与一致风格。
  - 代码中不新增版权或许可证头（除非明确要求）。

- 校验与格式化：
  - 先运行与改动最相关的测试，再逐步扩大范围。
  - 若有格式化工具已配置（Prettier/ESLint），在提交前确保通过。

- 输出与交互：
  - 输出为可执行的指令或可落地的补丁；引用具体文件路径与行号。
  - 重要外呼在答复末尾追加“工具调用简报”。

---

## 12. CLAUDE/AGENTS 差异已统一的内容

- 语言与注释：统一中文。
- 行为基线：先计划后执行、最小必要改动、可追溯、可回滚。
- 构建测试：以 `pnpm` 工具链为准，pre-commit/pre-push 由 Husky 执行类型检查、lint、覆盖率与构建。
- 架构摘要、API 清单、运维 CLI 与测试主题：以本文件“项目总览/对外 API/构建与测试/使用示例”为准。

---

## 13. 附：常见注意事项

- 写入后需 `flush()` 以持久化与增量合并分页索引，并重置 WAL。
- `rebuildIndexes: true` 可在下次 `open()` 时强制重建分页索引（或 `pageSize` 变更时自动重建）。
- 逻辑删除通过 tombstones 记录，查询自动过滤；重启后由 manifest 恢复。
- 属性存储为 JSON，维护版本号 `__v`；多次覆盖写入会提升版本。
- 诊断：`db:check --summary`（epoch/orphans/多页 primary）；热点观测用 `db:hot`。

---

（本文件 `base.md` 为统一规则来源；若有冲突或缺漏，请提交 PR 同步修订。）


# NervusDB 代码审查报告（2025-12-27，commit 87997e40）

生成方式：使用 `repomix-output.md`（全仓库打包）+ `git ls-files`（真实文件清单）做静态审查与归类。

- 文件数：272（`git ls-files`）
- 扫描统计（选定关键目录）：`unwrap` 396，`expect` 12，`panic!` 17，`unsafe {` 39

## A. 必须先承认的管理问题

- `docs/spec.md` 不存在：技术边界/兼容性承诺/性能预算缺少单一真相源。现在还能靠作者脑子记，团队一大就会崩。

## 0. 核心结论（按破坏性排序）

1) **High：FFI/Node 存在真实 UB 风险**（设计上不合法）。
   - 根因：用 `Arc::as_ptr(..) as *mut T` 伪造 `&mut T`，绕过 Rust 借用规则。
   - 直接证据（全仓库仅 3 处）：

```text
bindings/uniffi/nervusdb-uniffi/src/lib.rs:352:    let db = unsafe { &mut *(Arc::as_ptr(&state.db) as *mut CoreDatabase) };
bindings/node/native/nervusdb-node/src/lib.rs:59:        Ok(unsafe { &mut *(Arc::as_ptr(db) as *mut Database) })
nervusdb-core/src/ffi.rs:126:        Ok(&mut *(Arc::as_ptr(&handle.db) as *mut Database))
```

2) **High：v2 WAL 编码存在可触发 `panic!` 路径**（输入导致进程崩溃 = DoS）。
   - 直接证据：

```text
122:                    .unwrap_or_else(|_| panic!("label name too long: {} bytes", name_bytes.len()));
166:                    .unwrap_or_else(|_| panic!("key too long: {} bytes", key_bytes.len()));
184:                    .unwrap_or_else(|_| panic!("key too long: {} bytes", key_bytes.len()));
194:                    .unwrap_or_else(|_| panic!("key too long: {} bytes", key_bytes.len()));
204:                    .unwrap_or_else(|_| panic!("key too long: {} bytes", key_bytes.len()));
```

3) **Medium：锁 `.unwrap()`/`.expect()` 在库代码里很多**（poison 后崩库用户）。
   - 这不是‘风格问题’，是‘库把异常升级成进程崩溃’。

4) **Low/Medium：文档与实现漂移**（误导用户也是破坏 userspace）。
   - 例：`README.md` 仍写 OPTIONAL MATCH/MERGE/WITH/UNION/聚合不支持；但 `docs/tasks.md` 记录这些已做完（v1）。

## 1. 模块级审查（重点模块）

### 1.1 `nervusdb-core/`（v1 核心库）
- 作用：redb 持久化 + 三索引 hexastore + Cypher parser/planner/executor + C ABI。
- 主要风险：`src/ffi.rs` 的写路径通过 `Arc::as_ptr` 造 `&mut Database`（UB）。
- 次要风险：缓存锁 `expect()`（poison 会崩）。

### 1.2 `bindings/node/native/nervusdb-node/`（N-API addon）
- 作用：把 Rust Core 暴露给 Node（包括 statement API、批量查询、算法等）。
- 主要风险：同样的 `Arc::as_ptr` 造 `&mut`；虽然用 `Mutex` 串行化，但别名规则仍然可能被 statement/iterator 打破。

### 1.3 `bindings/uniffi/`（UniFFI）
- 作用：worker 线程模型把所有 DB 操作集中在单线程，接口通过 channel 传递命令。
- 评价：这是正确方向；但内部仍有 `Arc::as_ptr` 造 `&mut`，只是‘靠约束’把风险压住了。

### 1.4 `nervusdb-v2-storage/`（v2 存储内核）
- 作用：pager + WAL + snapshot + CSR segment + compaction + crash model。
- 主要风险：WAL encode `panic!`；大量锁 `.unwrap()`；label recovery 用 placeholder 填洞（需要非常清晰的外部约束，否则会污染 label 空间）。

### 1.5 `nervusdb-v2-query/`（v2 查询引擎）
- 作用：v2 的 parser/planner/executor/facade（pull-based streaming）。
- 观察：存在 `unreachable!()` 分支（如果不变量被打破就会炸）。建议改成返回 `Error`，除非你能证明输入永远达不到。

## 2. 建议的‘最小改动’修复路线（不破 ABI/不破 userspace）

- FFI/Node：加写门禁（有活跃 statement 时禁止写；返回错误码/抛异常）。这不改头文件 ABI，也不改变正常用户路径。
- v2 WAL：在 API 边界做 label/key 长度上限校验并返回 `Error`，WAL encode 永远不 panic。
- 锁 poison：把 `unwrap/expect` 改成错误返回，别把库错误升级成进程崩溃。
- 文档：更新支持矩阵，至少对齐现状（v1 vs v2 分开写）。

## 3. 每个文件的作用（逐文件一行）

| 文件 | 作用 | 依据 |
|---|---|---|
| `.github/pull_request_template.md` | Markdown 文档。（摘要：改动说明） | 文件头注释/标题 |
| `.github/workflows/ci.yml` | CI 工作流（构建/测试/门禁/崩溃门）。 | 路径/命名规则 |
| `.github/workflows/crash-gate-v2.yml` | CI 工作流（构建/测试/门禁/崩溃门）。 | 路径/命名规则 |
| `.github/workflows/crash-gate.yml` | CI 工作流（构建/测试/门禁/崩溃门）。 | 路径/命名规则 |
| `.gitignore` | Git 忽略规则（避免把构建产物/本地文件纳入版本控制）。 | 路径/命名规则 |
| `.husky/pre-commit` | Git hooks（提交前/推送前检查）。 | 路径/命名规则 |
| `.husky/pre-push` | Git hooks（提交前/推送前检查）。 | 路径/命名规则 |
| `.lintstaged.cjs` | JavaScript 脚本/工具。（摘要：module.exports = {） | 文件头注释/标题 |
| `.prettierignore` | Prettier 忽略清单。 | 路径/命名规则 |
| `.prettierrc` | Prettier 格式化配置。 | 路径/命名规则 |
| `.repomixignore` | Repomix 忽略清单（打包审查时排除）。 | 路径/命名规则 |
| `AGENTS.md` | Markdown 文档。（摘要：Intelligent Development Core (v11.1)） | 文件头注释/标题 |
| `CHANGELOG.md` | 变更记录（版本发布说明）。（摘要：变更日志） | 文件头注释/标题 |
| `CLAUDE.md` | Markdown 文档。（摘要：Intelligent Development Core (v11.1)） | 文件头注释/标题 |
| `COMMERCIAL_LICENSE.md` | 许可证/商业许可条款。（摘要：Commercial License） | 文件头注释/标题 |
| `Cargo.lock` | Rust 依赖锁定文件（可重复构建）。 | 路径/命名规则 |
| `Cargo.toml` | Rust workspace 配置（成员 crate/特性/依赖）。 | 路径/命名规则 |
| `GEMINI.md` | Markdown 文档。（摘要：Intelligent Development Core (v11.1)） | 文件头注释/标题 |
| `LICENSE` | 许可证/商业许可条款。 | 路径/命名规则 |
| `README.md` | 项目入口文档（定位/用法/结构/链接）。（摘要：NervusDB） | 文件头注释/标题 |
| `bindings/node/.npmrc` | 仓库文件（用途需结合上下文）。 | 路径/命名规则 |
| `bindings/node/benchmarks/basic.mjs` | JavaScript 脚本/工具。 | 路径/命名规则 |
| `bindings/node/benchmarks/comprehensive.mjs` | JavaScript 脚本/工具。 | 路径/命名规则 |
| `bindings/node/benchmarks/data/dmr-sample.json` | JSON 配置/数据样例。 | 路径/命名规则 |
| `bindings/node/benchmarks/data/longmemeval-sample.json` | JSON 配置/数据样例。 | 路径/命名规则 |
| `bindings/node/benchmarks/framework.mjs` | JavaScript 脚本/工具。 | 路径/命名规则 |
| `bindings/node/benchmarks/insert_scan.mjs` | JavaScript 脚本/工具。 | 路径/命名规则 |
| `bindings/node/benchmarks/path_agg.mjs` | JavaScript 脚本/工具。 | 路径/命名规则 |
| `bindings/node/benchmarks/quick.mjs` | JavaScript 脚本/工具。 | 路径/命名规则 |
| `bindings/node/benchmarks/run-all.mjs` | JavaScript 脚本/工具。 | 路径/命名规则 |
| `bindings/node/benchmarks/temporal-memory.mjs` | JavaScript 脚本/工具。 | 路径/命名规则 |
| `bindings/node/benchmarks/wasm-vs-js.mjs` | JavaScript 脚本/工具。 | 路径/命名规则 |
| `bindings/node/build.advanced.mjs` | JavaScript 脚本/工具。（摘要：NervusDB Advanced Build Configuration） | 文件头注释/标题 |
| `bindings/node/build.config.mjs` | JavaScript 脚本/工具。（摘要：NervusDB Build Configuration） | 文件头注释/标题 |
| `bindings/node/eslint.config.js` | ESLint 配置（Node 绑定）。 | 路径/命名规则 |
| `bindings/node/native/nervusdb-node/.gitignore` | Node N-API Rust addon（JS ↔ Rust Core 桥）。 | 路径/命名规则 |
| `bindings/node/native/nervusdb-node/Cargo.lock` | Node N-API Rust addon（JS ↔ Rust Core 桥）。 | 路径/命名规则 |
| `bindings/node/native/nervusdb-node/Cargo.toml` | Node N-API Rust addon（JS ↔ Rust Core 桥）。 | 路径/命名规则 |
| `bindings/node/native/nervusdb-node/build.rs` | Node N-API Rust addon（JS ↔ Rust Core 桥）。 | 路径/命名规则 |
| `bindings/node/native/nervusdb-node/napi.config.json` | Node N-API Rust addon（JS ↔ Rust Core 桥）。 | 路径/命名规则 |
| `bindings/node/native/nervusdb-node/npm/index.d.ts` | Node N-API Rust addon（JS ↔ Rust Core 桥）。 | 路径/命名规则 |
| `bindings/node/native/nervusdb-node/npm/index.js` | Node N-API Rust addon（JS ↔ Rust Core 桥）。 | 路径/命名规则 |
| `bindings/node/native/nervusdb-node/src/lib.rs` | Node N-API Rust addon（JS ↔ Rust Core 桥）。 | 路径/命名规则 |
| `bindings/node/package.json` | Node 绑定包配置（依赖/脚本/发布）。 | 路径/命名规则 |
| `bindings/node/pnpm-lock.yaml` | YAML 配置（CI/工具）。 | 路径/命名规则 |
| `bindings/node/repro_hang.ts` | TypeScript 源码文件。 | 路径/命名规则 |
| `bindings/node/scripts/bench-standard.mjs` | 基准测试脚本/程序。 | 路径/命名规则 |
| `bindings/node/scripts/check-contract.mjs` | 契约/接口一致性检查脚本（CI 门禁）。 | 路径/命名规则 |
| `bindings/node/scripts/check-coverage-per-file.mjs` | 覆盖率检查/统计脚本。 | 路径/命名规则 |
| `bindings/node/scripts/dump-graph.mjs` | 调试导出脚本。 | 路径/命名规则 |
| `bindings/node/scripts/memory-leak-analysis.mjs` | 内存泄漏分析脚本。 | 路径/命名规则 |
| `bindings/node/scripts/migrate-ndjson.mjs` | 迁移工具/脚本。 | 路径/命名规则 |
| `bindings/node/scripts/organize-native-artifact.mjs` | JavaScript 脚本/工具。 | 路径/命名规则 |
| `bindings/node/scripts/run-vitest-seq.mjs` | JavaScript 脚本/工具。 | 路径/命名规则 |
| `bindings/node/scripts/update-dir-tree.mjs` | JavaScript 脚本/工具。 | 路径/命名规则 |
| `bindings/node/src/cli/bench.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。 | 路径/命名规则 |
| `bindings/node/src/cli/cypher.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。 | 路径/命名规则 |
| `bindings/node/src/cli/nervusdb.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。（摘要：NervusDB 顶层 CLI 分发器） | 文件头注释/标题 |
| `bindings/node/src/core/index.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。（摘要：NervusDB Core - Database Kernel (v2.0)） | 文件头注释/标题 |
| `bindings/node/src/core/storage/persistentStore.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。（摘要：PersistentStore - Rust-Native Storage Wrapper (v2.0)） | 文件头注释/标题 |
| `bindings/node/src/core/storage/temporal/temporalStore.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。 | 路径/命名规则 |
| `bindings/node/src/core/storage/types.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。（摘要：Shared storage-layer type definitions.） | 文件头注释/标题 |
| `bindings/node/src/examples/coreUsage.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。（摘要：示例：薄绑定（thin binding）） | 文件头注释/标题 |
| `bindings/node/src/extensions/index.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。（摘要：NervusDB Extensions - Application Layer） | 文件头注释/标题 |
| `bindings/node/src/extensions/query/iterator.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。（摘要：流式查询迭代器与工具函数） | 文件头注释/标题 |
| `bindings/node/src/graph/labels.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。 | 路径/命名规则 |
| `bindings/node/src/index.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。（摘要：=======================） | 文件头注释/标题 |
| `bindings/node/src/native/core.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。 | 路径/命名规则 |
| `bindings/node/src/nervusDb.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。 | 路径/命名规则 |
| `bindings/node/src/types/openOptions.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。（摘要：NervusDB 数据库打开选项） | 文件头注释/标题 |
| `bindings/node/src/utils/experimental.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。 | 路径/命名规则 |
| `bindings/node/src/utils/fault.ts` | Node/TS 绑定实现（对外 API、加载 native、工具）。 | 路径/命名规则 |
| `bindings/node/tests/setup/global-cleanup.ts` | 测试用例（回归/契约/一致性验证）。 | 路径/命名规则 |
| `bindings/node/tests/unit/cypher/cypher_query.test.ts` | 测试用例（回归/契约/一致性验证）。 | 路径/命名规则 |
| `bindings/node/tests/unit/native/native_addon_smoke.native.test.ts` | 测试用例（回归/契约/一致性验证）。 | 路径/命名规则 |
| `bindings/node/tests/unit/native/native_loader.test.ts` | 测试用例（回归/契约/一致性验证）。 | 路径/命名规则 |
| `bindings/node/tests/unit/native/native_loader_resolve.test.ts` | 测试用例（回归/契约/一致性验证）。 | 路径/命名规则 |
| `bindings/node/tests/unit/native/native_statement.native.test.ts` | 测试用例（回归/契约/一致性验证）。 | 路径/命名规则 |
| `bindings/node/tests/unit/storage/persistentStore.native.test.ts` | 测试用例（回归/契约/一致性验证）。 | 路径/命名规则 |
| `bindings/node/tests/unit/temporal/temporal_guard.test.ts` | 测试用例（回归/契约/一致性验证）。 | 路径/命名规则 |
| `bindings/node/tests/unit/types/openOptions.runtime.test.ts` | 测试用例（回归/契约/一致性验证）。（摘要：NervusDB 打开选项运行时守卫测试） | 文件头注释/标题 |
| `bindings/node/tsconfig.build.json` | TypeScript 编译配置（Node 绑定）。 | 路径/命名规则 |
| `bindings/node/tsconfig.json` | TypeScript 编译配置（Node 绑定）。 | 路径/命名规则 |
| `bindings/node/tsconfig.vitest.json` | TypeScript 编译配置（Node 绑定）。 | 路径/命名规则 |
| `bindings/node/verify_native_query.ts` | 验证脚本（用于手动/CI smoke）。 | 路径/命名规则 |
| `bindings/node/verify_native_storage.ts` | 验证脚本（用于手动/CI smoke）。 | 路径/命名规则 |
| `bindings/node/verify_props.ts` | 验证脚本（用于手动/CI smoke）。 | 路径/命名规则 |
| `bindings/node/vitest.config.ts` | Vitest 测试配置（Node 绑定）。 | 路径/命名规则 |
| `bindings/python/nervusdb-py/pyproject.toml` | Python 绑定（PyO3/maturin 包装与测试）。 | 路径/命名规则 |
| `bindings/python/nervusdb-py/python/nervusdb/__init__.py` | Python 绑定（PyO3/maturin 包装与测试）。 | 路径/命名规则 |
| `bindings/python/nervusdb-py/tests/test_basic.py` | Python 绑定（PyO3/maturin 包装与测试）。 | 路径/命名规则 |
| `bindings/uniffi/nervusdb-uniffi/Cargo.toml` | UniFFI 绑定（跨语言封装 + worker 线程模型）。 | 路径/命名规则 |
| `bindings/uniffi/nervusdb-uniffi/build.rs` | UniFFI 绑定（跨语言封装 + worker 线程模型）。 | 路径/命名规则 |
| `bindings/uniffi/nervusdb-uniffi/src/lib.rs` | UniFFI 绑定（跨语言封装 + worker 线程模型）。 | 路径/命名规则 |
| `bindings/uniffi/nervusdb-uniffi/src/nervusdb.udl` | UniFFI 绑定（跨语言封装 + worker 线程模型）。 | 路径/命名规则 |
| `bindings/uniffi/nervusdb-uniffi/src/v2/mod.rs` | UniFFI 绑定（跨语言封装 + worker 线程模型）。（摘要：v2 Query API bindings for UniFFI） | 文件头注释/标题 |
| `cspell.config.cjs` | JavaScript 脚本/工具。（摘要：cspell configuration for SynapseDB） | 文件头注释/标题 |
| `docs/design/T1-storage-perf-baseline.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T1: Core 写读性能基线（索引收口 + 事务/表句柄复用 + 字符串缓存）） | 文件头注释/标题 |
| `docs/design/T10-binary-row-iterator.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T10：C API 二进制 Row 迭代器 + ABI 冻结策略） | 文件头注释/标题 |
| `docs/design/T11-perf-report-refresh.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T11: 性能重测与报告刷新（基于 T10 stmt API）） | 文件头注释/标题 |
| `docs/design/T12-release-1.0-prep.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T12: 1.0 封版准备（ABI 冻结 + 文档清洗 + Crash Gate 复跑）） | 文件头注释/标题 |
| `docs/design/T13-node-statement-api.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T13: Node Statement API（对标 T10，拆掉 V8 对象爆炸）） | 文件头注释/标题 |
| `docs/design/T14-release-v1.0.0.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T14: v1.0.0 封版（契约/白名单/Crash Gate）） | 文件头注释/标题 |
| `docs/design/T15-true-streaming.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T15: 真流式 Cypher 执行器） | 文件头注释/标题 |
| `docs/design/T17-true-streaming.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T17: 真流式执行器（消除 collect）） | 文件头注释/标题 |
| `docs/design/T18-node-property-optimization.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T18: Node.js 属性写入优化 - 消除 JSON 序列化） | 文件头注释/标题 |
| `docs/design/T19-temporal-separation.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T19: temporal_v2 分离为独立 crate） | 文件头注释/标题 |
| `docs/design/T2-drop-synapsedb-pages.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T2: 清理 Node 侧 `.synapsedb/.pages` 遗留（归档/删除）） | 文件头注释/标题 |
| `docs/design/T20-storage-key-compression.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T20: 存储键压缩设计） | 文件头注释/标题 |
| `docs/design/T21-order-by-skip.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T21: Cypher ORDER BY + SKIP） | 文件头注释/标题 |
| `docs/design/T22-aggregate-functions.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T22: Cypher Aggregate Functions） | 文件头注释/标题 |
| `docs/design/T23-with-clause.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T23: Cypher WITH Clause） | 文件头注释/标题 |
| `docs/design/T24-optional-match.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T24: Cypher OPTIONAL MATCH） | 文件头注释/标题 |
| `docs/design/T25-merge.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T25: Cypher MERGE） | 文件头注释/标题 |
| `docs/design/T26-variable-length-paths.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T26: Cypher Variable-Length Paths） | 文件头注释/标题 |
| `docs/design/T27-union.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T27: Cypher UNION / UNION ALL） | 文件头注释/标题 |
| `docs/design/T28-built-in-functions.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T28: Cypher Built-in Functions） | 文件头注释/标题 |
| `docs/design/T29-case-when.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T29: Cypher CASE WHEN） | 文件头注释/标题 |
| `docs/design/T3-intern-lru.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T3: 重写 Rust interning（真 LRU）） | 文件头注释/标题 |
| `docs/design/T30-exists-call-subqueries.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T30: EXISTS / CALL Subqueries） | 文件头注释/标题 |
| `docs/design/T31-list-literals-comprehensions.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T31: List Literals and List Comprehensions） | 文件头注释/标题 |
| `docs/design/T32-cypher-unwind-distinct-collect.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T32: Cypher UNWIND + DISTINCT + COLLECT 测试覆盖） | 文件头注释/标题 |
| `docs/design/T33-vector-and-fts.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T33: Vector Index + Full-Text Search (FTS)） | 文件头注释/标题 |
| `docs/design/T34-index-acceleration.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T34: FTS Index Pushdown (txt_score)） | 文件头注释/标题 |
| `docs/design/T35-vector-topk-pushdown.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T35: Vector Top-K Pushdown (ORDER BY + LIMIT)） | 文件头注释/标题 |
| `docs/design/T36-release-v1.0.3.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T36: 发布准备 v1.0.3（Rust + npm）） | 文件头注释/标题 |
| `docs/design/T37-uniffi-bindings.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T37: UniFFI 多语言绑定（以 C ABI Statement 为唯一硬契约）） | 文件头注释/标题 |
| `docs/design/T38-node-contract-ci.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T38: Node 真流式 Statement + 契约门禁（对齐 `nervusdb.h`）） | 文件头注释/标题 |
| `docs/design/T39-rust-cli.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T39: Rust CLI（查询/流式输出）） | 文件头注释/标题 |
| `docs/design/T4-node-bulk-resolve.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T4: Node 吞吐修复（批量返回字符串 triples）） | 文件头注释/标题 |
| `docs/design/T40-v2-kernel-spec.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T40: NervusDB v2 Kernel Spec（Property Graph + LSM Segments）） | 文件头注释/标题 |
| `docs/design/T41-v2-workspace-and-crate-structure.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T41: v2 Workspace / Crate 结构与边界） | 文件头注释/标题 |
| `docs/design/T42-v2-m0-pager-wal.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T42: v2 M0 — Pager + WAL Replay（Kernel 可验证内核）） | 文件头注释/标题 |
| `docs/design/T43-v2-m1-idmap-memtable-snapshot.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T43: v2 M1 — IDMap + MemTable + Snapshot（Log-Structured Graph）） | 文件头注释/标题 |
| `docs/design/T44-v2-m2-csr-segments-and-compaction.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T44: v2 M2 — CSR Segments + 显式 Compaction（读性能质变）） | 文件头注释/标题 |
| `docs/design/T45-v2-durability-checkpoint-and-crash-model.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T45: v2 Durability / Checkpoint / Crash Model（别自欺欺人）） | 文件头注释/标题 |
| `docs/design/T46-v2-public-api-facade.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T46: v2 Public API Facade（Rust First，绑定后置）） | 文件头注释/标题 |
| `docs/design/T47-v2-query-storage-boundary.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T47: v2 Query ↔ Storage 边界（Executor 重写的契约）） | 文件头注释/标题 |
| `docs/design/T48-v2-benchmark-and-perf-gate.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T48: v2 Benchmarks & Perf Gate（别让性能回归偷偷发生）） | 文件头注释/标题 |
| `docs/design/T49-v2-crash-gate.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T49: v2 Crash Gate（证明你真的不会丢数据）） | 文件头注释/标题 |
| `docs/design/T5-fuck-off-test.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T5: Fuck-off Test（`kill -9` 崩溃一致性验证）） | 文件头注释/标题 |
| `docs/design/T50-v2-m3-query-crate.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T50: v2 M3 — Query Crate（复用 Parser/Planner 的落地）） | 文件头注释/标题 |
| `docs/design/T51-v2-m3-executor-mvp.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T51: v2 M3 — Executor MVP（基于 GraphSnapshot 的流式算子）） | 文件头注释/标题 |
| `docs/design/T52-v2-m3-query-api.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T52: v2 M3 — Query API（prepare/execute_streaming + 参数）） | 文件头注释/标题 |
| `docs/design/T53-v2-m3-query-tests.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T53: v2 M3 — Query Tests + CLI 验收路径） | 文件头注释/标题 |
| `docs/design/T54-v2-property-storage.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T54: v2 属性存储层（Property Storage Layer）） | 文件头注释/标题 |
| `docs/design/T56-v2-delete.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T56-DELETE Design Document） | 文件头注释/标题 |
| `docs/design/T57-v2.0.0-release.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T57: v2.0.0 正式版发布（Spec 6.3 实现）） | 文件头注释/标题 |
| `docs/design/T58-v2-query-facade.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T58: v2 Query Facade DX 优化） | 文件头注释/标题 |
| `docs/design/T59-v2-label-interning.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T59: v2 Label Interning (String ↔ u32 Mapping)） | 文件头注释/标题 |
| `docs/design/T6-ffi-freeze.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T6: 冻结并对齐 `nervusdb.h`（最小稳定 C 契约）） | 文件头注释/标题 |
| `docs/design/T60-v2-variable-length-paths.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T60: v2 Variable Length Paths (多跳路径查询)） | 文件头注释/标题 |
| `docs/design/T61-v2-aggregation.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T61: v2 Aggregation (COUNT/SUM/AVG)） | 文件头注释/标题 |
| `docs/design/T62-v2-order-by-skip.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T62: v2 ORDER BY / SKIP / LIMIT） | 文件头注释/标题 |
| `docs/design/T63-v2-python-bindings.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T63: v2 Python Bindings） | 文件头注释/标题 |
| `docs/design/T7-node-thin-binding.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T7: Node 绑定去插件化 + 修复 Cypher 调用致命 Bug） | 文件头注释/标题 |
| `docs/design/T8-temporal-default-off.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T8: Temporal 变为 Optional Feature（Default OFF）） | 文件头注释/标题 |
| `docs/design/T9-node-ci.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：T9: Node Tests 纳入 CI（覆盖 Binding ↔ Native 交互）） | 文件头注释/标题 |
| `docs/memos/M2025-12-27-gap-analysis.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：Gap Analysis & Roadmap: Towards the "SQLite of Graph Databases"） | 文件头注释/标题 |
| `docs/memos/v2-next-steps.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：现状分析与下一步建议） | 文件头注释/标题 |
| `docs/memos/v2-status-assessment.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：NervusDB v2 项目状态评估报告） | 文件头注释/标题 |
| `docs/perf/PERFORMANCE_ANALYSIS.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：NervusDB 性能分析报告） | 文件头注释/标题 |
| `docs/perf/V2_BENCH.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：NervusDB v2：Benchmarks & Perf Gate（T48）） | 文件头注释/标题 |
| `docs/perf/v2/README.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：v2 perf runs） | 文件头注释/标题 |
| `docs/product/spec.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：NervusDB v2 — 产品规格（Spec v0.1）） | 文件头注释/标题 |
| `docs/reference/cypher_support.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：Cypher 支持范围（子集）） | 文件头注释/标题 |
| `docs/reference/project-structure.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：NervusDB 仓库结构） | 文件头注释/标题 |
| `docs/release/publishing.md` | 设计/参考/发布/性能/备忘录等文档。（摘要：发布指南（GitHub / crates.io / npm / PyPI）） | 文件头注释/标题 |
| `docs/tasks.md` | 设计/参考/发布/性能/备忘录等文档。 | 路径/命名规则 |
| `examples/c/basic_usage.c` | 示例代码（演示用法/基准脚本入口）。 | 路径/命名规则 |
| `examples/npm-package-test.mjs` | 示例代码（演示用法/基准脚本入口）。 | 路径/命名规则 |
| `nervusdb-cli/Cargo.toml` | Rust CLI（查询/输出/工具）。 | 路径/命名规则 |
| `nervusdb-cli/src/main.rs` | Rust CLI（查询/输出/工具）。 | 路径/命名规则 |
| `nervusdb-core/Cargo.lock` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/Cargo.toml` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/examples/bench_compare.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Benchmark comparison: NervusDB vs SQLite vs redb） | 文件头注释/标题 |
| `nervusdb-core/examples/bench_cypher_ffi.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Benchmark Cypher C API: JSON (exec_cypher) vs stmt (prepare/step/column).） | 文件头注释/标题 |
| `nervusdb-core/examples/bench_hexastore.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/examples/bench_temporal.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/include/nervusdb.h` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/src/algorithms/centrality.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Centrality algorithms for measuring node importance） | 文件头注释/标题 |
| `nervusdb-core/src/algorithms/mod.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Graph algorithms for NervusDB） | 文件头注释/标题 |
| `nervusdb-core/src/algorithms/pathfinding.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Pathfinding algorithms for graph traversal） | 文件头注释/标题 |
| `nervusdb-core/src/bin/nervus-crash-test.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Crash consistency verification tool ("fuck-off test").） | 文件头注释/标题 |
| `nervusdb-core/src/bin/nervus-migrate.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：NervusDB Migration Tool） | 文件头注释/标题 |
| `nervusdb-core/src/error.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Basic error and result types shared across the core crate.） | 文件头注释/标题 |
| `nervusdb-core/src/ffi.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：C FFI bindings for NervusDB.） | 文件头注释/标题 |
| `nervusdb-core/src/fts_index.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/src/lib.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：NervusDB core Rust library providing the low level storage primitives.） | 文件头注释/标题 |
| `nervusdb-core/src/migration/legacy_reader.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Reader for legacy .synapsedb file format (v1.x)） | 文件头注释/标题 |
| `nervusdb-core/src/migration/migrator.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Database migration logic from v1.x to v2.0） | 文件头注释/标题 |
| `nervusdb-core/src/migration/mod.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Migration tools for converting legacy .synapsedb format to redb） | 文件头注释/标题 |
| `nervusdb-core/src/parser.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：A minimal Cypher-like query parser and executor.） | 文件头注释/标题 |
| `nervusdb-core/src/query/ast.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/src/query/executor.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Query Executor v2 - Index-Aware Execution Engine） | 文件头注释/标题 |
| `nervusdb-core/src/query/lexer.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/src/query/mod.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/src/query/parser.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/src/query/planner.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/src/storage/disk.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/src/storage/disk_helper.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/src/storage/memory.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/src/storage/mod.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/src/storage/property.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Property serialization module using FlexBuffers for high performance.） | 文件头注释/标题 |
| `nervusdb-core/src/storage/schema.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/src/storage/varint_key.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Varint-encoded triple key for compact storage） | 文件头注释/标题 |
| `nervusdb-core/src/triple.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Basic triple/fact representation helpers.） | 文件头注释/标题 |
| `nervusdb-core/src/vector_index.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/tests/cypher_query_test.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Integration tests for Cypher query engine） | 文件头注释/标题 |
| `nervusdb-core/tests/fts_index_test.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/tests/fts_pushdown_test.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/tests/persistence_test.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/tests/property_binary_test.rs` | v1 核心库（存储/查询/FFI/算法）。（摘要：Integration test for FlexBuffers binary property storage） | 文件头注释/标题 |
| `nervusdb-core/tests/query_basic.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/tests/temporal_bench.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/tests/temporal_query_bench.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/tests/vector_index_test.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-core/tests/vector_topk_pushdown_test.rs` | v1 核心库（存储/查询/FFI/算法）。 | 路径/命名规则 |
| `nervusdb-temporal/Cargo.toml` | Temporal 可选功能 crate。 | 路径/命名规则 |
| `nervusdb-temporal/src/lib.rs` | Temporal 可选功能 crate。（摘要：Temporal Store v2 - Multi-table Architecture） | 文件头注释/标题 |
| `nervusdb-v2-api/Cargo.toml` | v2 查询↔存储边界 trait（GraphStore/GraphSnapshot）。 | 路径/命名规则 |
| `nervusdb-v2-api/src/lib.rs` | v2 查询↔存储边界 trait（GraphStore/GraphSnapshot）。 | 路径/命名规则 |
| `nervusdb-v2-query/Cargo.toml` | v2 查询引擎（parser/planner/executor/facade）。 | 路径/命名规则 |
| `nervusdb-v2-query/src/ast.rs` | v2 查询引擎（parser/planner/executor/facade）。 | 路径/命名规则 |
| `nervusdb-v2-query/src/error.rs` | v2 查询引擎（parser/planner/executor/facade）。（摘要：Error and result types for the v2 query crate.） | 文件头注释/标题 |
| `nervusdb-v2-query/src/evaluator.rs` | v2 查询引擎（parser/planner/executor/facade）。 | 路径/命名规则 |
| `nervusdb-v2-query/src/executor.rs` | v2 查询引擎（parser/planner/executor/facade）。 | 路径/命名规则 |
| `nervusdb-v2-query/src/facade.rs` | v2 查询引擎（parser/planner/executor/facade）。（摘要：Query Facade - convenient methods for querying the graph.） | 文件头注释/标题 |
| `nervusdb-v2-query/src/lexer.rs` | v2 查询引擎（parser/planner/executor/facade）。 | 路径/命名规则 |
| `nervusdb-v2-query/src/lib.rs` | v2 查询引擎（parser/planner/executor/facade）。（摘要：NervusDB v2 Query Engine） | 文件头注释/标题 |
| `nervusdb-v2-query/src/parser.rs` | v2 查询引擎（parser/planner/executor/facade）。 | 路径/命名规则 |
| `nervusdb-v2-query/src/planner.rs` | v2 查询引擎（parser/planner/executor/facade）。 | 路径/命名规则 |
| `nervusdb-v2-query/src/query_api.rs` | v2 查询引擎（parser/planner/executor/facade）。 | 路径/命名规则 |
| `nervusdb-v2-query/tests/create_test.rs` | v2 查询引擎（parser/planner/executor/facade）。 | 路径/命名规则 |
| `nervusdb-v2-query/tests/filter_test.rs` | v2 查询引擎（parser/planner/executor/facade）。 | 路径/命名规则 |
| `nervusdb-v2-query/tests/limit_boundary_test.rs` | v2 查询引擎（parser/planner/executor/facade）。（摘要：LIMIT boundary tests for v2 query engine） | 文件头注释/标题 |
| `nervusdb-v2-query/tests/t52_query_api.rs` | v2 查询引擎（parser/planner/executor/facade）。 | 路径/命名规则 |
| `nervusdb-v2-query/tests/t53_integration_storage.rs` | v2 查询引擎（parser/planner/executor/facade）。 | 路径/命名规则 |
| `nervusdb-v2-query/tests/t60_variable_length_test.rs` | v2 查询引擎（parser/planner/executor/facade）。（摘要：T60: Variable Length Paths Tests） | 文件头注释/标题 |
| `nervusdb-v2-query/tests/t61_aggregation_test.rs` | v2 查询引擎（parser/planner/executor/facade）。（摘要：T61: Aggregation Tests） | 文件头注释/标题 |
| `nervusdb-v2-query/tests/t62_order_by_skip_test.rs` | v2 查询引擎（parser/planner/executor/facade）。（摘要：T62: ORDER BY / SKIP / LIMIT Tests） | 文件头注释/标题 |
| `nervusdb-v2-storage/Cargo.toml` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/examples/bench_v2.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。（摘要：v2 micro-bench suite (M1/M2).） | 文件头注释/标题 |
| `nervusdb-v2-storage/src/api.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/src/bin/nervusdb-v2-crash-test.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。（摘要：v2 crash consistency verification tool ("crash gate").） | 文件头注释/标题 |
| `nervusdb-v2-storage/src/csr.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/src/engine.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/src/error.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/src/idmap.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/src/label_interner.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。（摘要：Label Interner - maps label names (String) to LabelId (u32).） | 文件头注释/标题 |
| `nervusdb-v2-storage/src/lib.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/src/memtable.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/src/pager.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/src/property.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/src/snapshot.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/src/wal.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/tests/m1_graph.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/tests/m2_compaction.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/tests/properties.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/tests/t47_api_trait.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/tests/t51_snapshot_scan.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/tests/t59_label_interning.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。 | 路径/命名规则 |
| `nervusdb-v2-storage/tests/tombstone_semantics.rs` | v2 存储内核（pager/WAL/segment/compaction/snapshot）。（摘要：Tombstone semantics tests for v2 storage） | 文件头注释/标题 |
| `nervusdb-v2/Cargo.toml` | v2 对外 facade（Db/ReadTxn/WriteTxn 等）。 | 路径/命名规则 |
| `nervusdb-v2/src/lib.rs` | v2 对外 facade（Db/ReadTxn/WriteTxn 等）。 | 路径/命名规则 |
| `nervusdb-v2/tests/smoke.rs` | v2 对外 facade（Db/ReadTxn/WriteTxn 等）。 | 路径/命名规则 |
| `nervusdb-wasm/.gitignore` | WASM 构建与导出（in-memory/实验）。 | 路径/命名规则 |
| `nervusdb-wasm/Cargo.toml` | WASM 构建与导出（in-memory/实验）。 | 路径/命名规则 |
| `nervusdb-wasm/src/lib.rs` | WASM 构建与导出（in-memory/实验）。 | 路径/命名规则 |
| `nervusdb/nervusdb-v2-query/tests/create_test.rs` | 测试用例（回归/契约/一致性验证）。 | 路径/命名规则 |
| `repomix.config.json` | Repomix 配置/打包输出（用于代码审查与上下文）。 | 路径/命名规则 |
| `scripts/v2_bench.sh` | 仓库脚本（bench/发布/辅助）。 | 路径/命名规则 |

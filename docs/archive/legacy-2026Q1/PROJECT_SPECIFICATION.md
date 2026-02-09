# NervusDB v2 — 项目规范 (Project Specification)

> **版本**: v2.0
> **状态**: Production-Ready (Alpha)
> **更新日期**: 2026-01-02
>
> 本文档是项目的**最高准则**，所有开发活动必须遵循此规范。

---

## 1. 项目概览 (Project Overview)

### 1.1 产品定位
- **一句话使命**: 打造纯 Rust 的嵌入式图数据库，像 SQLite 一样"一个文件打开就能用"，专为图遍历优化
- **核心价值**: 零依赖、嵌入式、崩溃安全、Cypher 查询
- **目标用户**: 需要本地图数据存储的 Rust/Python 开发者

### 1.2 技术特性
✅ **已实现**:
- 存储: `.ndb` (页存储) + `.wal` (重做日志)
- 事务: 单写者 + 快照读并发
- 崩溃恢复: WAL replay + checkpoint
- 图数据: MemTable → L0 frozen → CSR segments (压缩)
- 查询: Cypher 子集 (openCypher v9)
- 索引: B-Tree + HNSW (向量检索扩展)
- 绑定: Rust crate + Python (基础版) + CLI

### 1.3 架构约束
- **兼容性**: v2 独立于 v1，不兼容 v1
- **外部依赖**: 零外部服务进程
- **平台**: Linux/macOS/Windows (Native)
- **安全**: 不硬编码 secrets，崩溃一致性为硬门槛
- **复杂度**: 优先清晰简单，避免过度工程

---

## 2. 项目状态 (Project Status)

### 2.1 当前里程碑: M3 → M4 (进行中)

| 组件 | 状态 | 完成度 | 备注 |
|------|------|--------|------|
| 存储引擎 | ✅ 完成 | 95% | WAL/Pager/Compaction 完整 |
| 查询引擎 | 🔄 进行中 | 80% | 大部分功能完成，TCK 覆盖率 5% |
| Python 绑定 | 🔄 进行中 | 60% | 基础功能可用，文档不全 |
| CLI 工具 | ✅ 完成 | 90% | 核心功能完整 |
| 索引系统 | ✅ 完成 | 90% | B-Tree + HNSW 已实现 |

### 2.2 TCK (openCypher Test Compatibility Kit) 状态

| 类别 | 文件数 | 通过数 | 覆盖率 |
|------|--------|--------|--------|
| expressions/literals | 8 | 1 | 12.5% |
| expressions/mathematical | 17 | 0 | 0% |
| clauses/MATCH | 9 | 0 | 0% |
| clauses/CREATE | 6 | 0 | 0% |
| **总计** | 220 | ~11 | **~5%** |

**目标**:
- M4 阶段: TCK 覆盖率 ≥ 70%
- M5 阶段: TCK 覆盖率 ≥ 90%
- v1.0: TCK 覆盖率 ≥ 95%

### 2.3 待完成的关键任务 (M4 阶段)

**P0 (阻断级)**:
- [x] M4-01: 修复 `query_api.rs` 中 16 个 NotImplemented
- [x] M4-02: 修复 `executor.rs` 中 11 个 NotImplemented
- [x] M4-03: 完善 MERGE 语义 (链式、多标签)
- [x] M4-04: SET/DELETE 支持复杂表达式

**P1 (重要)**:
- [ ] M4-07: 扩展 TCK 到所有 clauses 测试
- [ ] M4-08: 扩展 TCK 到所有 expressions 测试

---

## 3. 技术规范 (Technical Specifications)

### 3.1 代码规范

**格式化 & Lint**:
```bash
# 格式化
cargo fmt

# Lint 检查
cargo clippy --all-targets --all-features -- -D warnings

# 必须通过 CI，不能有警告
```

**测试要求**:
- 单元测试: 核心模块覆盖率 ≥ 90%
- 集成测试: 存储 + 查询端到端
- 崩溃测试: `cargo test --test crash-gate`
- TCK 测试: `cargo test --test tck_harness`

### 3.2 提交规范 (Git)

**分支策略**:
- `main`: 长期分支，始终绿色可部署
- `feat/T{ID}-{name}`: 功能分支
- `fix/T{ID}-{name}`: 修复分支
- `refactor/...`: 重构分支

**提交信息**:
```
feat(T326): 集成 openCypher TCK 测试框架

- 实现解析器模式匹配
- 添加测试用例执行器
- 支持多特性并行测试

[TDD Verification]
- Leading tests: tests/tck_harness.rs
- 新增 11 个 TCK 测试用例

Refs: #T326
```

### 3.3 文档规范

**必须包含**:
- API 文档: `cargo doc --no-deps --open`
- 更新日志: `CHANGELOG.md`
- 功能支持: `docs/reference/cypher_support.md`
- 任务追踪: `docs/tasks.md`

**禁止**:
- 硬编码 secrets
- 裸 `println!`，使用 `log::info!`
- 未文档化的 public API

---

## 4. 开发流程 (Development Workflow)

### 4.1 任务生命周期

```
1. 创建任务 (在 docs/tasks.md 中记录)
   ├── 风险评估 (Low/Medium/High)
   ├── 依赖分析
   └── 估时 (1w/2w/3w)

2. 创建分支
   git checkout -b feat/T{ID}-{name}

3. TDD 循环
   ├── 编写失败的测试
   ├── 实现最小代码通过测试
   └── 重构优化

4. 验证
   ├── cargo test (全部通过)
   ├── cargo clippy (零警告)
   ├── TCK 测试 (相关用例通过)
   └── 崩溃测试 (通过)

5. 提交 & PR
   ├── 编写清晰的提交信息
   ├── 创建 PR 到 main
   ├── CI 全部绿灯
   └── 代码审查
```

### 4.2 测试驱动开发 (TDD)

**强制要求**:
- 核心业务逻辑必须先写测试
- 每次提交必须包含测试
- 测试失败时禁止合并

**测试类型**:
- 单元测试: 测试单个函数/模块
- 集成测试: 测试组件交互
- 端到端测试: 测试完整用户路径
- 回归测试: 确保修复不破坏现有功能

### 4.3 质量门禁 (Quality Gates)

**自动化检查 (CI 必须绿灯)**:
- [ ] `cargo test` 全部通过
- [ ] `cargo clippy --all-features -- -D warnings` 零警告
- [ ] `cargo fmt --check` 代码格式正确
- [ ] 测试覆盖率 ≥ 85% (核心模块 ≥ 90%)
- [ ] TCK 相关测试通过
- [ ] 崩溃测试通过

**代码审查检查**:
- [ ] YAGNI: 无过度工程
- [ ] 易回滚: 不破坏现有功能
- [ ] 文档一致: 代码与文档匹配

---

## 5. 里程碑计划 (Milestone Plan)

### M3 (当前) - Core Foundation ✅

**目标**: 基础图操作可用
**状态**: 完成但有缺口

| 类别 | 状态 |
|------|------|
| 存储 (WAL, 崩溃安全) | ✅ |
| 基本 MATCH/CREATE/DELETE | ✅ |
| 单跳模式 | ✅ |
| 聚合 (基础) | ✅ |
| CLI | ✅ |

**已知缺口**:
- 链式 MERGE 不支持
- SET/DELETE 表达式不完整
- CASE 表达式部分支持
- 多标签 MERGE 缺失
- 匿名节点处理缺失

### M4 (2026-Q1) - Cypher Completeness 🎯

**目标**: TCK 通过率 ≥ 70%，移除大部分 NotImplemented
**状态**: 进行中 (80% 完成)

| ID | 任务 | 优先级 | 状态 |
|----|------|--------|------|
| M4-01 | 修复 query_api.rs NotImplemented | P0 | ✅ |
| M4-02 | 修复 executor.rs NotImplemented | P0 | ✅ |
| M4-03 | 完善 MERGE 语义 | P0 | ✅ |
| M4-04 | SET/DELETE 表达式 | P0 | ✅ |
| M4-07 | 扩展 TCK clauses 测试 | P0 | 🔄 |
| M4-08 | 扩展 TCK expressions 测试 | P0 | 🔄 |

**退出标准**:
- TCK 通过率 ≥ 70%
- 零 P0 NotImplemented
- 所有核心 Cypher 子句可用

### M5 (2026-Q2) - Polish & Performance

**目标**: TCK 通过率 ≥ 90%，Python 绑定稳定，文档完整

| ID | 任务 | 优先级 | 估时 |
|----|------|--------|------|
| M5-01 | 完成 Python 绑定 | P0 | 2w |
| M5-02 | 编写用户指南 | P0 | 1w |
| M5-03 | 性能基准测试 | P1 | 1w |
| M5-04 | 并发读优化 | P1 | 2w |
| M5-05 | HNSW 调优 | P2 | 1w |

**退出标准**:
- TCK 通过率 ≥ 90%
- Python 示例和文档完整
- 性能基准发布

### v1.0 (2026-Q4) - Production Ready

**目标**: TCK 通过率 ≥ 95%，生产就绪，社区采用

| ID | 任务 | 优先级 |
|----|------|--------|
| 1.0-01 | TCK 通过率 ≥ 95% | P0 |
| 1.0-02 | 安全审计 | P0 |
| 1.0-03 | Swift/iOS 绑定 | P2 |
| 1.0-04 | WebAssembly 目标 | P2 |
| 1.0-05 | 真实案例研究 | P1 |

---

## 6. 目录结构 (Directory Structure)

```
nervusdb/
├── nervusdb-v2/              # 核心数据库 (package)
│   ├── nervusdb-v2-storage/  # 存储引擎
│   ├── nervusdb-v2-query/    # 查询引擎
│   └── nervusdb-v2-api/      # API 层
├── nervusdb-cli/             # CLI 工具
├── nervusdb-pyo3/            # Python 绑定
├── docs/                     # 文档
│   ├── reference/            # 功能参考
│   ├── design/               # 设计文档
│   ├── tasks.md              # 任务追踪 (单一真实源)
│   └── specs/                # 规范文档
├── tests/                    # 测试套件
│   ├── opencypher_tck/       # TCK 测试
│   ├── fuzz_cypher.rs        # 模糊测试
│   └── *.rs                  # 集成测试
└── scripts/                  # 脚本
```

---

## 7. 常见问题 (FAQ)

### Q1: 如何知道某个功能是否已实现？
**A**: 查看 `docs/reference/cypher_support.md`，该文档实时更新所有支持的功能。

### Q2: 如何运行完整的测试套件？
**A**:
```bash
# 单元测试
cargo test

# TCK 测试
cargo test --test tck_harness

# 崩溃测试
cargo test --test crash_gate

# 完整测试
cargo test --all-targets --all-features
```

### Q3: 如何贡献代码？
**A**:
1. 从 `docs/tasks.md` 选择一个任务
2. 创建分支 `feat/T{ID}-{name}`
3. 使用 TDD 方法实现
4. 运行所有测试确保通过
5. 创建 PR 到 `main`

### Q4: 项目当前处于什么阶段？
**A**: M4 阶段 (Cypher Completeness)，目标是在 2026-Q1 完成 TCK 覆盖率 70%。

---

## 8. 参考文献 (References)

- [openCypher 规范](https://github.com/openCypher/openCypher)
- [TCK 测试套件](tests/opencypher_tck/)
- [Cypher 支持文档](docs/reference/cypher_support.md)
- [路线图](ROADMAP.md)
- [任务追踪](docs/tasks.md)

---

**维护者**: NervusDB 团队
**最后更新**: 2026-01-02
**下次评审**: 2026-02-01

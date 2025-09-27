# SynapseDB TODO 目录

本目录用于存放项目的日常任务和待办事项。

## 📁 文件命名规范

格式：`[来源标识]_任务描述_日期.md`

示例：

- `[Bug-Fix]_紧急修复清单_2025-02.md` - 紧急 bug 修复任务
- `[Feature]_新功能开发_2025-03.md` - 新功能开发任务
- `[Maintenance]_技术债务清理_2025-04.md` - 技术债务和维护任务

## 📋 里程碑文档迁移说明

**重要通知**：所有里程碑文档已迁移到 `docs/milestones/` 目录

### 📁 新的目录结构

```
docs/
├── milestones/              # 🚀 里程碑规划文档
│   ├── v1.0/               # v1.0 系列
│   │   └── [Roadmap-v2.0]_升级实施计划_2025-01.md
│   ├── v1.1/               # v1.1 系列
│   │   ├── [Milestone-0]_基础巩固_v1.1.0.md
│   │   ├── [Milestone-1]_查询增强_v1.2.0.md
│   │   ├── [Milestone-2]_标准兼容_v1.3.0.md
│   │   └── [Milestone-3]_高级特性_v1.4.0.md
│   └── README.md
└── todo/                   # 📝 具体任务和待办事项
    └── README.md           # 本文件
```

### 🔗 快速导航

- **[查看所有里程碑](../milestones/README.md)** - 完整的版本规划和技术路线
- **[v1.1 系列里程碑](../milestones/v1.1/)** - 2025年重点开发目标

### 📊 当前进度

| 里程碑   | 版本   | 状态      | 优先级 |
| -------- | ------ | --------- | ------ |
| 基础巩固 | v1.1.0 | 🚧 进行中 | P0     |
| 查询增强 | v1.2.0 | 📝 待启动 | P1     |
| 标准兼容 | v1.3.0 | 📝 待启动 | P1     |
| 高级特性 | v1.4.0 | 📝 待启动 | P2     |

## 📝 历史记录

### 已归档文档 📚

以下文档已完成历史使命，内容已整合到新的里程碑体系中：

- **[Phase-A]\_性能优化与稳定性\_2025-01.md**
  - 状态：已合并到 Milestone-0
  - 内容：查询迭代器、属性索引下推、Bug修复

- **[Phase-B]\_图数据库核心功能\_2025-02.md**
  - 状态：已合并到 Milestone-0
  - 内容：节点标签系统、变长路径查询、聚合函数框架

- **[Phase-C]\_工程化与质量提升\_2025-02.md**
  - 状态：已合并到 Milestone-0
  - 内容：性能基准测试、TypeScript类型增强、文档完善

- **[Roadmap-v2.0]\_升级实施计划\_2025-01.md**
  - 状态：已迁移到 milestones/v1.0/
  - 内容：原Beta强化计划，现为v1.0系列文档

## 🔄 更新记录

- 2025-01-24：创建 TODO 目录，迁移升级实施计划
- 2025-01-24：制定 Phase A-C 详细实施计划，替代原 Roadmap v2.0
- 2025-01-24：创建完整里程碑体系，包含所有待实现能力：
  - Milestone-0：基础巩固（合并 Phase A-C）
  - Milestone-1：查询增强（模式匹配、变长路径）
  - Milestone-2：标准兼容（Cypher、Gremlin、GraphQL）
  - Milestone-3：高级特性（全文搜索、图算法、分布式）
- 2025-01-24：**重大重构** - 将所有里程碑文档迁移到独立的 `docs/milestones/` 目录
  - 按版本系列组织：v1.0/ 和 v1.1/
  - todo 目录专注于日常任务管理
  - milestones 目录专注于长期规划

## 💡 使用建议

### Todo 目录适用于：

- 日常开发任务
- Bug 修复清单
- 紧急问题处理
- 技术债务清理
- 短期功能开发

### Milestones 目录适用于：

- 版本发布规划
- 长期技术路线
- 架构演进计划
- 功能里程碑
- 项目战略规划

---

**下次添加任务时，请根据任务性质选择合适的目录：**

- 📝 日常任务 → `docs/todo/`
- 🚀 里程碑规划 → `docs/milestones/`

---

## 子系统 TODO 索引（严格规则下的后续实现）

为便于跟踪“简化/未实现”的技术债，以下子系统增加了细分 TODO：

- 全文检索（fulltext）
  - [engine.md](./fulltext/engine.md) — Query 序列化完善与性能指标结构化
  - [synapsedbExtension.md](./fulltext/synapsedbExtension.md) — 扩展初始化显式等待/ready()
  - [scorer.md](./fulltext/scorer.md) — 评分器组合与配置化增强
  - [query.md](./fulltext/query.md) — 短语 slop、wildcard/fuzzy、布尔增强

- 空间几何（spatial）
  - [geometry.md](./spatial/geometry.md) — 严格几何计算、buffer、保拓扑简化

- 图算法（algorithms）
  - [community.md](./algorithms/community.md) — Louvain 第二阶段折叠图构建
  - [pathfinding.md](./algorithms/pathfinding.md) — A\* 路径重建与启发式策略

> 说明：上述 TODO 为增量实现计划，均保证对外 API 不破坏；默认行为与现状等价，通过可选参数开启严格/增强模式。

### 算法优化

- **[ ] 实现 Louvain 算法的图折叠功能 (`buildCommunityGraph`)**
  - **位置:** `src/algorithms/community.ts`
  - **描述:** 当前 `buildCommunityGraph` 方法是一个存根，仅返回原图的克隆，导致无法进行多层次优化。为避免无限循环，已添加临时的防御性跳出机制。需要完整实现图折叠逻辑，将同一社区的节点合并为超节点，以完成完整的 Louvain 算法，并移除临时的跳出机制。

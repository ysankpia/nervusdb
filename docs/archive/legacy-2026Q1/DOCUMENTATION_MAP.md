# 📚 NervusDB v2 文档导航图

> 本文档帮助您快速找到所需的文档和信息。

---

## 📖 文档层次结构

```
PROJECT_SPECIFICATION.md (项目最高规范)
├── 1. 项目概览 → 了解项目是什么
├── 2. 项目状态 → 当前进展和完成度
├── 3. 技术规范 → 代码/提交/文档规范
├── 4. 开发流程 → TDD 和质量门禁
├── 5. 里程碑计划 → M3/M4/M5/v1.0 路线图
├── 6. 目录结构 → 代码组织方式
├── 7. FAQ → 常见问题
└── 8. 参考文献 → 相关链接

    │
    ├──► QUICK_REFERENCE.md (快速参考)
    │   ├── 常用命令
    │   ├── 当前状态摘要
    │   └── 关键链接
    │
    ├──► README.md (项目首页)
    │   ├── 产品介绍
    │   ├── 快速上手
    │   └── 特性对比
    │
    ├──► docs/
    │   ├── tasks.md (任务追踪)
    │   │   ├── 所有任务的详细状态
    │   │   ├── 分支信息
    │   │   └── 完成度追踪
    │   │
    │   ├── reference/cypher_support.md (Cypher 支持)
    │   │   ├── 支持的查询功能列表
    │   │   └── 使用示例
    │   │
    │   ├── design/ (设计文档)
    │   │   └── 架构和实现细节
    │   │
    │   └── spec.md (旧版规范 - 已弃用)
    │       └── 转向 PROJECT_SPECIFICATION.md
    │
    ├──► ROADMAP.md (路线图)
    │   ├── 里程碑定义
    │   ├── TCK 状态
    │   └── 贡献指南
    │
    └──► CHANGELOG.md (更新日志)
        └── 版本变更历史
```

---

## 🎯 按需求查找文档

### 我想了解项目是什么
👉 **阅读顺序**:
1. [README.md](../README.md) - 项目简介和特性
2. [PROJECT_SPECIFICATION.md §1-2](PROJECT_SPECIFICATION.md#1-项目概览-project-overview) - 详细定位和状态

### 我想开始开发
👉 **阅读顺序**:
1. [PROJECT_SPECIFICATION.md §4](PROJECT_SPECIFICATION.md#4-开发流程-development-workflow) - 开发流程
2. [QUICK_REFERENCE.md](QUICK_REFERENCE.md) - 常用命令
3. [tasks.md](tasks.md) - 选择任务

### 我想使用 NervusDB
👉 **阅读顺序**:
1. [README.md §1-2](../README.md#quick-start) - 快速上手
2. [QUICK_REFERENCE.md](QUICK_REFERENCE.md#使用) - API 使用示例
3. [reference/cypher_support.md](reference/cypher_support.md) - 支持的功能

### 我想贡献代码
👉 **阅读顺序**:
1. [PROJECT_SPECIFICATION.md §4.3](PROJECT_SPECIFICATION.md#43-质量门禁-quality-gates) - 质量要求
2. [tasks.md](tasks.md) - 可选任务
3. [ROADMAP.md §Contributing](../ROADMAP.md#how-to-contribute) - 贡献流程

### 我想了解项目进展
👉 **阅读顺序**:
1. [PROJECT_SPECIFICATION.md §2.2](PROJECT_SPECIFICATION.md#22-tck-opencypher-test-compatibility-kit-状态) - TCK 状态
2. [tasks.md](tasks.md) - 任务完成度
3. [CHANGELOG.md](../CHANGELOG.md) - 版本历史

### 我想了解技术细节
👉 **阅读顺序**:
1. [PROJECT_SPECIFICATION.md §3](PROJECT_SPECIFICATION.md#3-技术规范-technical-specifications) - 技术约束
2. [design/](design/) - 设计文档
3. 代码注释和 API 文档

---

## 📋 文档状态说明

| 文档 | 状态 | 最后更新 | 负责人 |
|------|------|----------|--------|
| PROJECT_SPECIFICATION.md | ✅ 活跃 | 2026-01-02 | Linus-AGI |
| QUICK_REFERENCE.md | ✅ 活跃 | 2026-01-02 | Linus-AGI |
| README.md | ✅ 活跃 | 2026-01-02 | Linus-AGI |
| docs/tasks.md | ✅ 活跃 | 持续更新 | 团队 |
| docs/reference/cypher_support.md | ✅ 活跃 | 持续更新 | 团队 |
| ROADMAP.md | ⚠️ 需同步 | 需更新 | 团队 |
| docs/spec.md | 🗑️ 弃用 | 转向新规范 | - |

---

## 🔄 文档更新流程

### 何时更新文档？
- ✅ 新功能实现后
- ✅ API 变更时
- ✅ 里程碑达成时
- ✅ 流程变更时

### 如何更新？
1. 确定影响范围
2. 更新相关文档 (参考导航图)
3. 运行文档检查
4. 提交 PR

### 文档检查清单
- [ ] 文档与代码一致
- [ ] 链接有效
- [ ] 示例可运行
- [ ] 状态信息最新

---

## 🆘 获取帮助

| 问题类型 | 推荐文档 |
|----------|----------|
| 如何开始？ | [QUICK_REFERENCE.md](QUICK_REFERENCE.md) |
| 开发流程 | [PROJECT_SPECIFICATION.md §4](PROJECT_SPECIFICATION.md#4-开发流程-development-workflow) |
| 功能支持 | [docs/reference/cypher_support.md](docs/reference/cypher_support.md) |
| 任务列表 | [docs/tasks.md](docs/tasks.md) |
| 技术细节 | [docs/design/](docs/design/) |
| 路线图 | [ROADMAP.md](ROADMAP.md) |

**找不到答案？**
1. 搜索 GitHub Issues
2. 创建新 Issue
3. 联系维护团队

---

**维护者**: NervusDB 团队
**文档管理员**: Linus-AGI
**最后更新**: 2026-01-02

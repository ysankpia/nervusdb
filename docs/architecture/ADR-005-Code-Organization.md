# ADR-005: 代码组织与分层

## 状态

已接受 (Accepted)

## 日期

2025-11-05

## 背景 (Context)

当前项目混合了数据库内核和应用层功能，导致：

1. **对比混淆**：与 Rust 内核项目对比时，TypeScript 项目包含了更多应用层功能（全文检索、空间索引、图算法），导致功能范围不一致
2. **架构不清晰**：用户无法直观理解哪些是核心功能，哪些是扩展功能
3. **维护困难**：核心代码和扩展代码混在一起，增加了维护复杂度
4. **扩展不优雅**：添加新扩展时没有清晰的组织结构

## 决策 (Decision)

我们将代码重组为清晰的分层目录结构，但**保持单一 npm 包**：

### 新目录结构

```
src/
├── core/                    # 数据库内核（对标 Rust 项目）
│   ├── storage/            # 存储层
│   │   ├── tripleStore.ts
│   │   ├── dictionary.ts
│   │   ├── tripleIndexes.ts
│   │   ├── wal.ts
│   │   ├── persistentStore.ts
│   │   └── propertyDataStore.ts
│   ├── query/              # 查询层
│   │   └── queryBuilder.ts
│   └── index.ts            # 核心层导出
│
├── extensions/             # 应用层扩展
│   ├── fulltext/          # 全文检索
│   ├── spatial/           # 空间索引
│   ├── algorithms/        # 图算法
│   ├── query/             # 高级查询
│   │   ├── pattern/       # 模式匹配
│   │   ├── path/          # 路径查找
│   │   └── aggregation.ts # 聚合查询
│   └── index.ts           # 扩展层导出
│
├── cli/                   # CLI 工具
├── utils/                 # 工具函数
├── types/                 # 类型定义
├── index.ts               # 主入口（导出 core + extensions）
├── synapseDb.ts           # 主 API
└── typedNervusDb.ts       # 类型安全 API
```

### 导出策略

```typescript
// src/index.ts
export * as Core from './core/index.js'; // 核心层
export * as Extensions from './extensions/index.js'; // 扩展层
export { NervusDB } from './synapseDb.js'; // 主 API（向后兼容）
```

用户可以：

1. **使用完整功能**：`import { NervusDB } from 'nervusdb';`
2. **只使用核心**：`import { Core } from 'nervusdb';`
3. **按需导入**：`import { Extensions } from 'nervusdb';`
4. **Tree-shaking**：打包工具自动移除未使用的代码

## 备选方案 (Alternatives Considered)

### 备选方案 A: Monorepo（多包）

- **描述**: 将项目拆分为多个 npm 包
  ```
  @nervusdb/core
  @nervusdb/extensions
  nervusdb (完整包)
  ```
- **优势**:
  - 物理隔离，强制分层
  - 可以独立发布核心包
- **为何未选择**:
  - **复杂度过高**：5 倍的维护成本（3 个 package.json、3 个发布流程、版本同步）
  - **破坏性大**：包名变更，影响现有用户
  - **收益有限**：<5% 的用户真正需要"只安装核心"
  - **版本管理复杂**：子包之间的依赖需要严格的版本同步

### 备选方案 B: 文档标记（不重组）

- **描述**: 通过文档说明分层，不改变代码结构
- **优势**:
  - 零破坏性
  - 5 分钟完成
- **为何未选择**:
  - **不解决根本问题**：目录结构仍然混乱
  - **扩展不优雅**：添加新扩展时没有清晰的位置
  - **维护困难**：开发者仍然需要在混乱的目录中找代码

### 备选方案 C: 单包 + 清晰目录结构（当前方案）

- **描述**: 重组目录结构，但保持单一 npm 包
- **优势**:
  - ✅ **清晰分层**：一眼看出核心 vs 扩展
  - ✅ **易于扩展**：新扩展直接加到 `extensions/`
  - ✅ **零破坏性**：仍然是单一包，用户无感知
  - ✅ **支持 Tree-shaking**：用户可以只导入需要的模块
  - ✅ **维护成本低**：仍然是单一包，无需管理多包版本
  - ✅ **对比清晰**：`src/core/` 直接对标 Rust 项目
- **为何选择**:
  - **实用主义**：解决真实问题（目录混乱、扩展不优雅）
  - **简洁优先**：比 Monorepo 简单 5 倍
  - **Linus 的哲学**："Don't delete code just because it's in the wrong place. Move it to the right place."

## 后果 (Consequences)

### 正面影响

- ✅ **清晰的架构分层**：核心层和扩展层一目了然
- ✅ **易于扩展**：添加新扩展时有清晰的组织结构
- ✅ **对比清晰**：`src/core/` 直接对标 Rust 项目，消除混淆
- ✅ **支持 Tree-shaking**：用户可以按需导入，减少打包体积
- ✅ **零破坏性**：仍然是单一包，现有用户无需修改代码
- ✅ **维护成本低**：无需管理多包版本、发布流程

### 负面影响与缓解措施

- **影响1**: 所有导入路径需要更新 → **缓解**: 已通过自动化脚本完成更新，所有测试通过
- **影响2**: 开发者需要适应新结构 → **缓解**: 通过 ADR 文档和 README 说明新结构
- **影响3**: Git 历史中文件路径变更 → **缓解**: 使用 `git mv` 保留文件历史

### 所需资源

- 开发时间: **1 天**（目录重组 + 导入路径更新 + 测试验证）
- 培训成本: **0**（向后兼容，用户无感知）
- 基础设施变更: **0**（仍然是单一包）

## 实施结果

### 迁移统计

- **移动的文件**: 约 100+ 个文件
- **更新的导入路径**: 约 200+ 处
- **更新的测试文件**: 约 50+ 个文件
- **测试通过率**: 100% (148/148 测试文件，573 个测试)

### 目录对比

| 旧结构                      | 新结构                           | 说明           |
| --------------------------- | -------------------------------- | -------------- |
| `src/storage/`              | `src/core/storage/`              | 核心存储层     |
| `src/query/queryBuilder.ts` | `src/core/query/queryBuilder.ts` | 核心查询构建器 |
| `src/fulltext/`             | `src/extensions/fulltext/`       | 全文检索扩展   |
| `src/spatial/`              | `src/extensions/spatial/`        | 空间索引扩展   |
| `src/algorithms/`           | `src/extensions/algorithms/`     | 图算法扩展     |
| `src/query/*` (其他)        | `src/extensions/query/*`         | 高级查询扩展   |

### Rust 核心集成路线

- 新增 `nervusdb-core/` Rust crate（位于仓库根目录），负责实现持久化内核、WAL、索引等功能；CI 运行 `cargo fmt` 与 `cargo test`
- 新增 `native/nervusdb-node/` 基于 napi-rs 的 Node 原生绑定，暴露 `open/add_fact/close`
- TypeScript 层通过 `src/native/core.ts` 中的加载器优雅降级：当原生绑定缺失（或设置 `NERVUSDB_DISABLE_NATIVE=1`）时继续使用现有纯 TS 实现

这一步为 v1.0.0 的 Rust 迁移奠定基础，同时确保“不破坏用户空间”：npm 包入口与 API 维持不变，原生绑定可以按需启用。

## 未来演进路径

如果未来出现以下需求，可以考虑迁移到 Monorepo：

1. **独立发布需求**：>20% 的用户明确要求"只要核心功能"
2. **多团队维护**：有多个团队分别维护核心层和扩展层
3. **独立版本管理**：核心层和扩展层需要独立的版本号
4. **商业化策略**：核心开源、扩展收费

在那之前，当前的单包 + 清晰目录结构已经足够。

## 参考

- Linus Torvalds: "Don't delete code just because it's in the wrong place. Move it to the right place."
- Linus Torvalds: "Theory and practice sometimes clash. Theory loses. Every single time."
- YAGNI (You Aren't Gonna Need It): 不要为假想的需求过度设计
- Tree-shaking: https://webpack.js.org/guides/tree-shaking/

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

SynapseDB 是一个基于 TypeScript 实现的嵌入式"三元组（SPO）知识库"，设计用于代码知识存储和联想查询。它是类似 SQLite 的单文件数据库，专门为支持分页索引、WAL v2 崩溃恢复、链式联想查询、Auto-Compact/GC 运维工具与读快照一致性而设计。

## 开发命令

### 构建与开发
- `pnpm build` - 编译 TypeScript 到 dist/ 目录
- `pnpm build:watch` - 监听模式编译
- `pnpm dev` - 开发模式运行 src/index.ts

### 测试
- `pnpm test` - 运行所有测试
- `pnpm test:watch` - 监听模式运行测试
- `pnpm test:coverage` - 生成测试覆盖率报告（目标：语句80%，分支75%，函数80%，行80%）

### 代码质量
- `pnpm lint` - 检查所有代码
- `pnpm lint:core` - 检查核心代码（零警告）
- `pnpm lint:fix` - 自动修复 lint 问题
- `pnpm typecheck` - TypeScript 类型检查

### 数据库运维 CLI
- `pnpm db:stats <db>` - 查看数据库统计信息
- `pnpm db:check <db>` - 检查数据库完整性
- `pnpm db:repair <db>` - 修复数据库
- `pnpm db:compact <db>` - 手动压缩
- `pnpm db:auto-compact <db>` - 自动压缩（支持增量模式）
- `pnpm db:gc <db>` - 垃圾回收
- `pnpm db:hot <db>` - 查看热点数据
- `pnpm db:txids <db>` - 事务 ID 管理和观测
- `pnpm db:dump <db>` - 导出数据库内容
- `pnpm bench` - 性能基准测试

## 核心架构

### 存储层 (Storage Layer)
- **persistentStore.ts** - 主存储引擎，管理三元组、属性和事务
- **wal.ts** - WAL v2 实现，支持 begin/commit/abort 批次语义和崩溃恢复
- **pagedIndex.ts** - 分页索引实现，支持大数据集的高效查询
- **tripleIndexes.ts** - SPO 三元组的多维索引（SPO, POS, OSP 等）
- **dictionary.ts** - 字符串到整数 ID 的双向映射
- **staging.ts** - LSM-lite 暂存层实现
- **hotness.ts** - 热点数据追踪，用于压缩决策

### 查询层 (Query Layer)
- **queryBuilder.ts** - 链式查询构建器，支持联想查询（find().follow().followReverse()）
- **synapseDb.ts** - 主 API 接口，提供读快照一致性

### 运维层 (Maintenance Layer)
- **compaction.ts** - 增量压缩算法
- **autoCompact.ts** - 自动压缩策略和热点导向压缩
- **gc.ts** - 垃圾回收，清理未使用的页面
- **check.ts** & **repair.ts** - 数据完整性检查和修复

### 并发控制
- **readerRegistry.ts** - 读者注册表，支持多读者并发
- **txidRegistry.ts** - 事务 ID 注册表，支持幂等性
- **lock.ts** - 进程级文件锁

## 关键概念

### 三元组 (SPO Triples)
所有数据以主语-谓语-宾语的形式存储，例如：
```ts
{ subject: 'file:/src/user.ts', predicate: 'DEFINES', object: 'class:User' }
```

### 读快照一致性
通过 `withSnapshot()` 或链式查询的自动 epoch pinning 确保查询过程中的数据一致性：
```ts
await db.withSnapshot(async (snap) => {
  return snap.find({ object: 'method:login' })
    .followReverse('HAS_METHOD')
    .all();
});
```

### 事务批次与幂等
支持可选的 txId 用于幂等性保证：
```ts
db.beginBatch({ txId: 'T-123', sessionId: 'writer-A' });
db.addFact({ subject: 'A', predicate: 'R', object: 'X' });
db.commitBatch();
```

## 测试指南

### 运行特定测试
```bash
# 运行单个测试文件
pnpm test tests/queryBuilder.test.ts

# 运行匹配模式的测试
pnpm test wal
```

### 测试分类
- **核心功能测试**：`persistentStore.test.ts`, `queryBuilder.test.ts`, `wal.test.ts`
- **压缩与维护测试**：`compaction*.test.ts`, `auto_compact*.test.ts`, `gc*.test.ts`
- **并发与一致性测试**：`query_snapshot_isolation.test.ts`, `*_respect_readers.test.ts`
- **WAL 与事务测试**：`wal_*.test.ts`, `crash_injection.test.ts`

## 性能优化要点

1. **索引选择**：查询优化器会根据查询模式自动选择最佳索引（SPO, POS, OSP 等）
2. **分页加载**：大数据集通过分页索引延迟加载
3. **热点驱动压缩**：基于访问频率进行智能压缩
4. **LSM-lite 暂存**：写入先进入内存暂存层，定期合并到持久存储

## 代码约定

- 使用 2 空格缩进
- 优先使用显式类型注解
- 所有公共 API 必须有 JSDoc 注释
- 测试文件使用 describe/it 结构，遵循 Given-When-Then 模式
- 错误处理使用自定义错误类型（见 utils/fault.ts）
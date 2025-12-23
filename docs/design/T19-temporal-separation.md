# T19: temporal_v2 分离为独立 crate

## 1. Context

`nervusdb-core/src/temporal_v2.rs` (50KB) 包含了 AI Memory / Episode 相关的业务逻辑：
- `StoredEpisode`, `StoredEntity`, `StoredFact`
- `trace_hash`, `fingerprint` 去重逻辑
- 时间线查询

这些是特定于 AI Agent 记忆系统的业务逻辑，不应该在通用图数据库核心中。

## 2. Goals

- 将 `temporal_v2.rs` 移出 `nervusdb-core`
- 创建独立的 `nervusdb-temporal` crate
- `nervusdb-core` 保持纯粹的图存储功能

## 3. Proposed Solution

### 目录结构

```
nervusdb/
├── nervusdb-core/          # 纯图存储
├── nervusdb-temporal/      # AI Memory 扩展 (新)
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs          # 从 temporal_v2.rs 迁移
├── bindings/
│   └── node/
│       └── native/
│           └── nervusdb-node/
│               └── Cargo.toml  # 可选依赖 nervusdb-temporal
```

### 依赖关系

```
nervusdb-temporal
├── redb (直接依赖)
├── serde, bincode
└── time

nervusdb-core
├── redb
└── [optional] nervusdb-temporal  # feature = "temporal"

nervusdb-node
├── nervusdb-core
└── [optional] nervusdb-temporal  # feature = "temporal"
```

### 迁移步骤

1. 创建 `nervusdb-temporal/Cargo.toml`
2. 移动 `temporal_v2.rs` 到 `nervusdb-temporal/src/lib.rs`
3. 定义独立的 `Error` 类型
4. 修改 `nervusdb-core` 的 feature gate
5. 更新 Node.js 绑定

## 4. Risks

- 破坏性变更：使用 temporal feature 的用户需要更新依赖
- 需要同步更新 Node.js 和 Python 绑定

## 5. Status

**Plan** - 等待 v1.1 发布后实施

# T41: v2 Workspace / Crate 结构与边界

## 1. Context

v2 明确为“全新内核”（不兼容 v1），但仓库当前已包含 v1 的 Rust core、CLI、Node/Python/UniFFI/WASM 绑定以及 CI/发布流程。要避免在实现 M0/M1 时被 v1 的历史包袱拖死，必须先把 v2 的代码边界、crate 命名、feature gate、与 bindings 的演进路径写清。

## 2. Goals

- v2 在仓库内独立演进：新 crate / 新 API / 新磁盘格式（`.ndb + .wal`）
- v1 保持可构建、可发布，不被 v2 打断（即便未来不再维护，也不能被搞烂）
- v2 的实现顺序去风险：先 Rust API + CLI，再绑定层
- 清晰的边界：存储/查询/公共 API 分层明确，避免循环依赖

## 3. Non-Goals（T41 不做）

- 不在此阶段重构/抽离 v1 的 parser/planner 到共享 crate（风险太高、收益太早）
- 不在此阶段定义稳定 ABI（C/Node/Python）——等 v2 M2 之后再冻结契约
- 不在此阶段实现任何 v2 代码（T41 只定义结构与约束）

## 4. Repo Layout Proposal

### 4.1 Workspace 顶层

保留现有 workspace，新增 v2 crates（命名带 `-v2`，避免误用）。

```
Cargo.toml (workspace)
  nervusdb-core/          # v1 (existing)
  nervusdb-cli/           # v1 CLI (existing)
  nervusdb-temporal/      # v1 optional (existing)
  nervusdb-wasm/          # v1 wasm (existing)
  nervusdb-v2/            # v2 public facade (new)
  nervusdb-v2-storage/    # v2 pager + wal + lsm graph (new)
  nervusdb-v2-query/      # v2 cypher frontend + planner/executor (new)
  nervusdb-v2-cli/        # v2 cli (new)
bindings/
  ...                     # v1 bindings (existing)
  v2/                     # v2 bindings (future)
docs/design/              # design docs
```

### 4.2 为什么要拆 `storage/query/facade`

- `nervusdb-v2-storage`：最底层、最难写对、最需要测试保护。必须不依赖 query。
- `nervusdb-v2-query`：依赖 storage 的 trait（`GraphStore`），实现 AST/Planner/Executor。
- `nervusdb-v2`：对外稳定入口（`Connection/Transaction/Statement`），只做组合与薄封装。

## 5. API Boundaries（最小接口）

### 5.1 `nervusdb-v2-storage` 对外 trait

v2 query 不该直接碰 pager/wal 细节，只依赖一个最小图存储接口（随 M0/M1/M2 递进扩展）：

- `begin_read() -> Snapshot`
- `begin_write() -> WriteTxn`
- `Snapshot::neighbors(src_iid, rel_type?) -> EdgeIter`
- `WriteTxn::create_node(external_id, label) -> internal_id`
- `WriteTxn::create_edge(src_iid, rel_type, dst_iid)`
- `commit()/abort()`

> 注意：属性与 schema 在 M1 只存在于 WAL/MemTable；接口不要过早把 columnar/typed props 暴露出去。

### 5.2 `nervusdb-v2-query` 的依赖策略

短期（M1/M2）建议：

- 直接复制 v1 的 Cypher parser/AST/planner 到 `nervusdb-v2-query`（保持独立）
- 等 v2 稳定后再考虑抽成共享 crate（例如 `nervusdb-cypher`），避免 early refactor 把 v1 搞坏

## 6. Feature Gates / Target Policy

### 6.1 Native vs WASM

- `nervusdb-v2-storage`：
  - `cfg(not(target_arch = "wasm32"))`：提供磁盘引擎（pager+wal）
  - `cfg(target_arch = "wasm32")`：不提供磁盘格式，仅提供 in-memory 实现或直接不编译（由 facade 做选择）
- `nervusdb-v2`：对 wasm 提供与 native 等价的 API，但后端选择 in-memory

### 6.2 Background Compaction

默认只提供显式 `db.compact()`；后台线程 compaction 作为可选 feature（例如 `background-compaction`），由嵌入式宿主自行决定是否开启。

### 6.3 Durability

暴露 `Durability` 配置（默认 `Full`，每 commit fsync）。该配置属于 v2 public API 的稳定面之一，尽早设计清楚但不在 T41 实现。

## 7. Build / Test Strategy（CI 影响）

- v1 现有 CI 不应因 v2 新增而变慢或不稳定
- v2 先增加最小测试集：
  - `nervusdb-v2-storage`：WAL replay / crash consistency / snapshot isolation
  - `nervusdb-v2-query`：小集合 e2e（MATCH expand + filter）
- crash 类测试要可控：默认在 CI 跑小次数，长时间 fuzz/stress 用手动 job 或 nightly

## 8. Migration / Compatibility（明确“不做”）

v2 不提供从 v1 数据文件迁移的承诺；未来如果需要，只做独立工具，不把迁移逻辑塞进 v2 内核。

## 9. Risks

- crate 拆分过早会带来样板代码，但比循环依赖/边界混乱更便宜
- 复用 v1 parser/planner 的方式（复制 vs 抽共享 crate）要控制节奏；M1 不做共享抽离是降风险选择


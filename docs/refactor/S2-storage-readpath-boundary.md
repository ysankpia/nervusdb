# S2：Storage 读路径边界治理（结构解耦）

更新时间：2026-02-13  
任务类型：Phase 2  
任务状态：In Progress

## 1. 目标

- 在不改语义前提下，对读路径热点职责做结构解耦。
- 降低锁热点与快照桥接的耦合密度。
- 为后续性能阶段（Phase 2/Phase 3）提供清晰边界。

## 2. 边界

- 允许：内部模块切分、辅助结构抽取、读路径整理。
- 禁止：存储格式变更、事务语义变更、外部 API 变更。
- 禁止：写路径逻辑重写。

## 3. 文件清单

### 3.1 必改文件

- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/engine.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/snapshot.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/api.rs`

### 3.2 可选新增

- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path/mod.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path/scanner.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path/materialize.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_overlay.rs`

## 4. 证据与前置

- engine 体量与复杂度高：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/engine.rs:1`
- 任务路径已在架构路线中定义：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-Architecture.md:1185`
- 需在 Phase 1a/1c 之后执行：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-Architecture.md:1179`

## 5. 测试清单

- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/tests/t51_snapshot_scan.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/tests/m1_graph.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/tests/t47_api_trait.rs`
- `bash scripts/workspace_quick_test.sh`

## 6. 回滚步骤

1. 读一致性/快照一致性失败，立即回滚 PR。
2. 回滚后增加最小复现测试，再拆分更细任务重做。

## 7. 完成定义（DoD）

- 读路径职责边界明确，模块耦合下降。
- 快照相关回归全绿。
- 不引入外部 API 与语义变化。

## 8. 当前进展（2026-02-12）

1. 已完成切片-1（Snapshot 属性读取 helper 抽离）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_overlay.rs`。
   - 将 `Snapshot::{node_property, edge_property, node_properties, edge_properties}` 的遍历/合并逻辑迁移到 helper 函数：
     `node_property_from_runs / edge_property_from_runs / merge_node_properties_from_runs / merge_edge_properties_from_runs`。
   - `Snapshot` 对外方法签名保持不变，仅改内部调用路径。
2. 新增模块级回归测试 4 条（固定当前语义）
   - `node/edge` 属性读取优先级与 tombstone 行为。
   - `node/edge` 属性 map 合并时“新 run 覆盖旧 run”行为。
3. 本轮验证通过
   - `cargo test -p nervusdb-v2-storage read_path_overlay --lib`
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

4. 已完成切片-2（Snapshot 邻接迭代器重复逻辑收敛）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_neighbors.rs`。
   - 将 `NeighborsIter/IncomingNeighborsIter` 中重复的 run tombstone 收集、run/segment 边加载、边屏蔽判定下沉到共享 helper：
     `apply_run_tombstones / load_outgoing_run_edges / load_incoming_run_edges / load_outgoing_segment_edges / load_incoming_segment_edges / edge_blocked_outgoing / edge_blocked_incoming`。
   - `L0Run::{edges_for_src, edges_for_dst}` 调整为 `pub(crate)`，仅用于 crate 内 helper 调用，不影响外部 API。
   - 迭代器外部行为与签名保持不变。
5. 切片-2 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_neighbors --lib`
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`

6. 已完成切片-3（邻接迭代器模块下沉）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_iters.rs`。
   - 将 `NeighborsIter / IncomingNeighborsIter` 结构体与迭代实现从 `snapshot.rs` 搬移到独立模块，`Snapshot::{neighbors,incoming_neighbors}` 返回类型保持不变。
   - `snapshot.rs` 中 `L0Run::{edges_for_src, edges_for_dst}` 提升为 `pub(crate)` 以支持 crate 内迭代器模块访问；无外部可见 API 变化。
7. 切片-3 验证通过
   - `cargo test -p nervusdb-v2-storage --lib`
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`

8. 已完成切片-4（属性值到 API 值转换下沉）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_convert.rs`。
   - 将 `snapshot.rs` 中 `convert_property` 递归转换逻辑迁移到 `read_path_convert`：
     `convert_property_to_api / convert_property_map_to_api`。
   - `GraphSnapshot` trait 实现签名不变，仅由 `snapshot.rs` 转为调用 helper，保留行为等价。
   - 新增模块测试 2 条，固定嵌套 `List/Map` 与标量 `DateTime/Blob` 的转换语义。
9. 切片-4 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_convert --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --lib`
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

10. 已完成切片-5（EdgeKey 转换逻辑收敛）
   - 在 `read_path_convert` 新增 helper：
     `api_edge_to_internal / internal_edge_to_api`。
   - `snapshot.rs` 中 `GraphSnapshot::{edge_property, edge_properties}` 及 `ApiNeighborsIter` 不再手写字段映射，统一走转换 helper。
   - 对外签名与返回语义不变，仅减少重复逻辑和转换分散点。
   - 新增转换测试 2 条，锁定字段等价映射行为。
11. 切片-5 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_convert --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

12. 已完成切片-6（节点 tombstone 判定 helper 下沉）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_nodes.rs`。
   - 抽离 `is_tombstoned_node_in_runs`，统一负责 run 层 tombstone 判定。
   - `Snapshot::{nodes, is_tombstoned_node}` 改为调用 helper；删除 `nodes()` 中冗长内联判定注释块，保留原判定语义。
   - 新增模块测试 2 条，锁定“任一 run tombstone 即删除”的当前行为。
13. 切片-6 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_nodes --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

14. 已完成切片-7（API 邻接迭代器适配器下沉）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_api_iter.rs`。
   - 将 `ApiNeighborsIter` 结构与转换实现从 `snapshot.rs` 下沉为独立模块，`snapshot.rs` 仅保留 `GraphSnapshot` 组装调用。
   - 新增模块测试 1 条，锁定内部 `EdgeKey` 到 API `EdgeKey` 的迭代转换行为。
   - 修复一次可见性回归（`E0446`）：`ApiNeighborsIter` 作为 `GraphSnapshot` 关联类型需保持公开。
15. 切片-7 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_api_iter --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

16. 已完成切片-8（标签解析 helper 下沉）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_labels.rs`。
   - 将 `Snapshot` 的标签相关访问逻辑下沉为纯函数 helper：
     `node_primary_label / node_all_labels / resolve_label_id / resolve_label_name`。
   - `snapshot.rs` 中 `node_label / node_labels / resolve_*` 系列方法改为调用 helper，`GraphSnapshot` 对外行为与签名保持不变。
   - 新增模块测试 2 条，固定节点标签读取与 label id/name 解析语义。
17. 切片-8 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_labels --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

18. 已完成切片-9（L0Run 属性判定 helper 下沉）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_run_props.rs`。
   - 将 `L0Run::{node_property, edge_property}` 内部的 tombstone 判定和属性读取下沉到：
     `node_property_in_run / edge_property_in_run`。
   - `L0Run` 对外方法签名不变，仅改为调用 helper。
   - 新增模块测试 2 条，锁定 node/edge 在 run 内部的 tombstone 覆盖行为与 live value 行为。
19. 切片-9 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_run_props --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

20. 已完成切片-10（L0Run 状态判定 helper 下沉）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_run_state.rs`。
   - 将 `L0Run::{is_empty, has_properties}` 的判定逻辑下沉到：
     `run_is_empty / run_has_properties`。
   - `snapshot.rs` 中 `L0Run` 对外方法签名不变，仅改为委托 helper，保持行为等价。
   - 新增模块测试 2 条，锁定 run 内部“空态判定”与“属性桶存在判定”语义。
21. 切片-10 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_run_state --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

22. 已完成切片-11（活节点迭代逻辑下沉）
   - 扩展模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_nodes.rs`。
   - 新增 helper：`live_node_ids(max_id, runs)`，将 `Snapshot::nodes` 的“按节点上界迭代 + tombstone 过滤”逻辑下沉到模块层。
   - `snapshot.rs` 中 `nodes()` 对外签名保持不变，仅改为委托 `live_node_ids`。
   - 新增模块测试 1 条，锁定活节点过滤行为（被 tombstone 的节点不会出现在节点迭代中）。
23. 切片-11 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_nodes --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

24. 已完成切片-12（L0Run 边索引读取 helper 下沉）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_run_edges.rs`。
   - 新增 helper：`edges_for_src / edges_for_dst`，承接 `L0Run` 中按 `src/dst` 读取边桶并在缺失时返回空切片的逻辑。
   - `snapshot.rs` 中 `L0Run::{edges_for_src, edges_for_dst}` 对外签名保持不变，内部改为委托 helper。
   - 新增模块测试 2 条，锁定命中与 miss 两类读取路径语义。
25. 切片-12 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_run_edges --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

26. 已完成切片-13（L0Run 迭代器 helper 下沉）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_run_iters.rs`。
   - 新增 helper：`iter_edges / iter_tombstoned_nodes / iter_tombstoned_edges`，承接 `L0Run` 的边与 tombstone 迭代逻辑。
   - `snapshot.rs` 中 `L0Run::{iter_edges, iter_tombstoned_nodes, iter_tombstoned_edges}` 对外签名不变，内部改为委托 helper。
   - 新增模块测试 2 条，锁定边桶 flatten 行为与 tombstone 集合迭代行为。
27. 切片-13 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_run_iters --lib`（编译级红灯后转绿）
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

28. 已完成切片-14（L0Run 属性桶读取 helper 下沉）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_run_property_maps.rs`。
   - 新增 helper：`node_properties_in_run / edge_properties_in_run`，承接 `L0Run` 对 node/edge 属性桶的只读查询逻辑。
   - `snapshot.rs` 中 `L0Run::{node_properties, edge_properties}` 对外签名保持不变，内部改为委托 helper。
   - 新增模块测试 2 条，锁定命中桶返回 `Some`、缺失桶返回 `None` 的行为。
29. 切片-14 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_run_property_maps --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

30. 已完成切片-15（Label/RelType 符号解析路径收敛）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_symbols.rs`。
   - `Snapshot::{resolve_label_id, resolve_rel_type_id, resolve_label_name, resolve_rel_type_name}` 已改为委托统一 helper。
   - 解析实现复用 `read_path_labels` 既有 helper，避免新增重复语义分支。
31. 切片-15 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_symbols --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t59_label_interning`
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

32. 已完成切片-16（Snapshot 统计读取 helper 下沉）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_stats.rs`。
   - 将 `Snapshot::get_statistics` 的 “stats_root=0 返回默认值 + 读取 blob + decode + corrupted 报错” 逻辑下沉为：
     `read_statistics(pager, stats_root)`。
   - 新增模块测试 3 条，覆盖 root=0、正常 roundtrip、损坏 payload 报错三种路径。
33. 切片-16 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_stats --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `cargo test -p nervusdb-v2 --test t156_optimizer`
   - `bash scripts/workspace_quick_test.sh`

34. 已完成切片-17（GraphSnapshot 属性桥接 helper 下沉）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_api_props.rs`。
   - 将 `GraphSnapshot` 的四个属性桥接方法收敛为 helper：
     `node_property_as_api / edge_property_as_api / node_properties_as_api / edge_properties_as_api`。
   - `snapshot.rs` 中 `GraphSnapshot` trait impl 对外签名保持不变，仅委托 helper，避免重复转换逻辑散落。
   - 新增模块测试 2 条，锁定 node/edge 的单值与 map 转换路径语义。
35. 切片-17 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_api_props --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2 --test t53_integration_storage`
   - `bash scripts/workspace_quick_test.sh`

36. 已完成切片-18（持久化属性树读取 helper 下沉 + api.rs 读路径收敛）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_property_store.rs`。
   - 将 `properties_root` 的 BTree/BlobStore 查询逻辑抽离为 helper：
     `read_node_property_from_store / read_edge_property_from_store / extend_node_properties_from_store / extend_edge_properties_from_store`。
   - `api.rs` 改为委托 helper，并复用 `read_path_convert` 的转换函数：
     `internal_edge_to_api / api_edge_to_internal / convert_property_to_api / convert_property_map_to_api`，删除重复转换实现点。
   - 新增模块测试 4 条，覆盖 node/edge 单值读取与 map 扩展“既有 key 不覆盖”的语义。
37. 切片-18 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_property_store --lib`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

38. 已完成切片-19（StorageSnapshot 统计计数路径收敛）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_api_stats.rs`。
   - 先补失败测试，再实现计数 helper：
     `node_count_from_stats / edge_count_from_stats`，固定 total/bucket/miss/null-cache 语义。
   - `api.rs` 中 `node_count/edge_count` 改为复用 helper，并通过 `ensure_stats_cache_loaded` 统一缓存装载逻辑，去除重复分支。
   - 对外 `GraphSnapshot` 接口与返回语义保持不变。
39. 切片-19 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_api_stats --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

40. 已完成切片-20（Engine 读视图装配 helper 下沉）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_engine_view.rs`。
   - 先补失败测试，再实现：
     `load_properties_and_stats_roots / build_snapshot_from_published`。
   - `engine.rs` 的 `begin_read` 与 `checkpoint_on_close` 改为复用 helper，收敛 roots 读取与 snapshot 组装路径，保持现有行为与对外接口不变。
41. 切片-20 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_engine_view --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `cargo test -p nervusdb-v2 --test t106_checkpoint_on_close`
   - `bash scripts/workspace_quick_test.sh`

42. 已完成切片-21（GraphStore snapshot tombstone 收敛）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_tombstones.rs`。
   - 先补失败测试，再实现 helper：`collect_tombstoned_nodes`，统一 run 集合上的 tombstone 节点聚合逻辑。
   - `api.rs` 的 `GraphStore::snapshot()` 改为委托 helper，减少读路径初始化中的内联遍历逻辑。
   - 对外 `StorageSnapshot` 与 `GraphSnapshot` 接口行为保持不变。
43. 切片-21 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_tombstones --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

44. 已完成切片-22（API->Storage 转换逻辑集中）
   - 在 `read_path_convert.rs` 新增 `convert_property_to_storage`，并补充红绿测试覆盖嵌套 `Map/List` 转换。
   - `api.rs` 的索引查找路径改为复用 `convert_property_to_storage`，删除本地重复 `to_storage` 实现点。
   - `engine.rs` 的索引更新路径同步切换到同一 helper，消除对已移除转换函数的依赖。
45. 切片-22 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_convert --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

46. 已完成切片-23（Engine 标签读取 helper 下沉）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_engine_labels.rs`。
   - 先补失败测试，再实现：
     `published_label_snapshot / lookup_label_id / lookup_label_name`。
   - `engine.rs` 中 `label_snapshot/get_label_id/get_label_name` 改为统一委托 helper，读路径锁访问逻辑集中化。
   - 对外接口行为保持不变。
47. 切片-23 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_engine_labels --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t59_label_interning`
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

48. 已完成切片-24（Engine IdMap 快照读取 helper 下沉）
   - 新增模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_engine_idmap.rs`。
   - 先补失败测试，再实现 `read_i2e_snapshot`，统一封装 `Mutex<IdMap>` 的快照读取逻辑，固定返回 owned copy 语义。
   - `engine.rs` 中 `scan_i2e_records` 改为统一委托 helper，读路径锁访问点进一步集中；对外 API 与返回值语义不变。
49. 切片-24 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_engine_idmap --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

50. 已完成切片-25（Engine IdMap 读取入口进一步收敛）
   - 扩展模块：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-v2-storage/src/read_path_engine_idmap.rs`。
   - 先补失败测试，再实现 `lookup_internal_node_id / read_i2l_snapshot`，统一封装 `IdMap` 查找与标签快照读取。
   - `engine.rs` 的 `lookup_internal_id` 与 `update_published_node_labels` 已改为委托 helper，保持外部 API 与语义不变。
51. 切片-25 验证通过
   - `cargo test -p nervusdb-v2-storage read_path_engine_idmap --lib`（先红后绿）
   - `cargo test -p nervusdb-v2-storage --test t47_api_trait`
   - `cargo test -p nervusdb-v2-storage --test t51_snapshot_scan`
   - `cargo test -p nervusdb-v2-storage --test m1_graph`
   - `bash scripts/workspace_quick_test.sh`

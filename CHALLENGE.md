# GraphLite-RS 生产级工程落地挑战：突破缓冲池物理瓶颈与事务自愈

## 1. 项目愿景与架构定位
GraphLite-RS 定位为 **“图数据库界的 SQLite”**：
- 嵌入式、单文件（主数据文件 `{path}` + 页级 WAL `{path}.wal`）；
- 纯磁盘外存架构（Zero In-Memory Graph，物理内存受 `BufferPoolManager` 严格硬约束，如 256 帧=1MB，512 帧=2MB）；
- 支持完整的 ACID 事务、Cypher 查询引擎、无索引直接物理寻址邻接表（Index-free Adjacency）。

## 2. 现状诊断与核心矛盾
当前在仓库根目录直接运行测试：
```bash
cargo test
```
共有 23 个测试通过，但有 3 个涉及底层存储引擎核心架构的关键测试失败：
1. `test_07_buffer_pool_eviction_large_graph_stress`
   - 报错：`Buffer pool capacity exceeded: all frames are uncommitted dirty pages (NO-STEAL enforced)`
2. `test_10_pure_out_of_core_stress`
   - 报错：`Buffer pool capacity exceeded: all frames are uncommitted dirty pages (NO-STEAL enforced)`
3. `test_tx_commit_failure_memory_cleanup`
   - 报错：`InvalidWeight(-5.0)` 直接中断了事务构造流程，未能完成预期的 commit 阶段失败回滚与脏内存回收验证。

### 核心架构痛点（也是此前各方争论的核心）：
在 `src/buffer.rs` 中，目前强制落实了最严苛的 `NO-STEAL` 淘汰策略：
所有属于当前未提交事务的脏页（`uncommitted_pages`）绝对禁止被 Buffer Pool 置换淘汰。
然而，在 `test_07`（20,000 节点大事务，缓冲池仅 512 帧=2MB）和 `test_10`（10,000 节点大事务，缓冲池仅 256 帧=1MB）中：
**单个大事务所产生/修改的物理页数量，远超缓冲池的总物理帧容量！**
在死守纯内存 `NO-STEAL` 且未提供事务级外存暂存机制的情况下，Buffer Pool 必然发生物理帧耗尽异常。

## 3. 你的任务目标（达到上生产标准）
1. **彻底解决大事务与受限缓冲池的冲突**：
   - 保证严格的内存硬约束（内存占用不得突破缓冲池容量限制，禁止粗暴放大内存）；
   - 支持单事务修改超过缓冲池容量的大图写入（可参考 SQLite WAL 的 Steal 机制、事务级 Spool 临时溢出暂存、或增量安全刷盘）；
   - 必须满足事务的回滚一致性（发生 rollback 或 crash 时，未提交数据绝不能污染主库文件，保证 ACID）。
2. **修复事务提交失败回滚与内存清理边界**（`test_tx_commit_failure_memory_cleanup`）：
   - 确保非法操作能进入 commit 阶段并在 apply 阶段校验失败，且失败后脏页被干净 discard，主库零污染。
3. **通过全量回归测试**：
   - 运行 `cargo test`，确保全部 26 个集成测试 **100% 全部通过**。
   - 运行 `cargo check --workspace`，保证零编译 Warning、零 Error。
4. **保持代码架构优雅**：
   - 严格遵循 `AGENTS.md` 中的所有非妥协性原则（Non-Negotiable Invariants）。
   - 严禁为了凑测试而做任何 Hardcode，严禁引入内存全局图缓存破环外存设计。

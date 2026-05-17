# T206 Implementation Plan: B-Tree Incremental Delete

## 1. Overview

当前 B-Tree 删除实现 (`delete_exact_rebuild`) 采用**全量扫描 + 重建**策略：

1. `scan_all()` 遍历所有叶子页，读取全部 KV 对到内存
2. 在内存中删除目标 key
3. `build_from_sorted_entries()` 重新分配页并写回所有数据

这在小索引时可接受，但当索引规模增长时（如 >10K 条目），每次删除的 I/O 开销线性增长，会导致严重的写放大和延迟抖动。

### 目标

实现**增量删除**：仅修改受影响的叶子页，避免重建整棵树。

---

## 2. Requirements Analysis

### 2.1 Use Scenarios

1. `DELETE n WHERE n.id = x` 触发属性索引更新（删除旧值、插入新值）
2. `SET n.prop = newVal` 时需删除旧索引条目
3. 批量删除节点时可能连续删除多个索引条目

### 2.2 Functional Requirements

- [ ] 删除单个 key 时只修改对应叶子页
- [ ] 叶子页为空时标记为"可回收"（bitmap free）
- [ ] 叶子页 underflow 时暂不合并（MVP）
- [ ] 保持页面排序不变性

### 2.3 Performance Goals

- 删除单 key: O(log N) 页读 + O(1) 页写（最坏 2 页）
- 无写放大（相比当前 O(N) 页写）

---

## 3. Design

### 3.1 Core Approach: Leaf-Only Delete

**策略**：定位到叶子页后，原地删除 cell，不触发 rebalance。

```
delete_in_place(key, payload):
  1. 从 root 向下查找到 leaf 页
  2. 在 leaf 页中定位 cell（binary search）
  3. 如果匹配：删除 cell（shift slots left）
  4. 如果 leaf 变空：free_page(leaf_id) 并更新父节点（可选，MVP 不做）
  5. 写回 leaf 页
```

### 3.2 Page Layout Changes

当前 slotted-page 布局支持原地删除：

- `cell_count -= 1`
- Shift slot array left
- Cell content 区域可留空洞，暂不 compact

### 3.3 API Design

```rust
impl BTree {
    /// Incremental delete: O(log N) reads, O(1) writes.
    /// Returns true if key was found and deleted.
    pub fn delete(&mut self, pager: &mut Pager, key: &[u8], payload: u64) -> Result<bool>;

    // Internal helper
    fn delete_from_leaf(page: &mut Page, idx: usize) -> Result<()>;
}
```

### 3.4 Deferred Work (Not in Scope)

- **Page merge/rebalance**：当 leaf 使用率 <50% 时合并兄弟页。复杂度高，MVP 暂不实现。
- **Internal page cleanup**：当 child 页被删除后更新父指针。MVP 允许 tombstone leaf 存在。

---

## 4. Implementation Plan

### Step 1: Implement `delete_from_leaf` (Risk: Low)

- File: `nervusdb-storage/src/index/btree.rs`
- 添加 `fn shift_slots_left(&mut self, idx: usize)` 方法
- 添加 `fn delete_from_leaf(&mut self, page: &mut Page, idx: usize) -> Result<()>`

### Step 2: Implement `BTree::delete` (Risk: Medium)

- File: `nervusdb-storage/src/index/btree.rs`
- 复用 `insert` 的 path 下降逻辑
- 找到 leaf 后调用 `delete_from_leaf`

### Step 3: Update call sites (Risk: Low)

- File: `nervusdb-storage/src/engine.rs:885`
- 替换 `delete_exact_rebuild` 为 `delete`

### Step 4: Deprecate old method (Risk: Low)

- 保留 `delete_exact_rebuild` 但标记 `#[deprecated]`
- 添加 warning 日志

---

## 5. Verification Plan

### 5.1 Unit Tests

```bash
cd nervusdb-storage && cargo test btree -- --nocapture
```

新增测试用例：

- `delete_single_key_incremental`：插入 10 个 key，删除 1 个，验证其余 9 个存在
- `delete_all_keys_one_by_one`：依次删除所有 key，验证最终 tree 为空

### 5.2 Integration Tests

```bash
cargo test --package nervusdb-storage --test index_integration
```

### 5.3 回归测试

确保现有 `t204_vacuum_reclaims_orphan_blob_pages` 仍通过。

### 5.4 Manual Verification

1. 创建 1000 节点图
2. 删除 100 节点
3. 对比删除前后 `.ndb` 文件大小变化（应远小于当前实现）

---

## 6. Risk Assessment

| Risk Description             | Impact Level | Mitigation Measures                |
| ---------------------------- | ------------ | ---------------------------------- |
| 删除后页面碎片化导致空间浪费 | Medium       | VACUUM 已实现，可定期回收          |
| 空叶子页未释放导致内存膨胀   | Low          | Step 2 中 free 空页                |
| Internal 节点指向已删除叶子  | Low          | MVP 接受此 trade-off，后续版本处理 |

---

## 7. Out of Scope

- Page compaction / defragmentation
- Internal node rebalancing
- 并发删除（当前 Single Writer 模型）

---

## 8. Future Extensions

- **Lazy Page Reclaim**：删除后延迟 free，避免频繁 bitmap 更新
- **Bulk Delete Optimization**：批量删除使用 merge-rebuild 策略

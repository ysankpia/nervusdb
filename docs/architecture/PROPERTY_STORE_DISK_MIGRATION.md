# PropertyStore 磁盘中心改造计划

**Issue**: #7  
**优先级**: P2 (中优先级)  
**状态**: 已验证需求，计划实施  
**日期**: 2025-10-17

---

## 📊 当前架构分析

### 测试数据

通过可扩展性测试得出以下结果：

| 节点数 | 属性数据大小 | 启动时间 | 增长倍数 |
| ------ | ------------ | -------- | -------- |
| 1,000  | 0.85 MB      | 9ms      | 1x       |
| 5,000  | 4.28 MB      | 33ms     | 3.67x    |
| 10,000 | 8.57 MB      | 59ms     | 6.56x    |

**结论：启动时间呈现 O(N) 线性增长特征**

### 当前实现

```typescript
// src/storage/persistentStore.ts:121
const propertyStore = PropertyStore.deserialize(sections.properties);
```

**问题**：

1. PropertyStore.deserialize 全量加载属性数据到内存
2. 数据库容量受限于可用内存
3. 启动时间随属性数据量线性增长

### 但是...性能尚可

- ✅ 8.57MB 属性数据仅需 59ms 启动
- ✅ 属性查询延迟 < 1ms
- ✅ 属性索引已支持高效查询

---

## 🎯 改造目标

### 1. 架构目标

将 PropertyStore 改造为磁盘中心模型，类似 TripleStore 的处理：

```typescript
// 当前（全量加载）
const propertyStore = PropertyStore.deserialize(sections.properties);

// 目标（空实例 + 磁盘索引）
const propertyStore = new PropertyStore(); // 空实例，仅用于增量
```

### 2. 性能目标

| 指标                | 当前        | 目标                |
| ------------------- | ----------- | ------------------- |
| 启动时间（10K节点） | 59ms (O(N)) | <20ms (O(1))        |
| 属性查询延迟        | <1ms        | <2ms (允许小幅增加) |
| 内存占用            | 8.57MB      | <1MB                |

---

## 📋 实施计划

### 阶段 1：扩展 PropertyIndexManager（3天）

**目标**：使其成为属性的事实之源（Source of Truth）

**当前**：

- MemoryPropertyIndex：仅存储倒排索引（属性名->值->ID集合）
- 不支持正向查询（ID->属性值）

**改造**：

```typescript
class PropertyIndexManager {
  private readonly forwardIndex: Map<number, Buffer>; // 正向索引：nodeId -> 属性数据
  private readonly inverseIndex: MemoryPropertyIndex; // 倒排索引

  // 新增：通过 ID 读取属性
  async getNodeProperties(nodeId: number): Promise<Record<string, unknown> | undefined> {
    // 1. 检查内存缓存
    if (this.forwardIndex.has(nodeId)) {
      return deserialize(this.forwardIndex.get(nodeId)!);
    }

    // 2. 从磁盘加载
    const data = await this.loadPropertyFromDisk(nodeId);
    if (data) {
      this.forwardIndex.set(nodeId, data); // 缓存
      return deserialize(data);
    }

    return undefined;
  }

  // 新增：磁盘分页存储
  private async loadPropertyFromDisk(nodeId: number): Promise<Buffer | undefined> {
    // 类似 PagedIndex 的分页读取
    const pageId = Math.floor(nodeId / PAGE_SIZE);
    const page = await this.loadPage(pageId);
    return page.get(nodeId);
  }
}
```

### 阶段 2：修改 PersistentStore.open（1天）

```typescript
// 不再反序列化 PropertyStore
const propertyStore = new PropertyStore(); // 空实例

// PropertyIndexManager 从磁盘加载索引
await store.propertyIndexManager.loadFromDisk();
```

### 阶段 3：修改属性读写路径（2天）

**当前路径**：

```
getNodeProperties(id)
  → this.properties.getNodeProperties(id)
  → 从内存 Map 读取
```

**目标路径**：

```
getNodeProperties(id)
  → this.propertyIndexManager.getNodeProperties(id)
  → 检查内存缓存
  → 从磁盘分页加载
  → 更新缓存
```

### 阶段 4：测试与验证（2天）

1. 所有现有属性测试通过
2. 性能测试验证 O(1) 启动时间
3. 内存使用降低验证
4. 崩溃恢复测试

**总计**：约 8 天工作量

---

## 🚧 当前状态

### Issue #5 (P0) ✅ 完成

- PersistentStore.open 不再全量加载 TripleStore
- 查询引擎从磁盘 PagedIndex 读取
- 启动时间：10K 三元组 55ms

### Issue #6 (P1) ✅ 完成

- FlushManager 实现增量持久化
- PagedIndex 追加而非重写
- Flush 时间：O(1) 复杂度（变异系数 3.3%）

### Issue #7 (P2) 📝 已验证需求

- PropertyStore 仍然全量加载（O(N)）
- 性能可接受但有改进空间
- 改造计划已制定

---

## 💡 建议

### 选项 A：立即实施（推荐）

**理由**：

- 完成整个阶段一的架构升级
- 彻底消除内存瓶颈
- 为未来大规模应用做准备

**时间**：约 2 周

### 选项 B：延后实施

**理由**：

- 当前性能尚可（59ms for 8.57MB）
- P0 和 P1 已完成核心改造
- 可以先进入阶段二（查询革命）

**风险**：

- 属性数据量继续增长时性能下降
- 与阶段二的改造可能产生冲突

---

## 📚 参考

- [ADR-004: 架构升级路线图](./ADR-004-Architecture-Upgrade.md)
- Issue #5: 磁盘中心 PersistentStore
- Issue #6: O(1) 增量持久化
- Issue #7: 磁盘中心 PropertyStore

---

**最后更新**: 2025-10-17  
**作者**: NervusDB Team

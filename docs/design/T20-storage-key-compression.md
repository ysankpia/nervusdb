# T20: 存储键压缩设计

## 1. Context

当前三元组键使用 `(u64, u64, u64)` = 24 字节：

```rust
pub struct Triple {
    pub subject_id: u64,   // 8 bytes
    pub predicate_id: u64, // 8 bytes
    pub object_id: u64,    // 8 bytes
}
```

对于嵌入式设备，这导致：
- 缓存命中率低
- 内存带宽浪费
- 比 SQLite (Varint) 慢

## 2. Goals

- 减少键大小到 ~9-12 字节（Varint 编码）
- 提高缓存命中率
- 保持 API 兼容性

## 3. Proposed Solution

### 方案 A: Varint 编码

```rust
// 自定义序列化
fn encode_triple_key(s: u64, p: u64, o: u64) -> Vec<u8> {
    let mut buf = Vec::with_capacity(12);
    leb128::write::unsigned(&mut buf, s).unwrap();
    leb128::write::unsigned(&mut buf, p).unwrap();
    leb128::write::unsigned(&mut buf, o).unwrap();
    buf
}
```

优点：
- 小 ID 只需 1-2 字节
- 平均键大小 ~9 字节

缺点：
- 需要自定义 redb 序列化
- 解码有少量开销

### 方案 B: u32 ID

```rust
pub struct CompactTriple {
    pub subject_id: u32,   // 4 bytes
    pub predicate_id: u32, // 4 bytes
    pub object_id: u32,    // 4 bytes
}
```

优点：
- 简单，无编解码开销
- 键大小固定 12 字节

缺点：
- 限制最大 ID 为 4B
- 需要迁移工具

### 推荐：方案 A (Varint)

更灵活，不限制 ID 范围。

## 4. Implementation Notes

redb 支持自定义 `Key` trait：

```rust
impl redb::Key for VarintTripleKey {
    fn compare(data1: &[u8], data2: &[u8]) -> Ordering {
        // 按 (s, p, o) 字典序比较
    }
}
```

## 5. Status

**Plan** - 需要更多性能测试数据来验证收益

# T201 Implementation Plan: Python Binding

## 1. Overview

为 NervusDB v2 添加 Python 支持，使 Python 开发者可以通过 `pip install nervusdb` 使用本地图数据库。

**技术选型**: pyo3 (Rust ↔ Python 绑定)

## 2. Requirements Analysis

### 2.1 Use Scenarios

1. **Data Science 应用**: 使用 Python 操作图数据，进行图分析
2. **AI/RAG Pipeline**: 存储知识图谱，检索相关节点
3. **后端服务**: 嵌入式数据库替代 SQLite

### 2.2 Functional Requirements

- [ ] `pip install nervusdb` 安装
- [ ] 打开/创建数据库 (`Db.open()`)
- [ ] Cypher 查询 (`db.query()`)
- [ ] 读写事务 (`begin_write()`, `commit()`)
- [ ] 节点/边操作
- [ ] 索引支持

## 3. Design

### 3.1 技术架构

```
┌─────────────────────────────────────────────────────┐
│                   Python Application                 │
├─────────────────────────────────────────────────────┤
│              nervusdb Python Package                │
│    ┌───────────────────────────────────────────┐    │
│    │  Db class (pyo3 exposed)                  │    │
│    │  - open()                                 │    │
│    │  - query()                                │    │
│    │  - begin_write()                          │    │
│    │  - snapshot()                             │    │
│    └───────────────────────────────────────────┘    │
│                       ↓                              │
│              nervusdb (Rust facade)              │
│    ┌───────────────────────────────────────────┐    │
│    │  Db, WriteTxn, ReadTxn, ResultSet         │    │
│    └───────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────┘
```

### 3.2 API 设计

```python
import nervusdb

# 打开数据库
db = nervusdb.Db.open("my_graph.ndb")

# Cypher 查询
result = db.query("MATCH (n) RETURN n LIMIT 10")

# 写入数据
with db.begin_write() as txn:
    txn.execute("CREATE (n:Person {name: 'Alice'})")
    txn.commit()

# 关闭
db.close()
```

### 3.3 暴露类型

| Rust 类型 | Python 类型 | 说明 |
|-----------|-------------|------|
| `Db` | `nervusdb.Db` | 主数据库句柄 |
| `WriteTxn` | `nervusdb.WriteTxn` | 写事务 |
| `PropertyValue` | `nervusdb.PropertyValue` | 属性值枚举 |
| `ResultSet` | `nervusdb.ResultSet` | 查询结果集 |

## 4. Implementation Plan

### Step 1: 创建 pyo3 子 crate (Risk: Low)

- 创建 `nervusdb-pyo3/Cargo.toml`
- 添加 pyo3 依赖
- 配置 build.rs

### Step 2: 绑定核心类型 (Risk: Low)

- 绑定 `Db` 类
- 绑定 `WriteTxn` 类
- 绑定 `PropertyValue` 枚举

### Step 3: 实现 Cypher 查询 (Risk: Medium)

- 绑定 `query()` 方法
- 处理 `ResultSet` 返回
- 转换 Rust → Python 类型

### Step 4: 配置发布 (Risk: Low)

- 配置 `setup.py` / `pyproject.toml`
- 添加 GitHub Actions 构建 wheel

## 5. Technical Key Points

### 5.1 类型转换

```rust
// PropertyValue 转换
impl FromPyObject for PropertyValue {
    fn extract(obj: &PyAny) -> PyResult<Self> {
        // Python dict → PropertyValue::Map
        // Python list → PropertyValue::List
        // ...
    }
}

impl IntoPy<PyObject> for PropertyValue {
    fn into_py(self, py: Python) -> PyObject {
        // PropertyValue → Python 对象
    }
}
```

### 5.2 GIL 释放

- pyo3 默认需要 GIL，API 设计成 "with GIL, release GIL"
- `Db` 和 `Txn` 操作期间保持 GIL

## 6. 文件结构

```
nervusdb-pyo3/
├── Cargo.toml
├── src/
│   └── lib.rs          # 主模块
├── src/
│   └── db.rs           # Db 类
│   └── txn.rs          # WriteTxn 类
│   └── types.rs        # PropertyValue 等类型
├── pyproject.toml      # Python 包配置
└── build.rs            # pyo3 build script
```

## 7. 暂不包含 (T203 后补充)

- 向量搜索 API (`vector.search()`)
- HNSW 索引相关类型

## 8. Risk Assessment

| 风险 | 级别 | 缓解措施 |
|------|------|---------|
| pyo3 版本兼容 | 低 | 使用 stable 版本 |
| 类型转换复杂 | 中 | 逐步实现，先支持基础类型 |
| 构建 wheel 复杂 | 低 | 使用 actions/setup-python |

## 9. Verification Plan

### 9.1 Unit Tests

- [ ] 类型转换测试
- [ ] 异常处理测试

### 9.2 Integration Tests

- [ ] 完整 Cypher 查询流程
- [ ] 读写事务测试
- [ ] 多线程安全测试

### 9.3 验收标准

```python
# 基础功能
db = Db.open("test.ndb")
db.query("CREATE (n {id: 1})")
result = db.query("MATCH (n) RETURN n.id")
assert result[0]["n.id"] == 1
```

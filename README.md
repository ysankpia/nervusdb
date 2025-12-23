# NervusDB

**一个嵌入式图数据库，像 SQLite 一样简单，但专门用来存储和查询"关系"。**

[![CI](https://github.com/LuQing-Studio/nervusdb/actions/workflows/ci.yml/badge.svg)](https://github.com/LuQing-Studio/nervusdb/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

---

## 🤔 什么是图数据库？

想象一下你的微信好友关系：

```
你 --[认识]--> 小明
小明 --[认识]--> 小红
小红 --[认识]--> 你
```

这就是一个"图"！图数据库专门用来存储这种**实体之间的关系**。

传统数据库（如 MySQL）存这种数据需要建很多表、写复杂的 JOIN 查询。而图数据库天生就是为关系设计的，查询起来又快又直观。

## 💡 NervusDB 能做什么？

NervusDB 把数据存成**三元组**：`(主体, 关系, 客体)`

```
(alice, knows, bob)        # alice 认识 bob
(alice, likes, 电影)        # alice 喜欢电影
(bob, works_at, Google)    # bob 在 Google 工作
```

然后你可以用类似 SQL 的 **Cypher 查询语言**来查询：

```cypher
// 找出 alice 认识的所有人
MATCH (alice)-[:knows]->(friend) 
WHERE alice.name = 'alice'
RETURN friend

// 找出两跳内的朋友（朋友的朋友）
MATCH (me)-[:knows]->()-[:knows]->(fof)
RETURN fof
```

## 🎯 适合什么场景？

| 场景 | 例子 |
|------|------|
| **社交网络** | 好友关系、关注/粉丝、共同好友推荐 |
| **知识图谱** | 实体关系、问答系统、智能搜索 |
| **推荐系统** | 用户-商品关系、协同过滤 |
| **欺诈检测** | 交易网络、异常模式识别 |
| **游戏开发** | NPC 关系、任务依赖、技能树 |
| **AI Agent** | 记忆存储、上下文关联、知识管理 |

## ✨ 为什么选择 NervusDB？

| 特点 | 说明 |
|------|------|
| **嵌入式** | 像 SQLite 一样，无需安装服务器，数据就是一个文件 |
| **崩溃安全** | 断电、kill -9 都不会丢数据 |
| **多语言** | Rust / Node.js / Python / C / WebAssembly |
| **Cypher 查询** | 业界标准的图查询语言（支持子集） |
| **高性能** | Rust 编写，449K ops/sec 写入速度 |

## 🚀 快速开始

### Node.js

```bash
npm install nervusdb
```

```javascript
import { NervusDB } from 'nervusdb';

// 打开数据库（文件不存在会自动创建）
const db = await NervusDB.open('my-graph.redb');

// 添加关系
db.addFact('alice', 'knows', 'bob');
db.addFact('bob', 'knows', 'charlie');
db.addFact('alice', 'likes', '电影');

// Cypher 查询：找出 alice 认识的人
const result = db.cypher('MATCH (a {name: "alice"})-[:knows]->(b) RETURN b');
console.log(result.records);
// => [{ b: 'bob' }]

// 关闭数据库
db.close();
```

### Python

```bash
pip install nervusdb
```

```python
from nervusdb import NervusDB

# 打开数据库
db = NervusDB.open('my-graph.redb')

# 添加关系
db.add_fact('alice', 'knows', 'bob')
db.add_fact('bob', 'knows', 'charlie')

# 查询
results = db.cypher('MATCH (a)-[:knows]->(b) RETURN a, b')
for row in results:
    print(f"{row['a']} knows {row['b']}")

db.close()
```

### Rust

```toml
[dependencies]
nervusdb-core = { git = "https://github.com/LuQing-Studio/nervusdb" }
```

```rust
use nervusdb_core::{Database, Fact, Options};

fn main() -> nervusdb_core::Result<()> {
    let mut db = Database::open(Options::new("my-graph.redb"))?;
    
    // 添加关系
    db.add_fact(Fact::new("alice", "knows", "bob"))?;
    
    // 查询
    let results = db.execute_query("MATCH (a)-[r]->(b) RETURN a, r, b")?;
    println!("{:?}", results);
    
    Ok(())
}
```

## 📖 更多示例

### 构建知识图谱

```javascript
// 添加实体和关系
db.addFact('北京', 'is_capital_of', '中国');
db.addFact('中国', 'located_in', '亚洲');
db.addFact('李白', 'born_in', '中国');
db.addFact('李白', 'is_a', '诗人');
db.addFact('李白', 'wrote', '静夜思');

// 查询：李白写了什么？
db.cypher('MATCH (lb {name: "李白"})-[:wrote]->(poem) RETURN poem');

// 查询：哪些诗人出生在亚洲的国家？
db.cypher(`
  MATCH (poet)-[:is_a]->(:诗人),
        (poet)-[:born_in]->(country),
        (country)-[:located_in]->(亚洲)
  RETURN poet
`);
```

### 社交网络分析

```javascript
// 添加好友关系
db.addFact('小明', 'follows', '小红');
db.addFact('小红', 'follows', '小刚');
db.addFact('小刚', 'follows', '小明');

// 找出小明关注的人也关注了谁（二度关系）
db.cypher(`
  MATCH (小明)-[:follows]->(friend)-[:follows]->(fof)
  WHERE 小明.name = '小明'
  RETURN fof
`);

// 使用内置算法计算 PageRank（影响力排名）
const pagerank = db.algorithms.pageRank({ predicate: 'follows' });
console.log(pagerank);
// => [{ nodeId: 123, score: 0.35 }, ...]
```

### AI Agent 记忆存储

```javascript
// 存储对话上下文
db.addFact('conversation_001', 'has_message', 'msg_001');
db.addFact('msg_001', 'content', '你好，我想订一张机票');
db.addFact('msg_001', 'intent', 'book_flight');
db.addFact('msg_001', 'timestamp', '2024-01-01T10:00:00Z');

// 存储用户偏好
db.addFact('user_alice', 'prefers', '经济舱');
db.addFact('user_alice', 'frequent_destination', '上海');

// 查询用户历史偏好
db.cypher(`
  MATCH (user {id: 'user_alice'})-[:prefers]->(pref)
  RETURN pref
`);
```

---

## 🔧 技术细节

### 存储架构

- **三索引三元组存储**：`SPO / POS / OSP` 索引覆盖常见查询模式
- **字典 Interning + LRU 缓存**：字符串只存一次，热数据走内存
- **单文件存储**：基于 [redb](https://github.com/cberner/redb)，ACID 事务保证

### 仓库结构

```
nervusdb/
├── nervusdb-core/       # Rust 核心库
│   ├── src/
│   │   ├── lib.rs       # 主入口
│   │   ├── storage/     # 存储层（Hexastore）
│   │   ├── query/       # Cypher 解析器和执行器
│   │   ├── algorithms/  # 图算法（PageRank、最短路径）
│   │   └── ffi.rs       # C FFI 接口
│   └── include/nervusdb.h
├── bindings/
│   ├── node/            # Node.js 绑定 (NAPI-RS)
│   └── python/          # Python 绑定 (PyO3)
└── nervusdb-wasm/       # WebAssembly 模块
```

### Cypher 支持范围

```cypher
-- ✅ 支持
MATCH (a)-[r:TYPE]->(b)
WHERE a.prop = 'value' AND b.prop > 10
RETURN a, r, b
LIMIT 100

CREATE (a:Person {name: 'Alice'})
CREATE (a)-[:KNOWS]->(b)

SET a.prop = 'value'
DELETE a
DETACH DELETE a

-- ❌ 暂不支持
OPTIONAL MATCH
MERGE
WITH
UNION
聚合函数 (COUNT, SUM, AVG)
```

完整支持列表见 [docs/cypher_support.md](docs/cypher_support.md)

### C API（SQLite 风格）

```c
#include "nervusdb.h"

nervusdb_db *db;
nervusdb_open("demo.redb", &db, NULL);

// 添加三元组
uint64_t alice, knows, bob;
nervusdb_intern(db, "alice", &alice, NULL);
nervusdb_intern(db, "knows", &knows, NULL);
nervusdb_intern(db, "bob", &bob, NULL);
nervusdb_add_triple(db, alice, knows, bob, NULL);

// 查询（类似 sqlite3_prepare/step/finalize）
nervusdb_stmt *stmt;
nervusdb_prepare_v2(db, "MATCH (a)-[r]->(b) RETURN a, r, b", NULL, &stmt, NULL);
while (nervusdb_step(stmt, NULL) == NERVUSDB_ROW) {
    uint64_t a = nervusdb_column_node_id(stmt, 0);
    // ...
}
nervusdb_finalize(stmt);
nervusdb_close(db);
```

## 📦 安装

### 从源码构建

```bash
git clone https://github.com/LuQing-Studio/nervusdb.git
cd nervusdb
cargo build --release
```

### Cargo

```toml
[dependencies]
nervusdb-core = { git = "https://github.com/LuQing-Studio/nervusdb" }
```

## 🧪 开发

```bash
# 格式检查
cargo fmt --all -- --check

# Lint
cargo clippy --workspace --all-targets

# 测试
cargo test --workspace

# 性能测试
cargo run --example bench_compare -p nervusdb-core --release
```

## 🤝 贡献

欢迎 Issue 和 PR！

- pre-commit 钩子会自动运行 `cargo fmt` 和 `cargo clippy`
- 设计文档在 `docs/design/` 目录

## 📄 许可证

[Apache-2.0](LICENSE)

---

**如果觉得有用，请给个 ⭐ Star！**

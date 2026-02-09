# NervusDB v2 - 快速参考

## 🚀 快速命令

### 开发
```bash
# 运行所有测试
cargo test

# TCK 测试
cargo test --test tck_harness

# 代码格式化
cargo fmt

# Lint 检查
cargo clippy --all-features -- -D warnings

# 生成文档
cargo doc --no-deps --open
```

### 使用
```rust
// Rust API
use nervusdb_v2::Db;

let db = Db::open_paths(["/tmp/demo.ndb"])?;
db.execute("CREATE (a {name: 'Alice'})", None)?;
let results = db.query("MATCH (a) RETURN a", None)?;
```

```python
# Python API
import nervusdb

db = nervusdb.connect("/tmp/demo.ndb")
db.execute("CREATE (a {name: 'Alice'})")
results = db.query("MATCH (a) RETURN a")
```

```bash
# CLI
nervusdb-cli v2 write --db /tmp/demo --cypher "CREATE (a {name: 'Alice'})"
nervusdb-cli v2 query --db /tmp/demo --cypher "MATCH (a) RETURN a"
```

## 📊 当前状态 (2026-01-02)

| 组件 | 完成度 | 状态 |
|------|--------|------|
| 存储引擎 | 95% | ✅ 完成 |
| 查询引擎 | 80% | 🔄 M4 进行中 |
| Python 绑定 | 60% | 🔄 进行中 |
| CLI | 90% | ✅ 完成 |
| TCK 覆盖 | 5% | 🎯 M4 目标 70% |

## 🎯 里程碑

- **M3** (当前): Core Foundation ✅
- **M4** (2026-Q1): Cypher Completeness (TCK ≥70%) 🔄
- **M5** (2026-Q2): Polish & Performance (TCK ≥90%)
- **v1.0** (2026-Q4): Production Ready (TCK ≥95%)

## 📚 关键文档

| 文档 | 用途 |
|------|------|
| [PROJECT_SPECIFICATION.md](PROJECT_SPECIFICATION.md) | 项目最高规范 (必读) |
| [docs/tasks.md](docs/tasks.md) | 任务追踪和进度 |
| [ROADMAP.md](ROADMAP.md) | 详细路线图 |
| [docs/reference/cypher_support.md](docs/reference/cypher_support.md) | Cypher 功能支持列表 |
| [README.md](README.md) | 项目介绍和快速上手 |

## 🐛 报告问题

1. 检查是否已存在 [GitHub Issues](https://github.com/LuQing-Studio/nervusdb/issues)
2. 创建新 issue，包含:
   - 复现步骤
   - 预期行为
   - 实际行为
   - 环境信息 (`rustc --version`, `cargo --version`)

## 🤝 贡献代码

1. 阅读 [PROJECT_SPECIFICATION.md](PROJECT_SPECIFICATION.md) 了解开发流程
2. 从 [docs/tasks.md](docs/tasks.md) 选择任务
3. 创建分支 `feat/T{ID}-{description}`
4. 使用 TDD 方法实现
5. 运行测试确保通过
6. 创建 PR

## ⚠️ 注意事项

- 项目处于 Alpha 阶段，API 可能变更
- 大量 Cypher 功能尚未实现 (TCK 覆盖率 5%)
- 崩溃恢复已实现并测试，但不建议生产使用
- Python 绑定尚未稳定

---

**最后更新**: 2026-01-02

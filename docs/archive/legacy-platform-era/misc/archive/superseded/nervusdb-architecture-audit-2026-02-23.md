# NervusDB 架构诊断：离“图数据库界的 SQLite”还有多远？

> 本审核文档基于 NervusDB v2.3 Beta 及目前的 C-API (v1) 大方向进行对比剥析。

## 1. 核心结论与正确转向 (The "Right Track" Pivot)

- **C-ABI 防腐层构建**：近期确立的 `docs/abi/c-api-v1.md` 稳定 ABI 以及 `libnervusdb` 动态库，是本项目最接近 SQLite 成功秘诀的一步。通过让复杂的业务逻辑（图中节点寻址、状态机、多重存储一致性、事务并发控制）完全下沉到 Rust 内核，对外仅暴露极少量的不透明句柄（`opaque handles`，如 `ndb_db_t`, `ndb_txn_t`）和统整化的整型错误码。这非常符合 SQLite `sqlite3.h` 的极简美学。
- **三端薄绑定 (Thin Bindings)**：从 `tasks.md` 中可以看到废除 Python(PyO3) 和 Node(N-API) 层的 "soft-gate"（特保/软放行）转为硬对齐断言。这保证了绑定层没有任何“业务语义解释”，只负责跨语言类型的转换，彻底斩断了逻辑泄露。这是极其正确的收敛动作。

## 2. “走偏”的历史包袱：过度设计 (Where it digressed)

如果用真正的 SQLite 标准（极致精简：B-Tree Pager + VDBE 字节码引擎）来衡量，NervusDB 的内部实现目前显得非常庞大，甚至呈现出过度设计（Over-engineering）的迹象：

### 2.1 存储引擎（Storage Engine）的缝合怪形态

- **架构现状**：`nervusdb-storage` 混合了多种复杂存储结构。边（Edge）的读写路径采用 LSM-Tree 的变种（`MemTable` -> 在内存聚合成 `L0Run` -> 落盘压实为 `CSR` 压缩稀疏行）；但属性（Property）存储又挂靠在另外独立的一套 B-Tree 架构上。
- **代价分析**：图计算确实需要 O(1) 级别的邻接表（CSR）加持以实现高速遍历，这也是图数据库避不开的结构。但为了应对更新去维护“双面数据结构”的心智负担、内存屏障（snapshot isolation）、以及 Checkpoint、Vacuum 时的多重一致性对齐，对嵌入式引擎而言极其沉重。这也是目前代码膨胀且在性能压测时容易触发资源吃紧的根因。相比之下，传统的 SQLite 依靠单一朴素的 B-Tree 便做到了极致的稳定。

### 2.2 执行引擎（Query Engine）的重火力

- **架构现状**：为了让高度复杂的图查询语言 openCypher 能够做到官方测试套件（TCK）近 100% 通过（跑通 3800+ 场景），导致解析器（Parser）和基于枚举（Enum）的火山模型（Volcano Iterator Tree）变得极为庞大。从文件树看，`executor` 模块甚至被横向切分成了多达 30 多份子文件。
- **代价分析**：SQLite 的核心体积之所以小，在于它先把复杂的 SQL 编译为紧凑的虚拟机字节码（VDBE），然后由极简的轻量状态机去跑。而直接解读 AST 并套着厚重的组合迭代器网络去跑 Cypher 语言特性（各类列表推导、任意深度路径、复杂的 OPTIONAL MATCH、各种表达式的强校验等），会让执行时的资源开销像大型商用服务器数据库一样沉重。

## 3. 下阶段架构收敛建议 (Next Steps and Convergence Road)

既然目前“混血双引擎存储”和“重型迭代器执行器”的厚重复杂度已经是为“强功能指标（TCK 95%+）”和 O(1)遍历付出的既定代价，那目前就不宜再强行推翻重来。相反，建议用**严苛的工程约束**来圈养这头“性能怪兽”：

1. **死守 ABI 与安全边界**：无论内部怎么重构，对外 C API 层以及单开销文件的 `libnervusdb` 编译产物必须像岩石一样稳定。对使用者的形态必须必须做到“全语言零配置（Zero-Conf）”。
2. **硬性的 OOM 资源护栏（Resource Fencing）**：从近期的 `W13-PERF` 进度看到，项目已引入 `ExecuteOptions` 控制中间结果集行数和 `soft_timeout_ms`。这就完全走对了！SQLite 哪怕跑再反人类的 JOIN 也极少崩溃，NervusDB 的下一步必须是“绝对防御”，即无论查询多么离谱都只报错，坚决不能让宿主进程内存爆炸。
3. **从“堆 Feature”彻底转向“Boring Stability”**：目前的“连续 7 天稳定窗”测试是一门非常硬核且古典的软件工程防线。后续的重点应当放在剥离没必要的泛型、削减编译二进制体积（可利用 Cargo strip、LTO 等），以及极致的冷启动优化上，必须要把该有的那套“最纯正的嵌入式灵魂”给抢回来。

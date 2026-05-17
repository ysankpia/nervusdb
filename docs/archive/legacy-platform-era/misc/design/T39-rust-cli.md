# T39: Rust CLI（查询/流式输出）

## 0. Linus 三问

1) **真问题吗？**  
真问题。没有 CLI，用户验证/基准/脚本化就只能靠 Node/Python 或自己写 Rust 程序。对“嵌入式数据库”来说这很蠢。

2) **更简单的方式？**  
最简单的 CLI 就做两件事：打开 DB、执行 Cypher、流式输出。别一上来做 REPL/导入导出/交互 UI，那是把复杂度往自己头上扣。

3) **会不会破坏 userspace？**  
不会。新增一个 binary crate，不改 `nervusdb-core` 的公开 API/ABI，不影响现有 Node/Python。

## 1. Goals

- 提供一个 Rust CLI：`nervusdb query ...`（或者等价命令），支持：
  - `--db <path>` 指定库文件基路径（与 core 的 `Options::new(path)` 一致）
  - `--cypher <string>` 或 `--file <path>` 输入查询
  - `--params-json <json>`（JSON object；为空/缺省表示无参数）
  - `--format ndjson`（默认，逐行输出；真流式）
- 输出必须是流式：不能把结果全塞进 Vec 再打印。

## 2. Non-Goals

- 不做 REPL（后续可加 `shell` 子命令，用 `reedline/rustyline` 之类做交互）。
- 不做 bulk import/export（那是另一套任务）。
- 不做跨平台预编译发布流程（先把功能做对）。

## 3. Solution

### 3.1 代码布局

新增 workspace member：`nervusdb-cli/`（单独 crate）

- `Cargo.toml`：依赖 `nervusdb-core` + `clap` + `serde_json`
- `src/main.rs`：解析参数、执行子命令

### 3.2 执行路径（核心：真流式）

- 解析 Cypher：`nervusdb_core::query::parser::Parser::parse`
- 规划：`nervusdb_core::query::planner::QueryPlanner::plan`
- 执行：`PhysicalPlan::execute_streaming(ArcExecutionContext)`
- 输出：每次 `iter.next()` 得到一条 `Record` 就立刻序列化并写到 stdout（NDJSON，一行一个 JSON object）

### 3.3 值的序列化

把 `Record { values: HashMap<String, Value> }` 转成 JSON：

- `String` → JSON string
- `Float` → JSON number
- `Boolean` → JSON bool
- `Null` → JSON null
- `Vector(Vec<f32>)` → JSON array[number]
- `Node(u64)` → `{ "node_id": <u64> }`
- `Relationship(Triple)` → `{ "subject_id": <u64>, "predicate_id": <u64>, "object_id": <u64> }`

注：这比现在 `exec_cypher`/node `executeQuery` 里那种 `{ "id": id }` 对关系的“瞎糊弄”要靠谱得多，但 CLI 不会改现有 JSON API 行为（不破坏 userspace）。

## 4. Testing Strategy

- `cargo build -p nervusdb-cli`
- `cargo run -p nervusdb-cli -- query --db /tmp/test --cypher \"MATCH ...\"`
- 若仓库已有适合的最小 smoke 查询，可加一个 `tests/cli_smoke.rs`（仅当现有项目已经在做 Rust integration tests；不额外引入测试框架）

## 5. Risks

- 新增 workspace member 会触发 CI 变更，属于高风险配置改动：必须确保 `cargo test --workspace` 仍然绿。
- Windows 路径/编码问题先别管（先保证 macOS/Linux 可用；后续再补）。


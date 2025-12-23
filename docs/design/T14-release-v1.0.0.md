# T14: v1.0.0 封版（契约/白名单/Crash Gate）

## 1. Context

v0.x 的“随时会变”对嵌入式数据库是致命的：没人会把一个不敢承诺 ABI 的库嵌进产品里。

v1.0.0 的意义不是“全功能”，而是 **契约已定、语义可预期、崩溃一致性门禁可复现**。

## 2. Goals（Definition of Done）

1. **ABI 法律化**
   - `nervusdb-core/include/nervusdb.h` 签名冻结（1.0 发布后至少 90 天内不改签名）。
   - `NERVUSDB_ABI_VERSION` 仅在破坏性 ABI 变更时递增。
2. **Cypher 白名单**
   - 支持：`MATCH` / `CREATE` / `RETURN` / `WHERE`（基础比较与逻辑）/ `LIMIT`（仅整数）。
   - 明确不支持：`OPTIONAL MATCH` / `UNION` / `WITH` / `ORDER BY` / `SKIP` / `RETURN DISTINCT`。
   - 不支持的语法必须 **fail-fast**：返回 `Error::NotImplemented`（禁止“跑错”）。
3. **诚实文档**
   - README 首页第一句话：`NervusDB: An Embedded, Crash-Safe Graph Database (Subset of Cypher, Powered by Rust)`.
   - `docs/cypher_support.md` 写清白名单与限制（以测试为准）。
4. **Crash Gate**
   - 发布前本地复跑 `nervus-crash-test` 1000 次并通过（参数与 CI gate 一致）。

## 3. Non-Goals

- 不做 CBO/复杂优化器（1.0 只保证语义边界清晰）。
- 不做 OPTIONAL MATCH/ORDER BY 等语义补齐（明确 NotImplemented 即可）。
- 不改 `nervusdb.h` 的现有函数签名与内存/生命周期约定。

## 4. Solution

1. **Parser → Error::NotImplemented**
   - 解析阶段识别不支持的关键字并返回 `Error::NotImplemented("<feature>")`。
2. **LIMIT**
   - Parser 支持 `RETURN ... LIMIT <int>`。
   - Planner/Executor 增加 `Limit` 物理算子（只实现 `LIMIT`，`SKIP` 明确 NotImplemented）。
3. **文档与版本**
   - 统一版本号到 `1.0.0`（Rust core / Node / Python）。
   - `CHANGELOG.md` 增加 `1.0.0` 条目（强调“契约稳定 + 子集 Cypher + crash gate”）。

## 5. Testing Strategy

- `cargo test --workspace`
- `pnpm -C bindings/node test && pnpm -C bindings/node test:native`
- `cargo run -p nervusdb-core --bin nervus-crash-test -- driver ... --iterations 1000`

## 6. Risks

- 白名单策略若没 fail-fast，会导致“看似能跑、实际上跑错”的灾难性信任崩塌。
- LIMIT 的实现必须确保不会吞掉执行期错误（只允许在 stop 后不再触发后续迭代）。


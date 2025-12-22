# T5: Fuck-off Test（`kill -9` 崩溃一致性验证）

## 1. Context

- 数据库的底线不是“快”，是“你怎么折腾都不坏”。
- `redb` 的事务是原子性的，但我们不能靠“理论正确”自我感动：必须用 `kill -9` 验证恢复后没有半条数据/字典错配。

## 2. Goals

- 提供一个可重复运行的压力程序：
  - 写入（含事务）过程中随机被外部 `kill -9`。
  - 反复重启打开 DB 后校验一致性：
    - 任意 triple 的 S/P/O ID 必须能在字典里解析到字符串。
    - `id_to_str` 与 `str_to_id` 必须互相一致。
- 默认不把 CI 搞成抽奖：可以先做成 `#[ignore]` 或显式命令运行的测试门。

## 3. Non-Goals

- 不做跨平台信号语义兼容（优先支持 Linux/macOS）。
- 不做“自带注入点”的复杂 crash framework（过度工程化）。

## 4. Solution

1. 新增一个工具二进制：`nervusdb-core/src/bin/nervus-crash-test.rs`
   - `writer`：循环 `begin tx → 批量写入 → commit`（让外部 `kill -9` 命中事务中间）
   - `verify`：做一致性检查（字典双向 + 三元组引用）
   - `driver`：spawn `writer` → 随机 sleep → `kill -9` → `verify`，重复 N 次
2. 运行方式（示例）
   - `cargo run -p nervusdb-core --bin nervus-crash-test -- driver ./tmp-fuckoff --iterations 200`
   - `cargo run -p nervusdb-core --bin nervus-crash-test -- verify ./tmp-fuckoff`

## 5. Testing Strategy

- 先提供可手动运行的命令（可控、可复现）。
- 需要时再把它纳入 CI 的“专门 job”（别污染常规单测）。

## 6. Risks

- 进程/信号导致的 flakiness：必须把迭代次数与时间控制在可接受范围，并且让失败输出足够可诊断。

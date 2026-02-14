# S1：CLI 边界收敛（移除直连 Storage）

更新时间：2026-02-12  
任务类型：Phase 1a  
任务状态：Done

## 1. 目标

- 去除 CLI 对 `nervusdb_storage::engine::GraphEngine` 的直接依赖。
- 统一通过 facade/Db 层完成查询执行与快照访问。
- 保持 CLI 参数、输出文本协议、错误映射不变。

## 2. 边界

- 允许：CLI 内部适配层重构、依赖收敛。
- 禁止：命令参数变更、输出字段变更、退出码策略变更。
- 禁止：借机修改 query/storage 语义。

## 3. 文件清单

### 3.1 必改文件

- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/main.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/repl.rs`

### 3.2 可选新增

- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/facade_adapter.rs`

## 4. 当前问题证据

- 历史问题（已收敛）：CLI `vacuum` 直连 storage 实现层。
  - 当前修复点：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/main.rs:248`
  - 门面入口：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/src/lib.rs:212`
- 依赖收敛证据：CLI 依赖中不再声明 `nervusdb-storage`。
  - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/Cargo.toml:15`

## 5. TDD 步骤

1. 增加 CLI 行为等价测试（参数解析/错误输出/REPL 常用命令）。
2. 删除 CLI 对 `GraphEngine` 直接 import，改走 facade 入口。
3. 跑全门禁 + CLI 定向回归。

## 6. 测试清单

- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/t202_import_cli.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/smoke.rs`
- `bash scripts/workspace_quick_test.sh`

## 7. 回滚步骤

1. 出现参数兼容性变化或输出协议变化，直接回滚该 PR。
2. 回滚后补充 CLI 回归用例再重提。

## 8. 完成定义（DoD）

- CLI 不再直连 `GraphEngine`。
- 用户可见行为不变。
- 全门禁通过。

## 9. 当前进展（2026-02-11）

- 已移除 CLI 对 `nervusdb_storage::engine::GraphEngine` 的直接依赖：
  - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/main.rs:2`
  - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/repl.rs:1`
- Query/Write/Repl 均改为通过 `Db::snapshot()` 获取快照：
  - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/main.rs:210`
  - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/main.rs:240`
  - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/repl.rs:131`
- 已通过 CLI 与回归验证：
  - `cargo test -p nervusdb-cli`
  - `cargo test -p nervusdb --test t202_import_cli -- --nocapture`
  - `bash scripts/workspace_quick_test.sh`（退出码 0）

## 10. 补充进展（2026-02-12）

- 新增 `nervusdb` 门面真空入口，CLI 不再调用 storage 真空实现：
  - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/src/lib.rs:62`
  - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/src/lib.rs:212`
  - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/main.rs:2`
  - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/main.rs:248`
- 新增门面单测（先红后绿）：
  - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/src/lib.rs:496`
  - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/src/lib.rs:507`
- 本轮验证通过：
  - `cargo test -p nervusdb --lib`
  - `cargo check -p nervusdb-cli`

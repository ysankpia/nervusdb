# T9: Node Tests 纳入 CI（覆盖 Binding ↔ Native 交互）

## 1. Context

- 当前 `.github/workflows/ci.yml` 只跑 Rust（fmt/clippy/test/build）。
- 绑定层（TypeScript + N-API loader + Node side wrappers）没有进入 CI，属于“盲飞”。

## 2. Goals

- CI 必须跑 Node：至少 typecheck + unit tests。
- 绑定层必须覆盖 **native 交互路径**（不是只跑 `NERVUSDB_DISABLE_NATIVE=1`）。
- 维持 CI 时长可接受：不跑重 benchmark、不跑 200 次 crash test。

## 3. Non-Goals

- 不把 `nervus-crash-test` 放进每次 PR（可做成 `workflow_dispatch` 或手动门）。
- 不在 CI 里跑性能基准（那是测量，不是验证）。

## 4. Solution

- 在 `.github/workflows/ci.yml` 增加 `node-ci` job（建议先只跑 `ubuntu-latest`）：
  1. Setup Node（LTS）+ 启用 corepack/pnpm
  2. `pnpm -C bindings/node install`
  3. 构建 native addon（`pnpm -C bindings/node build:native` 或等价命令）
  4. 运行两类测试：
     - **TS-only**：`NERVUSDB_DISABLE_NATIVE=1 pnpm -C bindings/node test`
     - **Native path**：设置 `NERVUSDB_EXPECT_NATIVE=1`，跑一组专门的 `*.native.test.ts`（需要新增/调整 script）

## 5. Testing Strategy

- 至少新增一个 native 交互回归用例：
  - `cypherQuery()` 走 native `executeQuery`（T7 的 P0 bug 用例），确保不会退回到事实查询 API。

## 6. Risks

- CI 引入 Node+pnpm 会增加时间与缓存复杂度：先从单 OS/job 做起，稳定后再扩矩阵。


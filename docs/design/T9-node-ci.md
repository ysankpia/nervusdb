# T9: Node Tests 纳入 CI（覆盖 Binding ↔ Native 交互）

## 1. Context

- 当前 `.github/workflows/ci.yml` 只跑 Rust（fmt/clippy/test/build）。
- 绑定层（TypeScript + N-API loader + Node side wrappers）没有进入 CI，属于“盲飞”。

## 2. Goals

- CI 必须跑 Node：至少 typecheck + unit tests。
- 绑定层必须覆盖 **native 交互路径**（不是只跑 `NERVUSDB_DISABLE_NATIVE=1`）。
- 平台覆盖：**Ubuntu + macOS** 都要跑（绑定层跨平台差异只能靠 CI 抓出来）。
- 维持 CI 时长可接受：不跑重 benchmark、不跑 200 次 crash test。

## 3. Non-Goals

- 不在每次 PR 跑 **200 次** `kill -9`（那会把 CI 变成赌博）。
- 但每次 PR 至少跑一个 **crash-smoke**（例如 3~5 次）来验证“Node → native → crash harness”路径没有断；完整 200 次作为 `workflow_dispatch` / nightly。
- 不在 CI 里跑性能基准（那是测量，不是验证）。

## 4. Solution

- 在 `.github/workflows/ci.yml` 增加 `node-ci` job（OS 矩阵：`ubuntu-latest` + `macos-latest`）：
  1. Setup Node（LTS）+ 启用 corepack/pnpm
  2. `pnpm -C bindings/node install`
  3. 构建 native addon（`pnpm -C bindings/node build:native` 或等价命令）
  4. 运行两类测试：
     - **TS-only**：`NERVUSDB_DISABLE_NATIVE=1 pnpm -C bindings/node test`
     - **Native path**：设置 `NERVUSDB_EXPECT_NATIVE=1`，跑一组专门的 `*.native.test.ts`（需要新增/调整 script）
  5. crash-smoke（小规模）：
     - 通过 Node 触发 `nervus-crash-test`（或直接 spawn 对应二进制），只跑少量轮次验证一致性与接口未断。
     - 完整 200 次版本放到单独 workflow（手动触发/定时）。

## 5. Testing Strategy

- 至少新增一个 native 交互回归用例：
  - `cypherQuery()` 走 native `executeQuery`（T7 的 P0 bug 用例），确保不会退回到事实查询 API。

## 6. Risks

- CI 引入 Node+pnpm 会增加时间与缓存复杂度：先把 **Ubuntu+macOS 的最小闭环** 跑通，再谈 cache/并行优化。
- crash test 属于天然易抖动：把 PR 的 crash-smoke 轮次控制在个位数，避免把 CI 变成 flaky 垃圾。

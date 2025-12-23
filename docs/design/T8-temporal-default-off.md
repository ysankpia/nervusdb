# T8: Temporal 变为 Optional Feature（Default OFF）

## 1. Context

- `TemporalStore`/`temporal_v2` 目前仍在 `nervusdb-core` 默认构建路径里（非 wasm32）。
- Temporal 具备强业务色彩（entity 版本、episode、timeline），不应强迫所有只想存三元组的用户买单。

## 2. Goals

- Temporal 以 Cargo feature 隔离：**默认关闭**。
- feature 关闭时：
  - `nervusdb-core` 不编译 temporal 模块，不暴露 temporal API/类型。
  - Node N-API 不导出 temporal 相关方法（TS 侧以 capability guard 识别）。
- feature 开启时：
  - 行为与当前一致（保持可选能力，不改变存储文件契约）。

## 3. Non-Goals

- 不在本任务里拆分成独立 crate（先 feature gate，后续再拆）。
- 不支持 wasm32 的 temporal（继续保持 native-only）。

## 4. Solution

### 4.1 Core（nervusdb-core）

- `nervusdb-core/Cargo.toml`：
  - 新增 feature：`temporal`
  - 将 temporal 专用依赖（如 `rmp-serde`）改为 `optional = true`，挂到 `temporal` feature 下。
- `nervusdb-core/src/lib.rs`：
  - `mod temporal_v2` 与 `pub use temporal_v2::*` 改为 `#[cfg(all(feature = \"temporal\", not(target_arch = \"wasm32\")))]`
  - `Database` 内的 `temporal` 字段与 `timeline_*`/`temporal_store*` 方法同样 feature gate。

### 4.2 Node Native（nervusdb-node / N-API）

- 为 `bindings/node/native/nervusdb-node` 添加同名 feature `temporal`：
  - 该 feature 透传启用 `nervusdb-core/temporal`
  - 相关类型 import 与 `#[napi] temporal*` 方法都加 `#[cfg(feature = \"temporal\")]`

### 4.3 TypeScript 绑定侧（能力探测与错误边界）

- TypeScript 不“模拟实现”，只做能力探测与 fail-fast：
  - 用现有 `nativeTemporalSupported(nativeHandle)` 判定是否支持。
  - 不支持时：调用任何 `db.memory.*`（除 `getStore()`）必须抛出清晰错误：`Temporal feature is disabled. Rebuild native addon with --features temporal.`

## 5. Testing Strategy

- Rust：在 CI 中增加一个最小编译检查（至少 `cargo test` + `cargo test --features temporal` 两条路径都能过）。
- Node：一条用 temporal-off 的 native build 跑基础测试；temporal-on 可作为后续扩展矩阵（不强行上来就把 CI 搞炸）。

## 6. Risks

- feature gate 牵涉到 public API 和 N-API 导出：容易出现“某处引用了 temporal 类型但 feature off”导致编译失败，需要系统性梳理。

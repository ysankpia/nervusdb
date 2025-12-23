# T12: 1.0 封版准备（ABI 冻结 + 文档清洗 + Crash Gate 复跑）

## 1. Context

当前代码已经接近“能发布”的状态，但用户可见层面还存在硬伤：

- `README.md` 与 `docs/project-structure.md` 描述与当前实现不一致（六序索引/Fact properties/Temporal 默认开启/目录结构等）。
- `CHANGELOG.md` 仍保留旧时代的内容（旧存储术语/插件系统/不存在的特性），属于误导。
- `bindings/node/package.json` 的描述仍在吹旧世界的能力清单，和当前仓库方向冲突。
- 1.0 的底线是“契约稳定”：`nervusdb.h` 必须视为法律，发布后至少三个月不改签名。
- 封版前必须再跑一次 1000 次 crash-test，避免回归。

## 2. Goals

1. **ABI 冻结策略落地（不改 ABI，只写清规则）**
   - 明确 `NERVUSDB_ABI_VERSION` 的含义与升级规则
   - 明确 1.0 发布后 3 个月内禁止改 `nervusdb.h` 签名
2. **文档“去谎言化”**
   - `README.md` 首页改为 C/Rust 接入与单文件语义
   - 纠正索引结构（`SPO/POS/OSP`）与 feature gate（Temporal 默认关闭、Cypher 实验性）
   - 修正/精简 `docs/project-structure.md`，不再指向不存在的目录
3. **变更日志清洗**
   - 主 `CHANGELOG.md` 保持短、可验证、与现状一致
   - 旧内容迁移到 `_archive/`（Git 历史不丢，但主线不污染）
4. **封版验证**
   - 本地复跑 1000 次 `nervus-crash-test`（与 `crash-gate.yml` 同参数）

## 3. Non-Goals

- 不修改 `nervusdb-core/include/nervusdb.h` 的 API 签名/语义。
- 不做 Node 侧“零拷贝对象”改造（1.0 之后再说）。
- 不新增与发布无关的功能。

## 4. Solution

### 4.1 ABI 冻结（文档化）

- 在 README 与/或 `docs/reference/` 中写清：
  - `nervusdb_abi_version()` 必须等于 `NERVUSDB_ABI_VERSION`
  - 仅当发生破坏性 ABI 变更才 bump `NERVUSDB_ABI_VERSION`
  - 1.0 发布后 90 天内禁止改 header（除非安全事故/紧急修复并走 major bump）

### 4.2 README 清洗（面向 C/Rust）

- 快速开始：C（stmt API）+ Rust（core API）
- 单文件语义：`Options::new(path)` -> 生成/使用 `path.with_extension(\"redb\")`
- 真实特性列表（避免“六序索引/全功能 Cypher”等夸张）

### 4.3 CHANGELOG 清洗

- 将旧 changelog 迁移到 `_archive/CHANGELOG_legacy.md`
- 新 `CHANGELOG.md` 只保留“未发布/1.0 关键点”与少量历史（可选）

### 4.4 Crash Gate

- 运行命令（同 workflow）：
  - `cargo run -p nervusdb-core --bin nervus-crash-test -- driver tmp-nightly-crash-gate --iterations 1000 --min-ms 2 --max-ms 15 --batch 200 --subject-pool 20 --predicate-pool 16 --object-pool 20 --verify-retries 50 --verify-backoff-ms 20`

## 5. Testing Strategy

- 文档/元信息：跑格式化与现有 CI 即可（CI 已包含 Rust/Node + crash smoke）
- 封版门禁：本地 1000 次 crash-test 必须 0 失败

## 6. Risks

- 文档更新容易“越写越大”：必须保持短、只写用户需要的硬信息。
- CHANGELOG 重写可能引发争议：旧内容进 `_archive`，主线只保留真实事实。

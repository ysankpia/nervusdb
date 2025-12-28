# 发布指南（GitHub / crates.io / npm / PyPI）

这份文档讲清楚一件事：**怎么把 NervusDB 的 Rust / Node / Python 包“可用地”发布出去**，以及每一步需要的账号、Token、命令、坑点。

> 规则：**Token 不要发到聊天里，不要写进仓库，不要进 shell 历史**。用环境变量或密码管理器。

---

## 0. 你现在卡住的点：`maturin publish` 要用户名

这是正常的：你还没给 PyPI 凭证。

推荐做法（API Token，不要输账号密码）：

1. `Ctrl+C` 退出当前交互式输入（不要继续手打用户名/密码）
2. 去 PyPI 创建 Token（见下面第 3 节）
3. 在当前终端里设置：

```bash
export MATURIN_PYPI_TOKEN="pypi-***"
export MATURIN_NON_INTERACTIVE=1
```

4. 重新执行（在 `bindings/python/nervusdb-py` 目录）：

```bash
maturin publish --skip-existing
```

`--skip-existing` 用来避免你重跑时因为“文件已存在”直接失败。

---

## 1. 发布前提：仓库状态与版本

### 1.1 必须保证工作区干净

```bash
git status -sb
```

要求：没有 `M`/`??`。

### 1.2 版本号要统一（同一个发布版本）

当前仓库使用“多包同版本”的策略（建议一直保持）：

- Rust Core：`nervusdb-core/Cargo.toml`
- Node 包：`bindings/node/package.json`
- Node 原生 addon：`bindings/node/native/nervusdb-node/Cargo.toml`
- Python 包：`bindings/python/nervusdb-py/pyproject.toml`
- Python Rust crate：`bindings/python/nervusdb-py/Cargo.toml`

改完后写入 `CHANGELOG.md`，再 commit、打 tag。

---

## 2. 需要哪些账号？要不要注册？

答案：**要。** 你发布到哪里，就必须在那个注册表上有账号/权限。

- GitHub Release：GitHub 账号（你已经有）
- crates.io：用 GitHub 登录（需要 crates.io API Token）
- npm：npm 账号（且你要对 `@nervusdb/core` 这个 scope 有发布权限）
- PyPI：PyPI 账号（强烈建议开启 2FA，并用 API Token 发布）

---

## 3. PyPI（Python 包）发布：账号 + Token + 命令

### 3.1 注册/登录 PyPI

1. 打开：`https://pypi.org/account/register/`
2. 注册并验证邮箱
3.（强烈建议）开启 2FA：`Account settings -> Two-factor authentication`

### 3.2 创建 API Token（推荐）

1. 打开：`https://pypi.org/manage/account/token/`
2. 新建 token（建议 scope 选“只允许某个项目”，项目名 `nervusdb`）
3. 保存 token（只显示一次）

### 3.3 本地发布命令

安装工具：

```bash
python -m pip install -U maturin
asdf reshim python 3.12.7  # 如果你用 asdf 且遇到 command not found
```

发布（在 `bindings/python/nervusdb-py`）：

```bash
export MATURIN_PYPI_TOKEN="pypi-***"
export MATURIN_NON_INTERACTIVE=1

cd bindings/python/nervusdb-py
maturin publish --skip-existing
```

### 3.4 重要现实：你目前只构建了 macOS arm64 wheel

你刚才的输出显示：

- ✅ 构建了 `macosx_11_0_arm64` wheel
- ✅ 构建了 sdist（源码包）

这意味着：

- macOS arm64 用户：直接安装快
- Linux/Windows 用户：大概率会走“从源码编译”（需要 Rust toolchain），体验差

要做“像样的” PyPI 发布，你需要后续补齐多平台 wheels（建议上 GitHub Actions，用 `maturin` 官方 action）。

---

## 4. crates.io（Rust 包）发布：账号 + Token + 命令

### 4.1 登录与 Token

1. 打开：`https://crates.io/`（用 GitHub 登录）
2. 生成 Token：`Account Settings -> API Access`

### 4.2 发布命令（建议先 dry-run）

```bash
cargo publish -p nervusdb-core --dry-run
```

确认无误后：

```bash
cargo login
cargo publish -p nervusdb-core
```

注意：crates.io **不允许覆盖同版本**；发错只能 bump 版本再发。

---

## 5. npm（Node 包）发布：账号 + scope 权限 + 命令

你现在的包名是：`@nervusdb/core`，这要求：

- npm 上存在 `nervusdb` 这个 scope（用户或 org）
- 你的账号对这个 scope/包有 publish 权限

### 5.1 登录检查

```bash
cd bindings/node
npm whoami
```

### 5.2 先 dry-run 看打包内容

```bash
cd bindings/node
npm publish --dry-run
```

### 5.3 真发布

```bash
cd bindings/node
npm publish --access public
```

同样：npm 也不允许覆盖同版本。

---

## 6. GitHub Release：tag 与发布页

推荐流程：

1. 版本号统一 + 更新 `CHANGELOG.md`
2. commit 到 `main`
3. 打 tag 并 push
4. 创建 GitHub Release

命令示例：

```bash
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin main vX.Y.Z
gh release create vX.Y.Z --title "vX.Y.Z" --notes-file CHANGELOG.md
```

---

## 7. 常见失败与处理

### 7.1 `maturin: command not found`

你用 asdf 的话，装完 pip 包要：

```bash
asdf reshim python 3.12.7
```

### 7.2 PyPI 403 / 包名被占用

- `nervusdb` 如果被别人占了：你只能换名（例如 `nervusdb-core` / `nervusdbdb`）或联系占用者协商转让。
- 如果是你自己的项目但没权限：检查登录账号 / Token scope。

### 7.3 “文件已存在”

用：

```bash
maturin publish --skip-existing
```

但注意：这不是让你“覆盖错误版本”，只是跳过已存在文件。

---

## 8. 建议（别装死）

如果你要让用户“`pip install nervusdb` 就能用”，你必须：

1. PyPI 多平台 wheels（Linux x86_64 至少要有）
2. npm 预编译 `.node`（你现在已经在做了）
3. crates.io 正式发版（Rust 用户才会信你）

这三个发布是**产品可用性**的一部分，不是“锦上添花”。


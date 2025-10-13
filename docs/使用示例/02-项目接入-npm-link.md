# 示例 02 · 项目接入（npm link）

## 目标

- 在多仓协同开发时，通过 npm link 将 NervusDB 作为本地依赖
- 支持边修改边在业务项目中调试

## 步骤 1：在 NervusDB 仓库创建 link

```bash
cd /Volumes/WorkDrive/Develop/github/NervusDB
pnpm build
npm link    # 或 pnpm link --global
```

## 步骤 2：在业务项目中引用

```bash
cd /path/to/your-app
npm link nervusdb    # 或 pnpm link nervusdb
```

## 步骤 3：刷新类型

- 业务项目运行 `pnpm install`
- IDE 中执行 TypeScript 重载，确保读取到本地包

## 步骤 4：调试流程

- 在 NervusDB 中修改源码 → `pnpm build`
- 业务项目重新运行测试或服务
- 可使用 `pnpm dev`（NervusDB）+ `npm run dev`（业务项目）双向 watch

## 步骤 5：解除 link

```bash
cd /path/to/your-app
npm unlink nervusdb --no-save
npm install nervusdb
```

或在全局：`npm unlink nervusdb`

## 常见问题

| 现象                 | 原因                         | 解决                                      |
| -------------------- | ---------------------------- | ----------------------------------------- |
| 业务项目找不到包     | 未执行 `npm link` 或路径错误 | 重新 link，确认包名 `nervusdb`            |
| 类型提示与源码不一致 | 未重新 build                 | 每次源码变更后执行 `pnpm build`           |
| Windows 上权限问题   | 全局 npm 目录需要管理员权限  | 使用 `nvm`/`fnm` 安装 Node 或以管理员运行 |

## 延伸阅读

- [示例 01 · 本地 tgz 安装](01-项目接入-本地tgz安装.md)
- [教程 01 · 安装与环境](../教学文档/教程-01-安装与环境.md)

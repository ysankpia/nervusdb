# 示例 01 · 项目接入（本地 tgz 安装）

## 目标

- 将 NervusDB 以 tgz 包形式安装到现有项目
- 验证构建产物与 TypeScript 类型无缝工作

## 前提

- 已执行 `pnpm build`
- 目标项目支持 ESM 或具备 tsconfig 配置

## 步骤 1：打包

```bash
pnpm pack
```

生成文件形如：`nervusdb-1.1.0.tgz`

## 步骤 2：在目标项目安装

```bash
cd /path/to/your-app
pnpm add ../NervusDB/nervusdb-1.1.0.tgz
```

或使用 npm：`npm install ../NervusDB/nervusdb-1.1.0.tgz`

## 步骤 3：TypeScript 配置

`tsconfig.json`

```json
{
  "compilerOptions": {
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "esModuleInterop": true,
    "resolveJsonModule": true
  }
}
```

## 步骤 4：编写测试代码

```ts
import { NervusDB } from 'nervusdb';

const db = await NervusDB.open('app.nervusdb', { enableLock: true });
await db.addFact({ subject: 'user:alice', predicate: 'FRIEND_OF', object: 'user:bob' });
console.log(await db.find({ predicate: 'FRIEND_OF' }).all());
await db.close();
```

## 步骤 5：打包或部署

- Webpack / Vite / tsup：确保 `external` 排除 `nervusdb` 或配置为 ESM
- 若使用 Docker，记得将 `node_modules` 与数据目录一并复制

## 常见问题

| 现象                   | 原因                    | 解决                                                                                |
| ---------------------- | ----------------------- | ----------------------------------------------------------------------------------- |
| `ERR_MODULE_NOT_FOUND` | ESM 配置缺失            | 设置 `"type": "module"` 或使用 `await import`                                       |
| 类型提示缺失           | IDE 未加载项目 tsconfig | 在 VSCode 中执行 `TypeScript: Select TypeScript Version` -> `Use Workspace Version` |
| 包含源码路径           | 未执行 `pnpm build`     | 打包前务必构建，tgz 会包含 dist/                                                    |

## 延伸阅读

- [示例 02 · 项目接入（npm link）](02-项目接入-npm-link.md)
- [教程 08 · 部署与最佳实践](../教学文档/教程-08-部署与最佳实践.md)

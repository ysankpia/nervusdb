# NervusDB 构建策略

## 🔐 代码保护

NervusDB 使用 **esbuild** 进行打包和混淆，保护源代码不被轻易反编译。

### 构建特性

1. **单文件打包** - 所有代码打包成一个文件
2. **代码混淆** - 变量名被缩短（如 `ir`, `et`, `or`）
3. **压缩** - 所有代码压缩成极少行数
4. **Tree Shaking** - 自动移除未使用的代码
5. **类型定义分离** - 保留 `.d.ts` 供 TypeScript 用户使用

### 构建产物

```
dist/
├── index.mjs       # 主库（151KB，8行，混淆）
├── index.d.ts      # 类型定义
├── cli/
│   └── nervusdb.js # CLI工具（2.3KB，7行，混淆）
└── **/*.d.ts       # 其他类型定义文件
```

## 🧱 原生 N-API 产物

- GitHub Actions 中的 `native-matrix` 任务会在 Linux、macOS（ARM64 / x64）以及 Windows 上编译原生扩展。
- 每个平台的二进制会被移动到 `native/nervusdb-node/npm/<platform>/index.node`，并通过 `upload-artifact` 暂存夜间构建。
- 测试矩阵在构建后运行 `pnpm vitest run tests/unit/native/native_loader.test.ts tests/unit/storage/persistentStore.native.test.ts`，并设置 `NERVUSDB_EXPECT_NATIVE=1` 以确保 `loadNativeCore()` 能在 CI 中实际加载到扩展。
- 本地验证示例：

```bash
pnpm exec napi build --release --platform --cargo-cwd native/nervusdb-node
PLATFORM=linux-x64-gnu # 将其替换为 darwin-arm64 / darwin-x64 / win32-x64-msvc 等实际平台
mkdir -p native/nervusdb-node/npm/${PLATFORM}
mv native/nervusdb-node/npm/index.node native/nervusdb-node/npm/${PLATFORM}/index.node
pnpm vitest run tests/integration/native/native_binding.test.ts
```

执行完成后，`loadNativeCore()` 将优先从 `native/nervusdb-node/npm/${PLATFORM}/index.node` 加载原生模块。

---

## 🛠️ 本地构建

### 开发构建（未混淆）

```bash
pnpm build:dev
```

生成 `dist/` 目录，包含未压缩的 JavaScript 文件和完整的类型定义。

### 生产构建（混淆）

```bash
pnpm build
```

使用 `build.config.mjs` 配置：

- ✅ 代码混淆和压缩
- ✅ 单文件打包
- ✅ Tree shaking
- ❌ 不生成 source map

---

## 📦 发布流程

### 1. 测试构建

```bash
pnpm build
```

### 2. 验证产物

```bash
# 检查文件大小
ls -lh dist/index.mjs dist/cli/nervusdb.js

# 验证 CLI 可执行
node dist/cli/nervusdb.js --help

# 测试导入
node -e "import('./dist/index.mjs').then(m => console.log(Object.keys(m)))"
```

### 3. 发布到 npm

```bash
# 检查登录状态
npm whoami

# 发布
npm publish
```

**发布时只包含**：

- `dist/` 目录（混淆后的代码）
- `README.md`
- `LICENSE`

**不包含**：

- ❌ `src/` 目录（源代码）
- ❌ `tests/` 目录
- ❌ `.map` 文件（source maps）

---

## 🔍 混淆效果对比

### 源代码（readable）

```typescript
export class QueryBuilder {
  constructor(private store: PersistentStore) {}

  anchor(orientation: FrontierOrientation): QueryBuilder {
    // ...
  }
}
```

### 混淆后（obfuscated）

```javascript
var ir=Object.defineProperty;var et=(c,e)=>()=>(c&&(e=c(c=0)),e);
export{B as QueryBuilder,ae as PluginManager...}
```

---

## ⚙️ 构建配置

### build.config.mjs

```javascript
import { build } from 'esbuild';

await build({
  entryPoints: ['src/index.ts'],
  bundle: true, // 打包所有依赖
  platform: 'node', // Node.js 平台
  target: 'node18', // 目标版本
  format: 'esm', // ES 模块
  outfile: 'dist/index.mjs',
  minify: true, // 混淆和压缩 ✅
  sourcemap: false, // 不生成 source map ✅
  treeShaking: true, // 移除未使用代码 ✅
});
```

### tsconfig.build.json

仅用于生成类型定义（`.d.ts` 文件）：

```json
{
  "extends": "./tsconfig.json",
  "compilerOptions": {
    "declaration": true,
    "emitDeclarationOnly": true,
    "outDir": "dist"
  }
}
```

---

## 🚫 反编译难度

### 混淆效果评估

| 方面             | 难度       | 说明                    |
| ---------------- | ---------- | ----------------------- |
| **变量名恢复**   | ⭐⭐⭐⭐⭐ | 变量名被缩短，难以理解  |
| **代码结构理解** | ⭐⭐⭐⭐   | 单行压缩，难以阅读      |
| **逻辑还原**     | ⭐⭐⭐     | 核心逻辑仍可反推        |
| **完全保护**     | ❌         | JavaScript 无法完全保护 |

### 注意事项

⚠️ **JavaScript 代码无法完全保护**

即使经过混淆，有经验的开发者仍可能：

1. 使用代码美化工具（如 Prettier）格式化
2. 分析运行时行为
3. 反推核心算法

**建议**：

- ✅ 核心算法可以混淆发布
- ✅ 商业逻辑可以保护
- ❌ 不应该将安全密钥硬编码在代码中
- ❌ 不应该依赖混淆作为唯一的保护手段

---

## 📊 对比其他方案

### 方案对比

| 方案                  | 保护程度   | 性能       | 开发体验   | 推荐    |
| --------------------- | ---------- | ---------- | ---------- | ------- |
| **esbuild 混淆**      | ⭐⭐⭐     | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐   | ✅ 推荐 |
| TypeScript 编译       | ⭐         | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | 基础    |
| UglifyJS              | ⭐⭐⭐     | ⭐⭐⭐     | ⭐⭐⭐     | 可选    |
| Terser                | ⭐⭐⭐     | ⭐⭐⭐⭐   | ⭐⭐⭐⭐   | 可选    |
| JavaScript Obfuscator | ⭐⭐⭐⭐   | ⭐⭐       | ⭐⭐       | 过度    |
| WebAssembly           | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐   | ⭐         | 复杂    |

**NervusDB 选择 esbuild** 原因：

1. ✅ 构建速度极快
2. ✅ 内置 Tree Shaking
3. ✅ 原生支持 TypeScript
4. ✅ 配置简单
5. ✅ 混淆效果足够

---

## 🎓 参考

- [esbuild 文档](https://esbuild.github.io/)
- [Claude Code 发布策略](https://github.com/anthropics/claude-code)
- [npm 发布最佳实践](https://docs.npmjs.com/packages-and-modules/contributing-packages-to-the-registry)

---

**最后更新**: 2025-01-14

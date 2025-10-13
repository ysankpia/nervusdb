# NervusDB 构建与发布策略

**文档版本**: v1.0  
**更新日期**: 2025-01-14  
**参考**: Claude Code 发布方式

---

## 📦 当前发布策略

### **目标：像 Claude Code 一样发布**

**Claude Code 的方式**:

```
@anthropic-ai/claude-code/
├── cli.js          9.1MB  (高度混淆的单文件)
├── sdk.mjs         521KB  (高度混淆的单文件)
├── sdk.d.ts        14KB   (TypeScript 类型)
├── sdk-tools.d.ts  7.1KB  (工具类型)
├── package.json
├── README.md
└── LICENSE.md
```

**特点**:

- ✅ 只发布必要文件（6 个文件）
- ✅ 高度 bundle 和 minify
- ✅ 没有源码目录
- ✅ 变量名完全混淆
- ✅ 有趣的版权声明

---

## 🎯 NervusDB 优化策略

### **优化前（当前）**

```
dist/
├── index.mjs          151KB  ✅ 已 minify
├── cli/nervusdb.js    小     ✅ 已 minify
├── index.d.ts
├── synapseDb.d.ts
├── algorithms/        ❌ 不需要发布
├── benchmark/         ❌ 不需要发布
├── cli/               ❌ 不需要发布
├── fulltext/          ❌ 不需要发布
├── graph/             ❌ 不需要发布
├── maintenance/       ❌ 不需要发布
├── plugins/           ❌ 不需要发布
├── query/             ❌ 不需要发布
├── spatial/           ❌ 不需要发布
├── storage/           ❌ 不需要发布
├── types/             ❌ 不需要发布
└── utils/             ❌ 不需要发布
```

**问题**: dist/ 包含太多子目录，虽然不影响功能，但不专业

---

### **优化后（目标）**

```
nervusdb/ (npm 包根目录)
├── index.mjs           ~150KB  (主库，已混淆)
├── cli.js              ~XKB    (CLI 工具，已混淆)
├── index.d.ts          (类型定义)
├── synapseDb.d.ts      (核心类型)
├── typedNervusDb.d.ts  (类型化 API)
├── package.json
├── README.md
└── LICENSE
```

**优势**:

- ✅ 干净简洁（8 个文件）
- ✅ 专业外观
- ✅ 文件扁平化
- ✅ 完全混淆保护

---

## 🔧 实施方案

### **1. 优化 build.config.mjs**

**关键改动**:

```javascript
// 主库输出到 dist/index.mjs
outfile: `${outdir}/index.mjs`;

// CLI 输出到 dist/cli.js（而不是 dist/cli/nervusdb.js）
outfile: `${outdir}/cli.js`;

// 只复制必要的类型定义文件
const typesToCopy = ['index.d.ts', 'synapseDb.d.ts', 'typedNervusDb.d.ts'];
```

---

### **2. 优化 package.json**

**关键改动**:

```json
{
  "main": "index.mjs", // 从 dist/index.mjs 改为 index.mjs
  "types": "index.d.ts", // 从 dist/index.d.ts 改为 index.d.ts
  "bin": {
    "nervusdb": "cli.js" // 从 dist/cli/nervusdb.js 改为 cli.js
  },
  "files": [
    "index.mjs", // 明确列出文件
    "cli.js",
    "index.d.ts",
    "synapseDb.d.ts",
    "typedNervusDb.d.ts",
    "README.md",
    "LICENSE"
  ]
}
```

**为什么这样改？**:

- npm 发布时，files 中的路径是相对于包根目录的
- dist/ 目录在本地开发，但发布时文件直接在根目录
- 这样用户 `npm install nervusdb` 后的结构更干净

---

### **3. npm 发布流程**

```bash
# Step 1: 构建
pnpm build

# Step 2: 检查发布内容（重要！）
npm pack --dry-run

# 输出示例：
# npm notice 📦  nervusdb@1.1.0
# npm notice === Tarball Contents ===
# npm notice 151KB index.mjs
# npm notice 45KB  cli.js
# npm notice 1.5KB index.d.ts
# npm notice 7.7KB synapseDb.d.ts
# npm notice 4.2KB typedNervusDb.d.ts
# npm notice 5.0KB README.md
# npm notice 1.0KB LICENSE
# npm notice === Tarball Details ===
# npm notice name:          nervusdb
# npm notice version:       1.1.0
# npm notice package size:  75.0 KB
# npm notice unpacked size: 216.4 KB
# npm notice total files:   7

# Step 3: 实际打包（用于测试）
npm pack

# 这会生成 nervusdb-1.1.0.tgz
# 解压检查：
tar -xzf nervusdb-1.1.0.tgz
ls -la package/
# 应该只看到 7 个文件，没有 dist/ 目录

# Step 4: 本地测试
cd ../test-project
npm install ../nervusdb/nervusdb-1.1.0.tgz
node -e "const {NervusDB} = require('nervusdb'); console.log(NervusDB)"

# Step 5: 发布到 npm
npm publish
```

---

## 📋 发布前检查清单

### **代码质量**

- [ ] 所有测试通过 (`pnpm test`)
- [ ] TypeScript 编译无错 (`pnpm typecheck`)
- [ ] Lint 检查通过 (`pnpm lint`)
- [ ] 构建成功 (`pnpm build`)

### **包内容检查**

- [ ] `npm pack --dry-run` 输出正确
- [ ] 只包含 7 个必要文件
- [ ] index.mjs 已 minify（检查文件内容）
- [ ] cli.js 已 minify
- [ ] cli.js 有执行权限（`#!/usr/bin/env node`）

### **文档完整**

- [ ] README.md 包含安装和使用说明
- [ ] LICENSE 文件存在
- [ ] CHANGELOG.md 更新
- [ ] package.json 版本号正确

### **测试安装**

- [ ] `npm pack` 生成 .tgz
- [ ] 解压 .tgz 检查内容
- [ ] 在新项目中安装测试
- [ ] CLI 命令可以运行 (`nervusdb --help`)
- [ ] API 可以 import

---

## 🎨 版权声明优化

### **参考 Claude Code**

```javascript
// cli.js 头部（Claude Code 风格）
#!/usr/bin/env node
// (c) NervusDB Team. All rights reserved.
// Version: 1.1.0

// Want to see the unminified source? Check out:
// https://github.com/YourUsername/nervusdb
```

```javascript
// index.mjs 头部
// NervusDB - Neural Knowledge Graph Database
// (c) 2025. All rights reserved.
// Version: 1.1.0

// Want to contribute? We welcome pull requests!
// https://github.com/YourUsername/nervusdb
```

**有趣且专业**:

- ✅ 明确版权
- ✅ 引导开源贡献
- ✅ 类似大公司的风格

---

## 📊 对比总结

| 项目           | 优化前   | 优化后 | 改进        |
| -------------- | -------- | ------ | ----------- |
| **发布文件数** | ~100 个  | 7 个   | ✅ 减少 93% |
| **包大小**     | ~500KB   | ~220KB | ✅ 减少 56% |
| **目录结构**   | 多层嵌套 | 扁平化 | ✅ 更专业   |
| **源码保护**   | 混淆     | 混淆   | ✅ 已实现   |
| **安装体验**   | 较慢     | 快速   | ✅ 文件少   |

---

## 🚀 下一步行动

### **立即执行**（本次）

1. 更新 build.config.mjs
2. 更新 package.json
3. 运行 `pnpm build`
4. 检查 dist/ 输出
5. 测试 `npm pack`

### **发布前**（社区版发布时）

1. 更新 README.md
2. 添加 LICENSE 文件
3. 更新 CHANGELOG.md
4. 设置 npm 账号
5. 决定包名（nervusdb 是否可用）

### **发布后**

1. 创建 GitHub Release
2. 发布公告
3. 更新文档网站
4. 社区宣传

---

## 💡 额外优化建议

### **进一步压缩（可选）**

如果想要更小的包体积，可以：

```javascript
// build.config.mjs 中添加
minifyIdentifiers: true,    // 更激进的标识符混淆
minifySyntax: true,         // 语法简化
minifyWhitespace: true,     // 移除所有空白
drop: ['console', 'debugger'], // 移除 console 和 debugger
```

**注意**: 过度压缩可能影响调试，需要权衡。

---

### **.npmignore**（可选）

虽然 `files` 字段已经明确列出发布文件，但可以添加 `.npmignore` 作为额外保险：

```
# .npmignore
src/
tests/
docs/
scripts/
benchmarks/
*.test.ts
*.spec.ts
tsconfig.json
vitest.config.ts
.github/
.husky/
```

---

## 📝 常见问题

### **Q: 为什么不直接发布 dist/?**

**A**:

- 用户体验：`node_modules/nervusdb/dist/index.mjs` vs `node_modules/nervusdb/index.mjs`
- 专业性：扁平结构更像大公司的包
- 清晰性：用户只看到必要文件

### **Q: 类型定义文件会丢失吗？**

**A**:
不会，我们明确复制了 3 个必要的 `.d.ts` 文件到 dist/ 根目录。

### **Q: 这样修改会破坏现有用户吗？**

**A**:
不会，因为：

- 暂时还没有发布到 npm
- 本地开发仍然使用 `pnpm build`
- 只是改变发布包的结构

### **Q: 如何回滚？**

**A**:
Git 回滚即可：

```bash
git checkout HEAD~1 build.config.mjs package.json
pnpm build
```

---

**文档维护**: 本文档应在每次构建策略变更时更新

**最后更新**: 2025-01-14  
**状态**: 📋 待实施

/**
 * NervusDB Build Configuration
 * 使用 esbuild 打包和混淆代码，类似 Claude Code 的发布方式
 */

import { build } from 'esbuild';
import fs from 'fs';
import path from 'path';

const outdir = 'dist';

async function buildBundle() {
  console.log('🔨 Building NervusDB...');

  // 清理旧的 dist
  if (fs.existsSync(outdir)) {
    fs.rmSync(outdir, { recursive: true });
  }
  fs.mkdirSync(outdir, { recursive: true });

  // 1. 构建主库 (ESM) - 所有依赖打包成单文件
  await build({
    entryPoints: ['src/index.ts'],
    bundle: true,
    platform: 'node',
    target: 'node18',
    format: 'esm',
    outfile: `${outdir}/index.mjs`,
    minify: true, // 混淆和压缩
    sourcemap: false, // 不生成 source map
    treeShaking: true, // 移除未使用代码
    keepNames: false, // 不保留函数名（更强混淆）
    legalComments: 'none', // 移除注释
    external: [
      // 不打包的外部依赖（如果有）
    ],
    banner: {
      js: '// NervusDB - Neural Knowledge Graph Database\n// (c) 2025. All rights reserved.\n// Version: 1.1.0\n\n// Want to see the unminified source? Check out https://github.com/YourRepo/nervusdb\n',
    },
  });

  // 2. 构建 CLI (单独打包，包含所有依赖)
  await build({
    entryPoints: ['src/cli/nervusdb.ts'],
    bundle: true,
    platform: 'node',
    target: 'node18',
    format: 'esm',
    outfile: `${outdir}/cli.js`,
    minify: true,
    sourcemap: false,
    treeShaking: true,
    keepNames: false,
    legalComments: 'none',
    banner: {
      js: '#!/usr/bin/env node\n// NervusDB CLI\n// (c) 2025. All rights reserved.\n',
    },
  });

  // 3. 生成类型定义文件（只生成必要的 .d.ts）
  console.log('📝 Generating type definitions...');
  const { execSync } = await import('child_process');
  
  // 使用 tsc 生成所有类型定义到临时目录
  execSync('tsc --project tsconfig.build.json --emitDeclarationOnly --outDir dist-types', {
    stdio: 'inherit',
  });

  // 只复制主要的类型定义文件到 dist
  const typesToCopy = [
    'index.d.ts',
    'synapseDb.d.ts',
    'typedNervusDb.d.ts',
  ];

  for (const file of typesToCopy) {
    const src = `dist-types/${file}`;
    const dest = `${outdir}/${file}`;
    if (fs.existsSync(src)) {
      fs.copyFileSync(src, dest);
      console.log(`  ✓ Copied ${file}`);
    }
  }

  // 清理临时类型定义目录
  fs.rmSync('dist-types', { recursive: true });

  // 4. 设置 CLI 可执行权限
  fs.chmodSync(`${outdir}/cli.js`, 0o755);

  // 5. 显示构建结果
  const stats = {
    'index.mjs': fs.statSync(`${outdir}/index.mjs`).size,
    'cli.js': fs.statSync(`${outdir}/cli.js`).size,
  };

  console.log('\n✅ Build complete!');
  console.log(`📦 Output: ${outdir}/`);
  console.log('\n📊 Bundle sizes:');
  console.log(`  - index.mjs: ${(stats['index.mjs'] / 1024).toFixed(1)} KB`);
  console.log(`  - cli.js: ${(stats['cli.js'] / 1024).toFixed(1)} KB`);
  console.log(`  - Total: ${((stats['index.mjs'] + stats['cli.js']) / 1024).toFixed(1)} KB`);
  console.log('\n📋 Published files:');
  console.log('  - index.mjs (main library)');
  console.log('  - cli.js (CLI tool)');
  console.log('  - index.d.ts (TypeScript types)');
  console.log('  - synapseDb.d.ts (Core types)');
  console.log('  - typedNervusDb.d.ts (Typed API)');
}

buildBundle().catch((err) => {
  console.error('❌ Build failed:', err);
  process.exit(1);
});

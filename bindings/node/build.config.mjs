/**
 * NervusDB Build Configuration
 * 方案1: 构建多个独立的 CLI 文件
 */

import { build } from 'esbuild';
import fs from 'fs';

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
    minify: true,
    sourcemap: false,
    treeShaking: true,
    keepNames: false,
    legalComments: 'none',
    external: [],
    banner: {
      js: '// NervusDB - Neural Knowledge Graph Database\n// (c) 2025. All rights reserved.\n// Version: 1.1.1\n\n',
    },
  });

  // 2. 构建 CLI 子命令 (v2.0 - Native only)
  const cliFiles = [
    'nervusdb.ts', // 主入口
    'cypher.ts', // Cypher 查询工具
    'bench.ts', // 快速性能测试
    'benchmark.ts', // 完整基准测试
  ];

  console.log('📝 Building CLI commands...');

  for (const file of cliFiles) {
    const isEntry = file === 'nervusdb.ts';
    const outFile = file.replace('.ts', '.js');

    await build({
      entryPoints: [`src/cli/${file}`],
      bundle: true,
      platform: 'node',
      target: 'node18',
      format: 'esm',
      outfile: `${outdir}/${outFile}`,
      minify: true,
      sourcemap: false,
      treeShaking: true,
      keepNames: false,
      legalComments: 'none',
      external: [],
      banner: {
        js: isEntry
          ? '#!/usr/bin/env node\n// NervusDB CLI\n// (c) 2025. All rights reserved.\n'
          : '// NervusDB CLI sub-command\n',
      },
    });

    // 只为主入口设置可执行权限
    if (isEntry) {
      fs.chmodSync(`${outdir}/${outFile}`, 0o755);
    }

    console.log(`  ✓ Built ${outFile}`);
  }

  // 3. 生成类型定义文件
  console.log('📝 Generating type definitions...');
  const { execSync } = await import('child_process');

  execSync('tsc --project tsconfig.build.json --emitDeclarationOnly --outDir dist-types', {
    stdio: 'inherit',
  });

  const typesToCopy = ['index.d.ts', 'synapseDb.d.ts', 'typedNervusDb.d.ts'];

  for (const file of typesToCopy) {
    const src = `dist-types/${file}`;
    const dest = `${outdir}/${file}`;
    if (fs.existsSync(src)) {
      fs.copyFileSync(src, dest);
      console.log(`  ✓ Copied ${file}`);
    }
  }

  fs.rmSync('dist-types', { recursive: true });

  // 4. 显示构建结果
  const distFiles = fs.readdirSync(outdir);
  const jsFiles = distFiles.filter((f) => f.endsWith('.js') || f.endsWith('.mjs'));
  const totalSize = jsFiles.reduce((sum, f) => sum + fs.statSync(`${outdir}/${f}`).size, 0);

  console.log('\n✅ Build complete!');
  console.log(`📦 Output: ${outdir}/`);
  console.log('\n📊 Bundle sizes:');
  console.log(`  - index.mjs: ${(fs.statSync(`${outdir}/index.mjs`).size / 1024).toFixed(1)} KB`);
  console.log(
    `  - CLI files: ${jsFiles.length} files, ${((totalSize - fs.statSync(`${outdir}/index.mjs`).size) / 1024).toFixed(1)} KB`,
  );
  console.log(`  - Total: ${(totalSize / 1024).toFixed(1)} KB`);
  console.log('\n📋 Published files:');
  console.log(`  - index.mjs (main library)`);
  console.log(`  - nervusdb.js + ${jsFiles.length - 1} CLI sub-commands`);
  console.log(`  - 3 TypeScript definition files`);
}

buildBundle().catch((err) => {
  console.error('❌ Build failed:', err);
  process.exit(1);
});

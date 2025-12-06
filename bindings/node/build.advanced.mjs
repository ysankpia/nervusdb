/**
 * NervusDB Advanced Build Configuration
 * 使用 javascript-obfuscator 提供更强的代码保护
 */

import { build } from 'esbuild';
import JavaScriptObfuscator from 'javascript-obfuscator';
import fs from 'fs';
import path from 'path';

const outdir = 'dist';

async function buildWithAdvancedObfuscation() {
  console.log('🔨 Building NervusDB with advanced obfuscation...');

  // 清理旧的 dist
  if (fs.existsSync(outdir)) {
    fs.rmSync(outdir, { recursive: true });
  }

  // 1. 首先用 esbuild 打包
  console.log('📦 Step 1: Bundling with esbuild...');
  
  await build({
    entryPoints: ['src/index.ts'],
    bundle: true,
    platform: 'node',
    target: 'node18',
    format: 'esm',
    outfile: `${outdir}/index.bundled.mjs`,
    minify: false, // 先不压缩，让 obfuscator 处理
    sourcemap: false,
    treeShaking: true,
    banner: {
      js: '// NervusDB - Neural Knowledge Graph Database\n// (c) 2025. All rights reserved.\n',
    },
  });

  // 2. 使用 javascript-obfuscator 进行高级混淆
  console.log('🔒 Step 2: Advanced obfuscation...');
  
  const bundledCode = fs.readFileSync(`${outdir}/index.bundled.mjs`, 'utf8');
  
  const obfuscationResult = JavaScriptObfuscator.obfuscate(bundledCode, {
    // 高级混淆配置
    compact: true, // 压缩代码
    controlFlowFlattening: true, // 控制流扁平化
    controlFlowFlatteningThreshold: 0.75,
    deadCodeInjection: true, // 注入死代码
    deadCodeInjectionThreshold: 0.4,
    debugProtection: false, // 反调试（谨慎使用，可能影响正常调试）
    debugProtectionInterval: 0,
    disableConsoleOutput: false, // 禁用 console（生产环境可启用）
    identifierNamesGenerator: 'hexadecimal', // 变量名生成策略
    log: false,
    numbersToExpressions: true, // 数字转表达式
    renameGlobals: false, // 不重命名全局变量（避免破坏依赖）
    selfDefending: true, // 自我防御
    simplify: true,
    splitStrings: true, // 分割字符串
    splitStringsChunkLength: 10,
    stringArray: true, // 字符串数组化
    stringArrayCallsTransform: true,
    stringArrayEncoding: ['base64'], // 字符串编码
    stringArrayIndexShift: true,
    stringArrayRotate: true,
    stringArrayShuffle: true,
    stringArrayWrappersCount: 2,
    stringArrayWrappersChainedCalls: true,
    stringArrayWrappersParametersMaxCount: 4,
    stringArrayWrappersType: 'function',
    stringArrayThreshold: 0.75,
    transformObjectKeys: true, // 转换对象键
    unicodeEscapeSequence: false, // 不使用 Unicode 转义（保持可读性）
  });

  fs.writeFileSync(`${outdir}/index.mjs`, obfuscationResult.getObfuscatedCode());
  fs.unlinkSync(`${outdir}/index.bundled.mjs`); // 删除临时文件

  // 3. CLI 也进行混淆
  console.log('🔒 Step 3: Obfuscating CLI...');
  
  await build({
    entryPoints: ['src/cli/nervusdb.ts'],
    bundle: true,
    platform: 'node',
    target: 'node18',
    format: 'esm',
    outfile: `${outdir}/cli/nervusdb.bundled.js`,
    minify: false,
    sourcemap: false,
    treeShaking: true,
    banner: {
      js: '#!/usr/bin/env node\n// NervusDB CLI\n// (c) 2025. All rights reserved.\n',
    },
  });

  const cliCode = fs.readFileSync(`${outdir}/cli/nervusdb.bundled.js`, 'utf8');
  const cliObfuscated = JavaScriptObfuscator.obfuscate(cliCode, {
    compact: true,
    controlFlowFlattening: true,
    controlFlowFlatteningThreshold: 0.5,
    deadCodeInjection: true,
    deadCodeInjectionThreshold: 0.3,
    identifierNamesGenerator: 'hexadecimal',
    stringArray: true,
    stringArrayEncoding: ['base64'],
    stringArrayThreshold: 0.75,
  });

  fs.writeFileSync(`${outdir}/cli/nervusdb.js`, cliObfuscated.getObfuscatedCode());
  fs.unlinkSync(`${outdir}/cli/nervusdb.bundled.js`);

  // 4. 生成类型定义
  console.log('📝 Step 4: Generating type definitions...');
  const { execSync } = await import('child_process');
  execSync('tsc --project tsconfig.build.json --emitDeclarationOnly', {
    stdio: 'inherit',
  });

  // 5. 设置可执行权限
  fs.chmodSync(`${outdir}/cli/nervusdb.js`, 0o755);

  console.log('✅ Advanced obfuscation complete!');
  console.log(`📦 Output: ${outdir}/`);
  console.log('⚠️  Note: Obfuscated code may be 2-3x larger and 15-80% slower');
}

buildWithAdvancedObfuscation().catch((err) => {
  console.error('❌ Build failed:', err);
  process.exit(1);
});

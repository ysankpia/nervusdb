#!/usr/bin/env node
import { autoCompact } from '../maintenance/autoCompact.js';

async function main() {
  const [dbPath, ...args] = process.argv.slice(2);
  if (!dbPath) {
    console.log('用法: pnpm db:auto-compact <db> [选项]');
    console.log('选项:');
    console.log('  --orders=SPO,POS        指定要分析的索引顺序（默认全部）');
    console.log('  --min-merge=2           最小合并页数阈值（默认2）');
    console.log('  --mode=incremental      压缩模式: incremental | rewrite（默认incremental）');
    console.log('  --hot-threshold=N       热度阈值，仅增量模式生效（默认不限制）');
    console.log('  --max-primary=N         每个顺序最多重写的primary数（默认不限制）');
    console.log('  --dry-run               显式模拟运行（默认即为 dry-run）');
    console.log('  --force                 真正执行压缩（默认不会修改磁盘）');
    console.log('  --auto-gc               压缩后自动运行垃圾回收');
    console.log('  --no-respect-readers    即使有活跃读者也执行压缩');
    console.log('  --quiet                 减少日志输出，仅显示关键信息');
    console.log('  --verbose               显示详细的分析和决策过程（默认）');
    process.exit(1);
  }
  const opts: Record<string, string | boolean> = {};
  for (const a of args) {
    const [k, v] = a.startsWith('--') ? a.substring(2).split('=') : [a, 'true'];
    opts[k] = v === undefined ? true : v;
  }
  const toBool = (value: string | boolean | undefined): boolean =>
    value === true || value === 'true';
  const isExplicitFalse = (value: string | boolean | undefined): boolean =>
    value === false || value === 'false';
  const mode = (opts['mode'] as 'rewrite' | 'incremental' | undefined) ?? 'incremental';
  const minMergePages = opts['min-merge'] ? Number(opts['min-merge']) : undefined;
  // 安全默认：干跑，只有 --force 或 --dry-run=false 才执行
  const dryRun = toBool(opts['force']) ? false : isExplicitFalse(opts['dry-run']) ? false : true;
  const orders = typeof opts['orders'] === 'string' ? String(opts['orders']).split(',') : undefined;
  const hotThreshold = opts['hot-threshold'] ? Number(opts['hot-threshold']) : undefined;
  const maxPrimariesPerOrder = opts['max-primary'] ? Number(opts['max-primary']) : undefined;
  const autoGC = Boolean(opts['auto-gc']);
  const quiet = Boolean(opts['quiet']);
  // const verbose = Boolean(opts['verbose']) || !quiet; // 默认详细输出 (unused variable)

  const respectReaders = !opts['no-respect-readers'];

  // 设置全局日志级别（简单方式）
  if (quiet) {
    const originalLog = console.log;
    console.log = (...args: unknown[]) => {
      // 只输出以特定前缀开头的重要信息
      const message = args[0];
      if (
        typeof message === 'string' &&
        (message.startsWith('🔧') ||
          message.startsWith('✅') ||
          message.startsWith('❌') ||
          message.startsWith('⚠️') ||
          message.includes('Final compaction decision') ||
          message.includes('Compaction completed') ||
          message.includes('Auto-compact finished'))
      ) {
        originalLog(...(args as Parameters<typeof originalLog>));
      }
    };
  }

  const result = await autoCompact(dbPath, {
    mode,
    minMergePages,
    dryRun,
    orders: orders as any,
    hotThreshold,
    maxPrimariesPerOrder,
    autoGC,
    respectReaders,
  });

  if (!quiet) {
    console.log('\n📋 Compaction result summary:');
  }
  console.log(JSON.stringify(result, null, 2));
}

// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();

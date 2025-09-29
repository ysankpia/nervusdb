#!/usr/bin/env node
import { compactDatabase, type IndexOrder } from '../maintenance/compaction.js';

async function main() {
  const [dbPath, ...args] = process.argv.slice(2);
  if (!dbPath) {
    console.log(
      '用法: pnpm db:compact <db> [--orders=SPO,POS] [--page-size=1024] [--min-merge=2] [--tombstone-threshold=0.2] [--compression=brotli:4|none] [--force]',
    );
    process.exit(1);
  }
  const opts: Record<string, string | boolean> = {};
  for (const a of args) {
    const [k, v] = a.startsWith('--') ? a.substring(2).split('=') : [a, 'true'];
    opts[k] = v === undefined ? true : v;
  }
  const orders: IndexOrder[] | undefined =
    typeof opts['orders'] === 'string'
      ? (String(opts['orders']).split(',').filter(Boolean) as IndexOrder[])
      : undefined;
  const pageSize = opts['page-size'] ? Number(opts['page-size']) : undefined;
  const minMergePages = opts['min-merge'] ? Number(opts['min-merge']) : undefined;
  const tombstoneRatioThreshold = opts['tombstone-threshold']
    ? Number(opts['tombstone-threshold'])
    : undefined;
  // 安全默认：dry-run 默认开启；只有 --force 显式关闭
  const dryRun = opts['dry-run'] === true ? true : opts['force'] === true ? false : true;
  let compression: { codec: 'none' | 'brotli'; level?: number } | undefined;
  if (typeof opts['compression'] === 'string') {
    const raw = String(opts['compression']);
    if (raw === 'none') compression = { codec: 'none' };
    else if (raw.startsWith('brotli')) {
      const [, levelStr] = raw.split(':');
      const level = levelStr ? Number(levelStr) : 4;
      compression = { codec: 'brotli', level };
    }
  }

  // 解析 only-primaries，格式：SPO:1,2;POS:3
  let onlyPrimaries: Record<string, number[]> | undefined;
  if (typeof opts['only-primaries'] === 'string') {
    onlyPrimaries = {};
    const groups = String(opts['only-primaries']).split(';').filter(Boolean);
    for (const g of groups) {
      const [ord, list] = g.split(':');
      if (!ord || !list) continue;
      const nums = list
        .split(',')
        .map((x) => Number(x.trim()))
        .filter((n) => Number.isFinite(n));
      if (nums.length > 0) (onlyPrimaries as any)[ord] = nums;
    }
  }

  const stats = await compactDatabase(dbPath, {
    orders,
    pageSize,
    minMergePages,
    tombstoneRatioThreshold,
    dryRun,
    compression,
    mode: (opts['mode'] as 'rewrite' | 'incremental' | undefined) ?? 'rewrite',
    onlyPrimaries: onlyPrimaries as any,
  });
  console.log(JSON.stringify(stats, null, 2));
}

// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();

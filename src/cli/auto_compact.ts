import { autoCompact } from '../maintenance/autoCompact';

async function main() {
  const [dbPath, ...args] = process.argv.slice(2);
  if (!dbPath) {
    console.log('用法: pnpm db:auto-compact <db> [--orders=SPO,POS] [--min-merge=2] [--mode=incremental] [--dry-run]');
    process.exit(1);
  }
  const opts: Record<string, string | boolean> = {};
  for (const a of args) {
    const [k, v] = a.startsWith('--') ? a.substring(2).split('=') : [a, 'true'];
    opts[k] = v === undefined ? true : v;
  }
  const mode = (opts['mode'] as 'rewrite' | 'incremental' | undefined) ?? 'incremental';
  const minMergePages = opts['min-merge'] ? Number(opts['min-merge']) : undefined;
  const dryRun = Boolean(opts['dry-run']);
  const orders = typeof opts['orders'] === 'string' ? String(opts['orders']).split(',') : undefined;
  const hotThreshold = opts['hot-threshold'] ? Number(opts['hot-threshold']) : undefined;
  const maxPrimariesPerOrder = opts['max-primary'] ? Number(opts['max-primary']) : undefined;
  const autoGC = Boolean(opts['auto-gc']);

  const respectReaders = Boolean(opts['respect-readers']);
  const result = await autoCompact(dbPath, { mode, minMergePages, dryRun, orders: orders as any, hotThreshold, maxPrimariesPerOrder, autoGC, respectReaders });
  console.log(JSON.stringify(result, null, 2));
}

// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();

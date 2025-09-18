import { compactDatabase } from '../maintenance/compaction';
async function main() {
    const [dbPath, ...args] = process.argv.slice(2);
    if (!dbPath) {
        console.log('用法: pnpm db:compact <db> [--orders=SPO,POS] [--page-size=1024] [--min-merge=2] [--tombstone-threshold=0.2] [--dry-run] [--compression=brotli:4|none]');
        process.exit(1);
    }
    const opts = {};
    for (const a of args) {
        const [k, v] = a.startsWith('--') ? a.substring(2).split('=') : [a, 'true'];
        opts[k] = v === undefined ? true : v;
    }
    const orders = typeof opts['orders'] === 'string'
        ? String(opts['orders']).split(',').filter(Boolean)
        : undefined;
    const pageSize = opts['page-size'] ? Number(opts['page-size']) : undefined;
    const minMergePages = opts['min-merge'] ? Number(opts['min-merge']) : undefined;
    const tombstoneRatioThreshold = opts['tombstone-threshold'] ? Number(opts['tombstone-threshold']) : undefined;
    const dryRun = Boolean(opts['dry-run']);
    let compression;
    if (typeof opts['compression'] === 'string') {
        const raw = String(opts['compression']);
        if (raw === 'none')
            compression = { codec: 'none' };
        else if (raw.startsWith('brotli')) {
            const [, levelStr] = raw.split(':');
            const level = levelStr ? Number(levelStr) : 4;
            compression = { codec: 'brotli', level };
        }
    }
    const stats = await compactDatabase(dbPath, {
        orders,
        pageSize,
        minMergePages,
        tombstoneRatioThreshold,
        dryRun,
        compression,
        mode: opts['mode'] ?? 'rewrite',
    });
    console.log(JSON.stringify(stats, null, 2));
}
// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();
//# sourceMappingURL=compact.js.map
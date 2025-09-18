import { readHotness } from '../storage/hotness';
async function main() {
    const [dbPath, ...args] = process.argv.slice(2);
    if (!dbPath) {
        console.log('用法: pnpm db:hot <db> [--order=SPO] [--top=10]');
        process.exit(1);
    }
    const opts = {};
    for (const a of args) {
        const [k, v] = a.startsWith('--') ? a.substring(2).split('=') : [a, ''];
        opts[k] = v ?? '';
    }
    const order = (opts['order'] ?? 'SPO');
    const top = Number(opts['top'] ?? '10');
    const hot = await readHotness(`${dbPath}.pages`);
    const counts = hot.counts[order] ?? {};
    const sorted = Object.entries(counts).sort((a, b) => b[1] - a[1]).slice(0, top);
    const out = sorted.map(([primary, count]) => ({ primary: Number(primary), count }));
    console.log(JSON.stringify({ order, updatedAt: hot.updatedAt, top: out }, null, 2));
}
// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();
//# sourceMappingURL=hot.js.map
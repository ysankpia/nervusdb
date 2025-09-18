#!/usr/bin/env node
import { garbageCollectPages } from '../maintenance/gc.js';
async function main() {
    const [dbPath, ...args] = process.argv.slice(2);
    if (!dbPath) {
        console.log('用法: pnpm db:gc <db> [--no-respect-readers]');
        process.exit(1);
    }
    const respect = !args.includes('--no-respect-readers');
    const stats = await garbageCollectPages(dbPath, { respectReaders: respect });
    console.log(JSON.stringify(stats, null, 2));
}
// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();
//# sourceMappingURL=gc.js.map
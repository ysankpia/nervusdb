import { garbageCollectPages } from '../maintenance/gc';
async function main() {
    const [dbPath] = process.argv.slice(2);
    if (!dbPath) {
        console.log('用法: pnpm db:gc <db>');
        process.exit(1);
    }
    const stats = await garbageCollectPages(dbPath);
    console.log(JSON.stringify(stats, null, 2));
}
// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();
//# sourceMappingURL=gc.js.map
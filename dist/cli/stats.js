import { promises as fs } from 'node:fs';
import { readStorageFile } from '../storage/fileHeader';
import { readPagedManifest } from '../storage/pagedIndex';
async function stats(dbPath) {
    const sections = await readStorageFile(dbPath);
    const dictCount = sections.dictionary.length >= 4 ? sections.dictionary.readUInt32LE(0) : 0;
    const tripleCount = sections.triples.length >= 4 ? sections.triples.readUInt32LE(0) : 0;
    const indexDir = `${dbPath}.pages`;
    const manifest = await readPagedManifest(indexDir);
    const lookups = manifest?.lookups ?? [];
    const epoch = manifest?.epoch ?? 0;
    const tombstones = manifest?.tombstones?.length ?? 0;
    let pageFiles = 0;
    let pages = 0;
    const orders = {};
    for (const l of lookups) {
        pageFiles += 1;
        pages += l.pages.length;
        const cnt = new Map();
        for (const p of l.pages)
            cnt.set(p.primaryValue, (cnt.get(p.primaryValue) ?? 0) + 1);
        const multi = [...cnt.values()].filter((c) => c > 1).length;
        orders[l.order] = { pages: l.pages.length, primaries: cnt.size, multiPagePrimaries: multi };
    }
    let walSize = 0;
    try {
        const st = await fs.stat(`${dbPath}.wal`);
        walSize = st.size;
    }
    catch { }
    const out = {
        dictionaryEntries: dictCount,
        triples: tripleCount,
        epoch,
        pageFiles,
        pages,
        tombstones,
        walBytes: walSize,
        orders,
    };
    console.log(JSON.stringify(out, null, 2));
}
async function main() {
    const [dbPath] = process.argv.slice(2);
    if (!dbPath) {
        console.log('用法: pnpm db:stats <db>');
        process.exit(1);
    }
    await stats(dbPath);
}
// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();
//# sourceMappingURL=stats.js.map
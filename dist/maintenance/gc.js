import { promises as fs } from 'node:fs';
import { join } from 'node:path';
import { readPagedManifest, writePagedManifest, pageFileName } from '../storage/pagedIndex';
export async function garbageCollectPages(dbPath) {
    const indexDir = `${dbPath}.pages`;
    const manifest = await readPagedManifest(indexDir);
    if (!manifest)
        throw new Error('缺少 manifest，无法进行 GC');
    let bytesBefore = 0;
    let bytesAfter = 0;
    const orderStats = [];
    for (const lookup of manifest.lookups) {
        const file = join(indexDir, pageFileName(lookup.order));
        let st;
        try {
            st = await fs.stat(file);
        }
        catch {
            orderStats.push({ order: lookup.order, bytesBefore: 0, bytesAfter: 0, pages: lookup.pages.length });
            continue;
        }
        bytesBefore += st.size;
        const tmp = `${file}.gc.tmp`;
        try {
            await fs.unlink(tmp);
        }
        catch { }
        const src = await fs.open(file, 'r');
        const dst = await fs.open(tmp, 'w');
        let offset = 0;
        const newPages = [];
        try {
            for (const page of lookup.pages) {
                const buf = Buffer.allocUnsafe(page.length);
                await src.read(buf, 0, page.length, page.offset);
                await dst.write(buf, 0, buf.length, offset);
                newPages.push({ primaryValue: page.primaryValue, offset, length: page.length, rawLength: page.rawLength, crc32: page.crc32 });
                offset += page.length;
            }
            await dst.sync();
        }
        finally {
            await src.close();
            await dst.close();
        }
        await fs.rename(tmp, file);
        // 更新该顺序的 pages 映射（offset 变化）
        lookup.pages = newPages;
        const stAfter = await fs.stat(file);
        bytesAfter += stAfter.size;
        orderStats.push({ order: lookup.order, bytesBefore: st.size, bytesAfter: stAfter.size, pages: newPages.length });
    }
    const newManifest = {
        ...manifest,
        epoch: (manifest.epoch ?? 0) + 1,
    };
    await writePagedManifest(indexDir, newManifest);
    return { orders: orderStats, bytesBefore, bytesAfter };
}
//# sourceMappingURL=gc.js.map
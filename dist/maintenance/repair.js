import { promises as fs } from 'node:fs';
import { join } from 'node:path';
import { checkStrict } from './check.js';
import { PagedIndexWriter, pageFileName, readPagedManifest, writePagedManifest, } from '../storage/pagedIndex.js';
import { SynapseDB } from '../synapseDb.js';
export async function repairCorruptedOrders(dbPath) {
    const indexDir = `${dbPath}.pages`;
    const manifest = await readPagedManifest(indexDir);
    if (!manifest) {
        throw new Error('缺少 manifest，无法修复');
    }
    const strict = await checkStrict(dbPath);
    if (strict.ok)
        return { repairedOrders: [] };
    const badOrders = new Set(strict.errors.map((e) => e.order));
    const repairedOrders = [];
    const newLookups = [];
    // 从主文件获取“权威”三元组集合，避免因坏页导致数据丢失
    const db = await SynapseDB.open(dbPath);
    const all = db.listFacts();
    for (const lookup of manifest.lookups) {
        if (!badOrders.has(lookup.order)) {
            newLookups.push(lookup);
            continue;
        }
        // 直接重写整个顺序，不再单独处理 primaries
        const tmpFile = join(indexDir, `${pageFileName(lookup.order)}.tmp`);
        try {
            await fs.unlink(tmpFile);
        }
        catch { }
        const writer = new PagedIndexWriter(tmpFile, {
            directory: indexDir,
            pageSize: manifest.pageSize,
            compression: manifest.compression,
        });
        // 直接使用主文件事实重建该顺序的页
        const getPrimary = (t) => lookup.order === 'SPO' || lookup.order === 'SOP'
            ? t.subjectId
            : lookup.order === 'POS' || lookup.order === 'PSO'
                ? t.predicateId
                : t.objectId;
        for (const f of all) {
            const t = { subjectId: f.subjectId, predicateId: f.predicateId, objectId: f.objectId };
            writer.push(t, getPrimary(t));
        }
        const pages = await writer.finalize();
        const dest = join(indexDir, pageFileName(lookup.order));
        try {
            await fs.unlink(dest);
        }
        catch { }
        try {
            await fs.rename(tmpFile, dest);
        }
        catch (e) {
            // 若无数据写入 tmpFile 可能不存在，创建空文件后再替换
            if (e.code === 'ENOENT') {
                await fs.writeFile(tmpFile, Buffer.alloc(0));
                await fs.rename(tmpFile, dest);
            }
            else {
                throw e;
            }
        }
        newLookups.push({ order: lookup.order, pages });
        repairedOrders.push(lookup.order);
    }
    const newManifest = {
        version: manifest.version,
        pageSize: manifest.pageSize,
        createdAt: Date.now(),
        compression: manifest.compression,
        lookups: newLookups,
        tombstones: manifest.tombstones,
    };
    await writePagedManifest(indexDir, newManifest);
    return { repairedOrders };
}
export async function repairCorruptedPagesFast(dbPath) {
    const indexDir = `${dbPath}.pages`;
    const manifest = await readPagedManifest(indexDir);
    if (!manifest)
        throw new Error('缺少 manifest，无法修复');
    const strict = await checkStrict(dbPath);
    if (strict.ok)
        return { repaired: [] };
    const errorGroups = new Map();
    for (const e of strict.errors) {
        if (e.order === '*' || e.primaryValue < 0)
            continue;
        const set = errorGroups.get(e.order) ?? new Set();
        set.add(e.primaryValue);
        errorGroups.set(e.order, set);
    }
    const db = await SynapseDB.open(dbPath);
    const facts = db.listFacts();
    const repaired = [];
    const getPrimary = (order, t) => order === 'SPO' || order === 'SOP'
        ? t.subjectId
        : order === 'POS' || order === 'PSO'
            ? t.predicateId
            : t.objectId;
    for (const [order, primaries] of errorGroups.entries()) {
        const lookup = manifest.lookups.find((l) => l.order === order);
        if (!lookup)
            continue;
        const writer = new PagedIndexWriter(join(indexDir, pageFileName(order)), {
            directory: indexDir,
            pageSize: manifest.pageSize,
            compression: manifest.compression,
        });
        const primariesArr = [...primaries.values()];
        for (const p of primariesArr) {
            const vf = facts.filter((f) => getPrimary(order, f) === p);
            // 稳定排序
            vf.sort((a, b) => a.subjectId - b.subjectId || a.predicateId - b.predicateId || a.objectId - b.objectId);
            for (const f of vf)
                writer.push({ subjectId: f.subjectId, predicateId: f.predicateId, objectId: f.objectId }, p);
            const newPages = await writer.finalize();
            // 替换 manifest 中该 primary 的页映射
            const remained = lookup.pages.filter((pg) => pg.primaryValue !== p);
            lookup.pages = [...remained, ...newPages];
        }
        repaired.push({ order, primaryValues: primariesArr });
    }
    // bump epoch
    manifest.epoch = (manifest.epoch ?? 0) + 1;
    await writePagedManifest(indexDir, manifest);
    return { repaired };
}
//# sourceMappingURL=repair.js.map
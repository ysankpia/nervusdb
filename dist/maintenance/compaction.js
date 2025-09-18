import { promises as fs } from 'node:fs';
import { join } from 'node:path';
import { PagedIndexReader, PagedIndexWriter, pageFileName, readPagedManifest, writePagedManifest, } from '../storage/pagedIndex';
function primarySelector(order) {
    if (order === 'SPO' || order === 'SOP')
        return (t) => t.subjectId;
    if (order === 'POS' || order === 'PSO')
        return (t) => t.predicateId;
    return (t) => t.objectId;
}
function encodeTripleKey(t) {
    return `${t.subjectId}:${t.predicateId}:${t.objectId}`;
}
export async function compactDatabase(dbPath, options = {}) {
    const indexDir = `${dbPath}.pages`;
    const manifest = await readPagedManifest(indexDir);
    if (!manifest) {
        throw new Error('未找到分页索引 manifest，无法执行 compaction');
    }
    const pageSize = options.pageSize ?? manifest.pageSize;
    const orders = options.orders ?? ['SPO', 'SOP', 'POS', 'PSO', 'OSP', 'OPS'];
    const minMergePages = options.minMergePages ?? 2;
    const tombstoneThreshold = options.tombstoneRatioThreshold ?? 0;
    const tombstoneSet = new Set((manifest.tombstones ?? []).map(([s, p, o]) => `${s}:${p}:${o}`));
    const newLookups = [];
    let pagesBefore = 0;
    let pagesAfter = 0;
    let primariesMerged = 0;
    let removedByTombstones = 0;
    const ordersRewritten = [];
    // 实验性：读取 LSM 段，供各顺序并入
    let lsmTriples = [];
    let lsmSegmentFiles = [];
    if (options.includeLsmSegments) {
        try {
            const manPath = join(indexDir, 'lsm-manifest.json');
            const buf = await fs.readFile(manPath);
            const lsm = JSON.parse(buf.toString('utf8'));
            for (const seg of lsm.segments ?? []) {
                const file = join(indexDir, seg.file);
                try {
                    const data = await fs.readFile(file);
                    const cnt = Math.floor(data.length / 12);
                    for (let i = 0; i < cnt; i += 1) {
                        const off = i * 12;
                        lsmTriples.push({
                            subjectId: data.readUInt32LE(off),
                            predicateId: data.readUInt32LE(off + 4),
                            objectId: data.readUInt32LE(off + 8),
                        });
                    }
                    lsmSegmentFiles.push(file);
                }
                catch { }
            }
        }
        catch { }
    }
    for (const order of orders) {
        const lookup = manifest.lookups.find((l) => l.order === order);
        if (!lookup) {
            newLookups.push({ order, pages: [] });
            continue;
        }
        pagesBefore += lookup.pages.length;
        const reader = new PagedIndexReader({ directory: indexDir, compression: manifest.compression }, lookup);
        // 聚合每个主键的所有三元组，并去重/去除 tombstones
        const byPrimary = new Map();
        const seen = new Set();
        const primaries = [...new Set(lookup.pages.map((p) => p.primaryValue))];
        for (const primary of primaries) {
            const triples = await reader.read(primary);
            for (const t of triples) {
                const key = encodeTripleKey(t);
                const isTomb = tombstoneSet.has(key);
                if (isTomb)
                    removedByTombstones += 1;
                if (isTomb || seen.has(`${order}|${key}`))
                    continue;
                seen.add(`${order}|${key}`);
                const list = byPrimary.get(primary) ?? [];
                if (!byPrimary.has(primary))
                    byPrimary.set(primary, list);
                list.push(t);
            }
        }
        // 并入 LSM 段（若设置 includeLsmSegments）
        if (options.includeLsmSegments && lsmTriples.length > 0) {
            const getPrimary = primarySelector(order);
            for (const t of lsmTriples) {
                const key = encodeTripleKey(t);
                const isTomb = tombstoneSet.has(key);
                if (isTomb) {
                    removedByTombstones += 1;
                    continue;
                }
                if (seen.has(`${order}|${key}`))
                    continue;
                seen.add(`${order}|${key}`);
                const primary = getPrimary(t);
                const list = byPrimary.get(primary) ?? [];
                if (!byPrimary.has(primary))
                    byPrimary.set(primary, list);
                list.push(t);
            }
        }
        // 评估是否重写该顺序：满足 minMergePages 或 tombstone 比例
        const shouldRewrite = (() => {
            if (lookup.pages.length === 0)
                return false;
            // 任意 primary 的页数达到阈值
            const countMap = new Map();
            lookup.pages.forEach((pg) => {
                countMap.set(pg.primaryValue, (countMap.get(pg.primaryValue) ?? 0) + 1);
            });
            const hasMergeCandidate = [...countMap.values()].some((c) => c >= minMergePages);
            if (hasMergeCandidate)
                return true;
            if (tombstoneThreshold > 0) {
                const totalTriples = seen.size; // 近似
                const ratio = totalTriples === 0 ? 0 : removedByTombstones / (removedByTombstones + totalTriples);
                if (ratio >= tombstoneThreshold)
                    return true;
            }
            return false;
        })();
        if (options.dryRun && !shouldRewrite) {
            newLookups.push(lookup);
            continue;
        }
        if (options.dryRun && shouldRewrite) {
            // 仅统计变更，不落盘
            const estimatePages = byPrimary.size; // 近似估计：每个主键至少 1 页
            pagesAfter += estimatePages;
            primariesMerged += [...new Set(lookup.pages.map((p) => p.primaryValue))].length;
            ordersRewritten.push(order);
            newLookups.push(lookup);
            continue;
        }
        const mode = options.mode ?? 'rewrite';
        if (mode === 'rewrite') {
            // 写入新的页文件（tmp → rename）
            const tmpFile = join(indexDir, `${pageFileName(order)}.tmp`);
            try {
                await fs.unlink(tmpFile);
            }
            catch { }
            const writer = new PagedIndexWriter(tmpFile, {
                directory: indexDir,
                pageSize,
                compression: options.coldCompression ?? options.compression ?? manifest.compression,
            });
            const getPrimary = primarySelector(order);
            for (const list of byPrimary.values()) {
                list.sort((a, b) => a.subjectId - b.subjectId || a.predicateId - b.predicateId || a.objectId - b.objectId);
                for (const t of list)
                    writer.push(t, getPrimary(t));
            }
            const pages = await writer.finalize();
            const dest = join(indexDir, pageFileName(order));
            try {
                await fs.unlink(dest);
            }
            catch { }
            await fs.rename(tmpFile, dest);
            newLookups.push({ order, pages });
            pagesAfter += pages.length;
            primariesMerged += byPrimary.size;
            ordersRewritten.push(order);
        }
        else {
            // incremental：仅为目标 primary 追加新页，并替换 manifest 中该 primary 的页映射
            const dest = join(indexDir, pageFileName(order));
            const writer = new PagedIndexWriter(dest, {
                directory: indexDir,
                pageSize,
                compression: options.hotCompression ?? options.compression ?? manifest.compression,
            });
            const getPrimary = primarySelector(order);
            // 选出需要重写的 primary（达到 minMergePages 或墓碑比例高）
            const pageCountByPrimary = new Map();
            for (const p of lookup.pages)
                pageCountByPrimary.set(p.primaryValue, (pageCountByPrimary.get(p.primaryValue) ?? 0) + 1);
            const rewritePrimaries = new Set();
            for (const [pval, count] of pageCountByPrimary.entries()) {
                if (count >= minMergePages)
                    rewritePrimaries.add(pval);
            }
            const limitPrimaries = options.onlyPrimaries?.[order]
                ? new Set(options.onlyPrimaries[order])
                : null;
            if (limitPrimaries) {
                for (const p of [...rewritePrimaries]) {
                    if (!limitPrimaries.has(p))
                        rewritePrimaries.delete(p);
                }
                if (rewritePrimaries.size === 0) {
                    newLookups.push(lookup);
                    continue;
                }
            }
            // 逐 primary 写入新页
            const newPagesByPrimary = new Map();
            for (const [primary, list] of byPrimary.entries()) {
                if (!rewritePrimaries.has(primary))
                    continue;
                // 稳定排序
                list.sort((a, b) => a.subjectId - b.subjectId || a.predicateId - b.predicateId || a.objectId - b.objectId);
                for (const t of list)
                    writer.push(t, getPrimary(t));
                const pages = await writer.finalize();
                newPagesByPrimary.set(primary, pages);
            }
            // 重建 pages 映射：替换被重写的 primary，保留其余原页
            const mergedPages = [];
            const removedPages = [];
            const rewrittenSet = new Set(newPagesByPrimary.keys());
            lookup.pages.
                forEach((pg) => {
                if (rewrittenSet.has(pg.primaryValue)) {
                    removedPages.push(pg);
                }
                else {
                    mergedPages.push(pg);
                }
            });
            for (const [, newp] of newPagesByPrimary.entries())
                mergedPages.push(...newp);
            newLookups.push({ order, pages: mergedPages });
            // 统计：按主键数计
            pagesAfter += mergedPages.length;
            primariesMerged += rewrittenSet.size;
            ordersRewritten.push(order);
            // 记录孤页待 GC
            if (removedPages.length > 0) {
                const orphans = manifest.orphans ?? [];
                orphans.push({ order, pages: removedPages });
                manifest.orphans = orphans;
            }
        }
    }
    const newManifest = {
        version: manifest.version,
        pageSize,
        createdAt: Date.now(),
        compression: options.compression ?? manifest.compression,
        lookups: newLookups,
        tombstones: manifest.tombstones,
        epoch: (manifest.epoch ?? 0) + 1,
        orphans: manifest.orphans,
    };
    await writePagedManifest(indexDir, newManifest);
    // 清理已并入的 LSM 段与清单
    if (options.includeLsmSegments && lsmSegmentFiles.length > 0) {
        try {
            for (const f of lsmSegmentFiles) {
                try {
                    await fs.unlink(f);
                }
                catch { }
            }
            const manPath = join(indexDir, 'lsm-manifest.json');
            await fs.writeFile(manPath, JSON.stringify({ version: 1, segments: [] }, null, 2), 'utf8');
        }
        catch { }
    }
    return { ordersRewritten, pagesBefore, pagesAfter, primariesMerged, removedByTombstones };
}
//# sourceMappingURL=compaction.js.map
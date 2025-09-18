import { readPagedManifest } from '../storage/pagedIndex.js';
import { promises as fsp } from 'node:fs';
import { compactDatabase, } from './compaction.js';
import { readHotness } from '../storage/hotness.js';
import { garbageCollectPages } from './gc.js';
export async function autoCompact(dbPath, options = {}) {
    const manifest = await readPagedManifest(`${dbPath}.pages`);
    if (!manifest) {
        return { selectedOrders: [] };
    }
    if (options.respectReaders) {
        try {
            const { getActiveReaders } = await import('../storage/readerRegistry.js');
            const readers = await getActiveReaders(`${dbPath}.pages`);
            if (readers.length > 0) {
                return {
                    selectedOrders: [],
                    skipped: true,
                    reason: 'active_readers',
                    readers: readers.length,
                };
            }
        }
        catch {
            // ignore registry failures
        }
    }
    const orders = options.orders ?? ['SPO', 'SOP', 'POS', 'PSO', 'OSP', 'OPS'];
    const minMergePages = options.minMergePages ?? 2;
    const tombstones = new Set((manifest.tombstones ?? []).map((t) => `${t[0]}:${t[1]}:${t[2]}`));
    const selected = new Set();
    const onlyPrimaries = {};
    const hot = await readHotness(`${dbPath}.pages`).catch(() => null);
    for (const order of orders) {
        const lookup = manifest.lookups.find((l) => l.order === order);
        if (!lookup || lookup.pages.length === 0)
            continue;
        // 统计 primary → 页数
        const cnt = new Map();
        for (const p of lookup.pages)
            cnt.set(p.primaryValue, (cnt.get(p.primaryValue) ?? 0) + 1);
        const hasMergeCandidate = [...cnt.values()].some((c) => c >= minMergePages);
        if (hasMergeCandidate)
            selected.add(order);
        // 简化墓碑触发：仅依据有无 tombstones（阈值在 compaction 内二次判定）
        if (tombstones.size > 0)
            selected.add(order);
        // 热度驱动（增量模式）：选取热度超过阈值且拥有多页的 primary
        if (options.mode !== 'rewrite' && hot && options.hotThreshold && options.hotThreshold > 0) {
            const counts = hot.counts[order] ?? {};
            const candidates = [];
            const w = {
                hot: options.scoreWeights?.hot ?? 1,
                pages: options.scoreWeights?.pages ?? 1,
                tomb: options.scoreWeights?.tomb ?? 0.5,
            };
            const minScore = options.minScore ?? 1;
            for (const [pval, count] of cnt.entries()) {
                if (count <= 1)
                    continue; // 非多页
                const pvStr = String(pval);
                const hotCount = counts[pvStr] ?? 0;
                // 评分：热度*wh + (页数-1)*wp + (tombstones>0?1:0)*wt
                const tombTerm = tombstones.size > 0 ? 1 : 0;
                const score = hotCount * w.hot + (count - 1) * w.pages + tombTerm * w.tomb;
                if (hotCount >= options.hotThreshold && score >= minScore)
                    candidates.push({ p: pval, c: hotCount, pages: count, score });
            }
            // 优先按分数、再按热度排序
            const sorted = candidates.sort((a, b) => b.score - a.score || b.c - a.c);
            const topK = options.maxPrimariesPerOrder
                ? sorted.slice(0, options.maxPrimariesPerOrder)
                : sorted;
            if (topK.length > 0) {
                onlyPrimaries[order] = topK.map((x) => x.p);
                selected.add(order);
            }
        }
    }
    let selectedOrders = [...selected];
    // 评估是否并入 LSM 段
    let includeLsmSegments = options.includeLsmSegments ?? false;
    if (!includeLsmSegments && options.includeLsmSegmentsAuto) {
        try {
            const buf = await fsp.readFile(`${dbPath}.pages/lsm-manifest.json`);
            const lsm = JSON.parse(buf.toString('utf8'));
            const segs = lsm.segments?.length ?? 0;
            const triples = (lsm.segments ?? []).reduce((a, s) => a + (s.count ?? 0), 0);
            const segTh = options.lsmSegmentsThreshold ?? 1;
            const triTh = options.lsmTriplesThreshold ?? options.pageSize ?? manifest.pageSize ?? 1024;
            if (segs >= segTh || triples >= triTh)
                includeLsmSegments = true;
        }
        catch {
            /* ignore */
        }
    }
    if (selectedOrders.length === 0 && includeLsmSegments && !(options.dryRun ?? false)) {
        // 当仅因为 LSM 段需要并入时，至少对指定 orders 执行一次合并
        selectedOrders = orders;
    }
    if (selectedOrders.length === 0)
        return { selectedOrders };
    const compactOpts = {
        orders: selectedOrders,
        pageSize: options.pageSize ?? manifest.pageSize,
        minMergePages,
        tombstoneRatioThreshold: options.tombstoneRatioThreshold,
        compression: options.compression ?? manifest.compression,
        hotCompression: options.hotCompression,
        coldCompression: options.coldCompression,
        dryRun: options.dryRun ?? false,
        mode: options.mode ?? 'incremental',
        onlyPrimaries,
        includeLsmSegments,
    };
    const stats = await compactDatabase(dbPath, compactOpts);
    if (options.autoGC && !options.dryRun) {
        await garbageCollectPages(dbPath);
    }
    return { selectedOrders, compact: stats };
}
//# sourceMappingURL=autoCompact.js.map
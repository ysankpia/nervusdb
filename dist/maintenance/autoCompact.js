import { readPagedManifest } from '../storage/pagedIndex';
import { compactDatabase } from './compaction';
import { readHotness } from '../storage/hotness';
import { garbageCollectPages } from './gc';
export async function autoCompact(dbPath, options = {}) {
    const manifest = await readPagedManifest(`${dbPath}.pages`);
    if (!manifest) {
        return { selectedOrders: [] };
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
            for (const [pval, count] of cnt.entries()) {
                if (count <= 1)
                    continue; // 非多页
                const pvStr = String(pval);
                const hotCount = counts[pvStr] ?? 0;
                if (hotCount >= options.hotThreshold) {
                    candidates.push({ p: pval, c: hotCount });
                    selected.add(order);
                }
            }
            const sorted = candidates.sort((a, b) => b.c - a.c);
            const topK = options.maxPrimariesPerOrder ? sorted.slice(0, options.maxPrimariesPerOrder) : sorted;
            if (topK.length > 0) {
                onlyPrimaries[order] = topK.map((x) => x.p);
            }
        }
    }
    const selectedOrders = [...selected];
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
    };
    const stats = await compactDatabase(dbPath, compactOpts);
    if (options.autoGC && !options.dryRun) {
        await garbageCollectPages(dbPath);
    }
    return { selectedOrders, compact: stats };
}
//# sourceMappingURL=autoCompact.js.map
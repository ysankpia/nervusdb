import { readPagedManifest } from '../storage/pagedIndex.js';
import { promises as fsp } from 'node:fs';
import {
  compactDatabase,
  type CompactOptions,
  type CompactStats,
  type IndexOrder,
} from './compaction.js';
import { readHotness } from '../storage/hotness.js';
import { garbageCollectPages } from './gc.js';

export interface AutoCompactOptions {
  orders?: IndexOrder[];
  minMergePages?: number;
  tombstoneRatioThreshold?: number;
  pageSize?: number;
  compression?: { codec: 'none' | 'brotli'; level?: number };
  hotCompression?: { codec: 'none' | 'brotli'; level?: number };
  coldCompression?: { codec: 'none' | 'brotli'; level?: number };
  dryRun?: boolean;
  mode?: 'rewrite' | 'incremental';
  hotThreshold?: number; // 热主键阈值，仅增量模式生效
  maxPrimariesPerOrder?: number; // 每个顺序最多重写的 primary 数
  autoGC?: boolean; // 执行后自动 GC
  scoreWeights?: { hot?: number; pages?: number; tomb?: number }; // 多因素评分权重（默认 hot=1,pages=1,tomb=0.5）
  minScore?: number; // 满足分数阈值才纳入候选（默认 1）
  respectReaders?: boolean; // 当存在读者时跳过（跨进程可见）
  includeLsmSegments?: boolean; // 将 LSM 段并入 compaction 并清理
  includeLsmSegmentsAuto?: boolean; // 自动评估是否并入 LSM 段
  lsmSegmentsThreshold?: number; // 触发并入的段数量阈值（默认 1）
  lsmTriplesThreshold?: number; // 触发并入的段三元组数量阈值（默认 pageSize 或 1024）
}

export interface AutoCompactDecision {
  selectedOrders: IndexOrder[];
  compact?: CompactStats;
  skipped?: boolean;
  reason?: string;
  readers?: number;
}

export async function autoCompact(
  dbPath: string,
  options: AutoCompactOptions = {},
): Promise<AutoCompactDecision> {
  // 默认干跑（不改磁盘），与 CLI 文档一致；需要真实执行时显式传 dryRun: false
  const dryRun = options.dryRun ?? true;
  console.log(`🔧 Starting auto-compact analysis for: ${dbPath}`);
  console.log(`   Mode: ${options.mode ?? 'incremental'}`);
  console.log(`   Min merge pages: ${options.minMergePages ?? 2}`);
  console.log(`   Dry run: ${dryRun}`);

  const manifest = await readPagedManifest(`${dbPath}.pages`);
  if (!manifest) {
    console.log(`❌ No paged manifest found`);
    return { selectedOrders: [] };
  }

  console.log(`📊 Manifest summary:`);
  console.log(`   Total lookups: ${manifest.lookups.length}`);
  console.log(`   Page size: ${manifest.pageSize}`);
  console.log(`   Tombstones: ${manifest.tombstones?.length ?? 0}`);

  if (options.respectReaders) {
    try {
      const { getActiveReaders } = await import('../storage/readerRegistry.js');
      const readers = await getActiveReaders(`${dbPath}.pages`);
      if (readers.length > 0) {
        console.log(`🔒 Skipping compaction due to ${readers.length} active readers`);
        return {
          selectedOrders: [],
          skipped: true,
          reason: 'active_readers',
          readers: readers.length,
        };
      } else {
        console.log(`✅ No active readers found - proceeding with compaction`);
      }
    } catch {
      console.log(`⚠️  Failed to check active readers - proceeding anyway`);
    }
  }

  const orders: IndexOrder[] = options.orders ?? ['SPO', 'SOP', 'POS', 'PSO', 'OSP', 'OPS'];
  const minMergePages = options.minMergePages ?? 2;
  const tombstones = new Set((manifest.tombstones ?? []).map((t) => `${t[0]}:${t[1]}:${t[2]}`));

  console.log(`\n🎯 Analyzing orders: [${orders.join(', ')}]`);

  const selected = new Set<IndexOrder>();
  const onlyPrimaries: Partial<Record<IndexOrder, number[]>> = {};
  const hot = await readHotness(`${dbPath}.pages`).catch(() => null);

  if (hot) {
    console.log(`🔥 Hotness data loaded (updated: ${new Date(hot.updatedAt).toISOString()})`);
  } else {
    console.log(`📈 No hotness data available`);
  }
  const getCountsForOrder = (order: IndexOrder) => {
    if (!hot) return {} as Record<string, number>;
    const a = hot.counts[order] ?? {};
    const pair: Partial<Record<IndexOrder, IndexOrder>> = {
      SPO: 'SOP',
      SOP: 'SPO',
      POS: 'PSO',
      PSO: 'POS',
      OSP: 'OPS',
      OPS: 'OSP',
    };
    const bKey = pair[order];
    if (!bKey) return a;
    const b = hot.counts[bKey] ?? {};
    const merged: Record<string, number> = { ...a };
    for (const [k, v] of Object.entries(b)) merged[k] = (merged[k] ?? 0) + v;
    return merged;
  };

  for (const order of orders) {
    console.log(`\n📋 Analyzing order: ${order}`);
    const lookup = manifest.lookups.find((l) => l.order === order);
    if (!lookup || lookup.pages.length === 0) {
      console.log(`   ❌ No lookup or empty pages`);
      continue;
    }

    console.log(`   📄 Total pages: ${lookup.pages.length}`);

    // 统计 primary → 页数
    const cnt = new Map<number, number>();
    for (const p of lookup.pages) cnt.set(p.primaryValue, (cnt.get(p.primaryValue) ?? 0) + 1);

    const multiPagePrimaries = [...cnt.entries()].filter(
      ([, pageCount]) => pageCount >= minMergePages,
    );
    const hasMergeCandidate = multiPagePrimaries.length > 0;

    console.log(`   🔗 Unique primaries: ${cnt.size}`);
    console.log(
      `   📊 Multi-page primaries (>=${minMergePages} pages): ${multiPagePrimaries.length}`,
    );

    if (hasMergeCandidate) {
      selected.add(order);
      console.log(`   ✅ Selected for compaction (merge candidates found)`);
      multiPagePrimaries.slice(0, 5).forEach(([primary, pageCount]) => {
        console.log(`      • Primary ${primary}: ${pageCount} pages`);
      });
      if (multiPagePrimaries.length > 5) {
        console.log(`      • ... and ${multiPagePrimaries.length - 5} more`);
      }
    }

    // 简化墓碑触发：仅依据有无 tombstones（阈值在 compaction 内二次判定）
    if (tombstones.size > 0) {
      if (!selected.has(order)) {
        selected.add(order);
        console.log(`   ✅ Selected for compaction (tombstone cleanup needed)`);
      } else {
        console.log(`   📰 Also has tombstones to clean`);
      }
    }

    // 热度驱动（增量模式）：选取热度超过阈值且拥有多页的 primary
    if (options.mode !== 'rewrite' && hot && options.hotThreshold && options.hotThreshold > 0) {
      console.log(`   🔥 Hot-based analysis (threshold: ${options.hotThreshold})`);
      const counts = getCountsForOrder(order);
      const candidates: Array<{ p: number; c: number; pages: number; score: number }> = [];
      const w = {
        hot: options.scoreWeights?.hot ?? 1,
        pages: options.scoreWeights?.pages ?? 1,
        tomb: options.scoreWeights?.tomb ?? 0.5,
      };
      const minScore = options.minScore ?? 1;

      console.log(`   📊 Score weights: hot=${w.hot}, pages=${w.pages}, tomb=${w.tomb}`);
      console.log(`   🎯 Min score threshold: ${minScore}`);

      for (const [pval, count] of cnt.entries()) {
        if (count <= 1) continue; // 非多页
        const pvStr = String(pval);
        const hotCount = counts[pvStr] ?? 0;
        // 评分：热度*wh + (页数-1)*wp + (tombstones>0?1:0)*wt
        const tombTerm = tombstones.size > 0 ? 1 : 0;
        const score = hotCount * w.hot + (count - 1) * w.pages + tombTerm * w.tomb;

        const scoreDetail = {
          primary: pval,
          hotness: hotCount,
          pageCount: count,
          fragmentation: count - 1,
          score: {
            hotness: hotCount * w.hot,
            pageCount: (count - 1) * w.pages,
            tombstone: tombTerm * w.tomb,
            total: score,
          },
        };

        if (hotCount >= options.hotThreshold && score >= minScore) {
          console.log(`   ✅ Primary ${pval} qualifies:`);
          console.log(`      • Hotness: ${hotCount} (score: +${scoreDetail.score.hotness})`);
          console.log(`      • Pages: ${count} (score: +${scoreDetail.score.pageCount})`);
          console.log(
            `      • Tombstone factor: ${tombTerm} (score: +${scoreDetail.score.tombstone})`,
          );
          console.log(`      • Total score: ${scoreDetail.score.total}`);
          console.log(`      • Action: INCLUDE`);
          console.log(
            `      • Reason: score >= ${minScore} AND hotness >= ${options.hotThreshold}`,
          );

          candidates.push({ p: pval, c: hotCount, pages: count, score });
        } else {
          const reasons = [];
          if (hotCount < options.hotThreshold)
            reasons.push(`hotness ${hotCount} < ${options.hotThreshold}`);
          if (score < minScore) reasons.push(`score ${score} < ${minScore}`);

          console.log(`   ❌ Primary ${pval} excluded:`);
          console.log(`      • Hotness: ${hotCount}`);
          console.log(`      • Pages: ${count}`);
          console.log(`      • Total score: ${scoreDetail.score.total}`);
          console.log(`      • Action: SKIP`);
          console.log(`      • Reason: ${reasons.join(' AND ')}`);
        }
      }

      // 优先按分数、再按热度排序
      const sorted = candidates.sort((a, b) => b.score - a.score || b.c - a.c);
      const topK = options.maxPrimariesPerOrder
        ? sorted.slice(0, options.maxPrimariesPerOrder)
        : sorted;

      if (topK.length > 0) {
        console.log(`   🎯 Top ${topK.length} hot primaries selected:`);
        topK.forEach((c, i) => {
          console.log(
            `      ${i + 1}. Primary ${c.p}: hotness=${c.c}, pages=${c.pages}, score=${c.score}`,
          );
        });

        (onlyPrimaries as any)[order] = topK.map((x) => x.p);
        if (!selected.has(order)) {
          selected.add(order);
          console.log(`   ✅ Selected for compaction (hot primaries found)`);
        }
      } else {
        console.log(`   ❌ No hot primaries qualify for incremental compaction`);
      }
    }
  }

  let selectedOrders = [...selected];

  console.log(`\n📈 LSM segment analysis:`);
  // 评估是否并入 LSM 段
  let includeLsmSegments = options.includeLsmSegments ?? false;
  if (!includeLsmSegments && options.includeLsmSegmentsAuto) {
    try {
      const buf = await fsp.readFile(`${dbPath}.pages/lsm-manifest.json`);
      const lsm = JSON.parse(buf.toString('utf8')) as { segments: Array<{ count?: number }> };
      const segs = lsm.segments?.length ?? 0;
      const triples = (lsm.segments ?? []).reduce((a, s) => a + (s.count ?? 0), 0);
      const segTh = options.lsmSegmentsThreshold ?? 1;
      const triTh = options.lsmTriplesThreshold ?? options.pageSize ?? manifest.pageSize ?? 1024;

      console.log(`   📊 LSM segments: ${segs}`);
      console.log(`   📊 LSM triples: ${triples}`);
      console.log(`   🎯 Thresholds: segments >= ${segTh}, triples >= ${triTh}`);

      if (segs >= segTh || triples >= triTh) {
        includeLsmSegments = true;
        console.log(`   ✅ Will include LSM segments in compaction`);
        const reasons = [];
        if (segs >= segTh) reasons.push(`segments ${segs} >= ${segTh}`);
        if (triples >= triTh) reasons.push(`triples ${triples} >= ${triTh}`);
        console.log(`   📋 Reason: ${reasons.join(' OR ')}`);
      } else {
        console.log(`   ❌ LSM segments below threshold - excluding`);
      }
    } catch {
      console.log(`   ⚠️  No LSM manifest found - skipping LSM analysis`);
    }
  } else if (includeLsmSegments) {
    console.log(`   ✅ LSM segments explicitly included`);
  } else {
    console.log(`   ❌ LSM segments not requested`);
  }

  if (selectedOrders.length === 0 && includeLsmSegments && !dryRun) {
    // 当仅因为 LSM 段需要并入时，至少对指定 orders 执行一次合并
    console.log(`\n🔄 No orders selected but LSM merge needed - selecting all orders`);
    selectedOrders = orders;
  }

  console.log(`\n🎯 Final compaction decision:`);
  console.log(`   Selected orders: [${selectedOrders.join(', ')}]`);
  console.log(`   Include LSM segments: ${includeLsmSegments}`);
  console.log(`   Dry run: ${dryRun}`);

  if (selectedOrders.length === 0) {
    console.log(`\n✅ No compaction needed - all indexes are optimal`);
    return { selectedOrders };
  }

  const compactOpts: CompactOptions = {
    orders: selectedOrders,
    pageSize: options.pageSize ?? manifest.pageSize,
    minMergePages,
    tombstoneRatioThreshold: options.tombstoneRatioThreshold,
    compression: options.compression ?? manifest.compression,
    hotCompression: options.hotCompression,
    coldCompression: options.coldCompression,
    dryRun,
    mode: options.mode ?? 'incremental',
    onlyPrimaries,
    includeLsmSegments,
  };

  console.log(`\n🚀 Starting compaction...`);
  const stats = await compactDatabase(dbPath, compactOpts);

  console.log(`\n📊 Compaction completed:`);
  console.log(`   Pages before: ${stats.pagesBefore ?? 0}`);
  console.log(`   Pages after: ${stats.pagesAfter ?? 0}`);
  console.log(`   Primaries merged: ${stats.primariesMerged ?? 0}`);
  console.log(`   Removed by tombstones: ${stats.removedByTombstones ?? 0}`);
  if (stats.ordersRewritten) {
    console.log(`   Orders processed: [${stats.ordersRewritten.join(', ')}]`);
  }

  if (options.autoGC && !dryRun) {
    console.log(`\n🗑️  Running auto garbage collection...`);
    await garbageCollectPages(dbPath, { dryRun: false });
    console.log(`✅ Garbage collection completed`);
  } else if (options.autoGC && dryRun) {
    console.log(`\nℹ️  Auto GC skipped (dry-run mode)`);
  }

  console.log(`\n✅ Auto-compact finished successfully`);
  return { selectedOrders, compact: stats };
}

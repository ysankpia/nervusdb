import { promises as fs } from 'node:fs';
import { join } from 'node:path';

import { readStorageFile } from '../storage/fileHeader';
import { readPagedManifest } from '../storage/pagedIndex';

async function stats(
  dbPath: string,
  opts: { listTxIds?: number; txIdsWindowMin?: number },
): Promise<void> {
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
  const orders: Record<string, { pages: number; primaries: number; multiPagePrimaries: number }> =
    {};
  for (const l of lookups) {
    pageFiles += 1;
    pages += l.pages.length;
    const cnt = new Map<number, number>();
    for (const p of l.pages) cnt.set(p.primaryValue, (cnt.get(p.primaryValue) ?? 0) + 1);
    const multi = [...cnt.values()].filter((c) => c > 1).length;
    orders[l.order] = { pages: l.pages.length, primaries: cnt.size, multiPagePrimaries: multi };
  }

  let walSize = 0;
  try {
    const st = await fs.stat(`${dbPath}.wal`);
    walSize = st.size;
  } catch {}

  // txId 注册表（若存在）
  let txIds = 0;
  let txIdItems: Array<{ id: string; ts: number; sessionId?: string }> | undefined;
  let txIdsWindow = 0;
  let txIdsBySession: Record<string, number> | undefined;
  let lsmSegments = 0;
  let lsmTriples = 0;
  try {
    const { readTxIdRegistry } = await import('../storage/txidRegistry');
    const reg = await readTxIdRegistry(`${dbPath}.pages`);
    txIds = reg.txIds.length;
    if (opts.listTxIds && opts.listTxIds > 0) {
      txIdItems = [...reg.txIds].sort((a, b) => b.ts - a.ts).slice(0, opts.listTxIds);
    }
    if (opts.txIdsWindowMin && opts.txIdsWindowMin > 0) {
      const since = Date.now() - opts.txIdsWindowMin * 60_000;
      const items = reg.txIds.filter((x) => x.ts >= since);
      txIdsWindow = items.length;
      const g: Record<string, number> = {};
      for (const it of items) {
        const key = it.sessionId ?? 'unknown';
        g[key] = (g[key] ?? 0) + 1;
      }
      txIdsBySession = g;
    }
  } catch {}

  // LSM-Lite 段清单（实验性）
  try {
    const man = await fs.readFile(`${dbPath}.pages/lsm-manifest.json`);
    const m = JSON.parse(man.toString('utf8')) as { segments: Array<{ count: number }> };
    lsmSegments = m.segments?.length ?? 0;
    lsmTriples = (m.segments ?? []).reduce((a, s) => a + (s.count ?? 0), 0);
  } catch {}

  const out: any = {
    dictionaryEntries: dictCount,
    triples: tripleCount,
    epoch,
    pageFiles,
    pages,
    tombstones,
    walBytes: walSize,
    txIds,
    lsmSegments,
    lsmTriples,
    orders,
  };
  if (txIdItems) out.txIdItems = txIdItems;
  if (opts.txIdsWindowMin) {
    out.txIdsWindowMin = opts.txIdsWindowMin;
    out.txIdsWindow = txIdsWindow;
    if (txIdsBySession) out.txIdsBySession = txIdsBySession;
  }
  console.log(JSON.stringify(out, null, 2));
}

async function main() {
  const args = process.argv.slice(2);
  const dbPath = args[0];
  if (!dbPath) {
    console.log('用法: pnpm db:stats <db>');
    process.exit(1);
  }
  const listArg = args.find((a) => a.startsWith('--txids'));
  let listTxIds: number | undefined;
  if (listArg) {
    const parts = listArg.split('=');
    listTxIds = parts.length > 1 ? Number(parts[1]) : 50;
  }
  const winArg = args.find((a) => a.startsWith('--txids-window='));
  const txIdsWindowMin = winArg ? Number(winArg.split('=')[1]) : undefined;
  await stats(dbPath, { listTxIds, txIdsWindowMin });
}

// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();

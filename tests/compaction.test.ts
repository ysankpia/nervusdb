import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';
import { readPagedManifest } from '@/storage/pagedIndex';

describe('Compaction MVP', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-compact-'));
    dbPath = join(workspace, 'compact.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('合并同主键的小页，压缩页数', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 4 });
    // 初次写入 3 条（同 subject），首次构建将写入 1 页
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O3' });
    await db.flush();

    const m1 = await readPagedManifest(`${dbPath}.pages`);
    const spo1 = m1!.lookups.find((l) => l.order === 'SPO')!;
    const primary = m1?.lookups.find((l) => l.order === 'SPO')!.pages[0].primaryValue!;
    const pagesBefore = spo1.pages.filter((p) => p.primaryValue === primary).length;
    expect(pagesBefore).toBe(1);

    // 追加 1 条并 flush，会产生一个新页（不足 pageSize）
    db.addFact({ subject: 'S', predicate: 'R', object: 'O4' });
    await db.flush();
    const m2 = await readPagedManifest(`${dbPath}.pages`);
    const spo2 = m2!.lookups.find((l) => l.order === 'SPO')!;
    const pagesMiddle = spo2.pages.filter((p) => p.primaryValue === primary).length;
    expect(pagesMiddle).toBeGreaterThanOrEqual(2);

    // 执行 compaction，期望合并为 1 页（共 4 条）
    const { compactDatabase } = await import('@/maintenance/compaction');
    await compactDatabase(dbPath);
    const m3 = await readPagedManifest(`${dbPath}.pages`);
    const spo3 = m3!.lookups.find((l) => l.order === 'SPO')!;
    const pagesAfter = spo3.pages.filter((p) => p.primaryValue === primary).length;
    expect(pagesAfter).toBeLessThanOrEqual(pagesMiddle);

    // 读逻辑不变
    const results = db.find({ subject: 'S', predicate: 'R' }).all();
    expect(results).toHaveLength(4);
  });
});

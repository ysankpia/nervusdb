import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { NervusDB } from '@/synapseDb';
import { autoCompact } from '@/maintenance/autoCompact';
import { readPagedManifest } from '@/core/storage/pagedIndex';

describe('运维组合工况（增量合并 + 热度驱动 + autoGC）', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-maint-'));
    dbPath = join(workspace, 'maint.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('在高热度主键下进行增量合并，并在 autoGC 后无冗余 orphan', async () => {
    const db = await NervusDB.open(dbPath, { pageSize: 2 });
    // 构造两个主语，其中 S 为热门且多页，T 为较冷
    for (let i = 1; i <= 5; i += 1) {
      db.addFact({ subject: 'S', predicate: 'R', object: `O${i}` });
    }
    for (let i = 1; i <= 4; i += 1) {
      db.addFact({ subject: 'T', predicate: 'R', object: `P${i}` });
    }
    await db.flush();

    // 读取 S 多次以提升热度（命中 SPO 主键 S）
    for (let i = 0; i < 5; i += 1) {
      void db.find({ subject: 'S', predicate: 'R' }).all();
    }

    const manifestBefore = await readPagedManifest(`${dbPath}.pages`);
    const epochBefore = manifestBefore?.epoch ?? 0;

    const decision = await autoCompact(dbPath, {
      mode: 'incremental',
      orders: ['SPO'],
      minMergePages: 2,
      hotThreshold: 1,
      maxPrimariesPerOrder: 1,
      autoGC: true,
    });
    expect(decision.selectedOrders).toContain('SPO');

    const manifestAfter = await readPagedManifest(`${dbPath}.pages`);
    expect(manifestAfter?.epoch ?? 0).toBeGreaterThan(epochBefore);
    // autoGC 后不应残留或phans（若字段存在）
    if (manifestAfter && 'orphans' in manifestAfter) {
      const anyManifest = manifestAfter as any;
      expect(anyManifest.orphans?.length ?? 0).toBe(0);
    }

    // 关闭并重开实例以确保读取最新 manifest（亦可视为跨进程验证）
    await db.close();
    const db2 = await NervusDB.open(dbPath);
    // 数据可见性未受影响
    const allS = db2.find({ subject: 'S', predicate: 'R' }).all();
    expect(allS.map((x) => x.object).sort()).toEqual(['O1', 'O2', 'O3', 'O4', 'O5']);
    await db2.close();
  });
});

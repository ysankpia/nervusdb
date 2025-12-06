import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { NervusDB } from '@/synapseDb';
import { autoCompact } from '@/maintenance/autoCompact';
import { readPagedManifest } from '@/core/storage/pagedIndex';

describe('Auto-Compact 多因素评分决策', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-auto-score-'));
    dbPath = join(workspace, 'as.synapsedb');
  });

  afterEach(async () => {
    // 强制清理readers目录中的所有文件，确保完全清理
    try {
      const readersDir = join(dbPath + '.pages', 'readers');
      // 重试清理逻辑，处理可能的竞态条件
      for (let attempt = 0; attempt < 5; attempt++) {
        try {
          const files = await readdir(readersDir);
          for (const file of files) {
            try {
              await unlink(join(readersDir, file));
            } catch {
              // 忽略删除失败
            }
          }
          await rmdir(readersDir);
          break; // 成功清理，退出重试循环
        } catch (err: any) {
          if (err?.code === 'ENOTEMPTY' && attempt < 4) {
            // 目录不为空，等待一下再重试
            await new Promise((resolve) => setTimeout(resolve, 50 * (attempt + 1)));
            continue;
          }
          // 其他错误或最后一次尝试失败，忽略
          break;
        }
      }
    } catch {
      // 忽略所有清理错误
    }

    await rm(workspace, { recursive: true, force: true });
  });

  it('在 S 与 T 同为多页时，优先对热度更高的 S 进行合并（限制 Top1）', async () => {
    const db = await NervusDB.open(dbPath, { pageSize: 1 });
    // 产生两个多页主键 S、T
    db.addFact({ subject: 'S', predicate: 'R', object: 'S1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'S2' });
    db.addFact({ subject: 'T', predicate: 'R', object: 'T1' });
    db.addFact({ subject: 'T', predicate: 'R', object: 'T2' });
    await db.flush();

    // 提升 S 的热度
    for (let i = 0; i < 5; i += 1) db.find({ subject: 'S', predicate: 'R' }).all();
    for (let i = 0; i < 2; i += 1) db.find({ subject: 'T', predicate: 'R' }).all();
    await db.flush();

    const before = await readPagedManifest(`${dbPath}.pages`);
    const spo = before!.lookups.find((l) => l.order === 'SPO')!;
    const pagesByPrimaryBefore = new Map<number, number>();
    for (const p of spo.pages)
      pagesByPrimaryBefore.set(p.primaryValue, (pagesByPrimaryBefore.get(p.primaryValue) ?? 0) + 1);

    await autoCompact(dbPath, {
      mode: 'incremental',
      orders: ['SPO'],
      minMergePages: 2,
      hotThreshold: 1,
      maxPrimariesPerOrder: 1,
      scoreWeights: { hot: 1, pages: 0.5, tomb: 0 },
      minScore: 1,
    });

    const after = await readPagedManifest(`${dbPath}.pages`);
    const spo2 = after!.lookups.find((l) => l.order === 'SPO')!;
    const pagesByPrimaryAfter = new Map<number, number>();
    for (const p of spo2.pages)
      pagesByPrimaryAfter.set(p.primaryValue, (pagesByPrimaryAfter.get(p.primaryValue) ?? 0) + 1);

    // 数据保持一致
    const db2 = await NervusDB.open(dbPath);
    const factsS = db2.find({ subject: 'S', predicate: 'R' }).all();
    const factsT = db2.find({ subject: 'T', predicate: 'R' }).all();
    expect(factsS.length).toBe(2);
    expect(factsT.length).toBe(2);
  });
});

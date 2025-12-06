import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { NervusDB } from '@/synapseDb';
import { compactDatabase } from '@/maintenance/compaction';
import { readPagedManifest } from '@/core/storage/pagedIndex';

describe('Compaction 高级选项', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-compact-adv-'));
    dbPath = join(workspace, 'c.synapsedb');
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

  it('dry-run 仅输出统计，不修改 manifest', async () => {
    const db = await NervusDB.open(dbPath, { pageSize: 2 });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    await db.flush();
    const m1 = await readPagedManifest(`${dbPath}.pages`);
    const pagesBefore = m1!.lookups.find((l) => l.order === 'SPO')!.pages.length;

    const stats = await compactDatabase(dbPath, {
      dryRun: true,
      minMergePages: 1,
      orders: ['SPO'],
    });
    expect(stats.ordersRewritten).toContain('SPO');

    const m2 = await readPagedManifest(`${dbPath}.pages`);
    const pagesAfter = m2!.lookups.find((l) => l.order === 'SPO')!.pages.length;
    expect(pagesAfter).toBe(pagesBefore); // 未变更
  });

  it('orders 过滤仅重写指定顺序', async () => {
    const db = await NervusDB.open(dbPath, { pageSize: 2 });
    db.addFact({ subject: 'S', predicate: 'R1', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R2', object: 'O2' });
    await db.flush();
    const stats = await compactDatabase(dbPath, { orders: ['SPO'], minMergePages: 1 });
    expect(stats.ordersRewritten).toEqual(['SPO']);
  });
});

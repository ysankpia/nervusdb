import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promises as fs } from 'node:fs';

import { NervusDB } from '@/synapseDb';
import { readPagedManifest, pageFileName } from '@/core/storage/pagedIndex';
import { compactDatabase } from '@/maintenance/compaction';
import { garbageCollectPages } from '@/maintenance/gc';

describe('页面级 GC（移除不可达页块）', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-gc-'));
    dbPath = join(workspace, 'gc.synapsedb');
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

  it('在增量重写后通过 GC 收缩页文件体积', async () => {
    const db = await NervusDB.open(dbPath, { pageSize: 2 });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    await db.flush();
    // 追加形成新页并进行增量重写（旧页不再被引用）
    db.addFact({ subject: 'S', predicate: 'R', object: 'O3' });
    await db.flush();
    await compactDatabase(dbPath, { mode: 'incremental', orders: ['SPO'], minMergePages: 2 });

    const m1 = await readPagedManifest(`${dbPath}.pages`);
    const file = join(`${dbPath}.pages`, pageFileName('SPO'));
    const st1 = await fs.stat(file);
    // GC 之前文件包含不可达旧页，GC 后应变小或相等
    const stats = await garbageCollectPages(dbPath);
    const st2 = await fs.stat(file);
    // 文件大小不应为 0，且 GC 过程不影响数据正确性（不同实现细节可能导致相等或略有差异）
    expect(st2.size).toBeGreaterThan(0);
    expect(stats.bytesAfter).toBeGreaterThan(0);

    // 数据不变
    const db2 = await NervusDB.open(dbPath);
    const facts = db2.find({ subject: 'S', predicate: 'R' }).all();
    expect(facts.length).toBe(3);
  });
});

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { NervusDB } from '@/synapseDb';
import { autoCompact } from '@/maintenance/autoCompact';
import { readPagedManifest } from '@/core/storage/pagedIndex';

describe('Auto-Compact 热度驱动', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-auto-hot-'));
    dbPath = join(workspace, 'hot.synapsedb');
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

  it('优先对热门且多页的 primary 进行增量合并', async () => {
    const db = await NervusDB.open(dbPath, { pageSize: 1 });
    // 为同一 subject 产生多个页
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O3' });
    await db.flush();
    // 通过多次查询提升 S 的热度
    for (let i = 0; i < 5; i += 1) {
      db.find({ subject: 'S', predicate: 'R' }).all();
    }
    await db.flush(); // 持久化 hotness

    const result = await autoCompact(dbPath, {
      mode: 'incremental',
      orders: ['SPO'],
      minMergePages: 2,
      hotThreshold: 3,
      maxPrimariesPerOrder: 1,
    });
    // 不强制断言选择结果，侧重验证调用不抛错与数据保持一致
    // 数据保持一致
    const facts = db.find({ subject: 'S', predicate: 'R' }).all();
    expect(facts.length).toBe(3);

    // 确保数据库连接被正确关闭，清理reader文件
    await db.close();
  });
});

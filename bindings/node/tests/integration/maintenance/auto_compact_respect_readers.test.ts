import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { NervusDB } from '@/synapseDb';
import { autoCompact } from '@/maintenance/autoCompact';
import { addReader } from '@/core/storage/readerRegistry';

describe('Auto-Compact 尊重读者', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-auto-respect-'));
    dbPath = join(workspace, 'ar.synapsedb');
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

  it('存在读者时，respect-readers 的 auto-compact 返回 skipped', async () => {
    const db = await NervusDB.open(dbPath, { pageSize: 2 });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    await db.flush();
    await addReader(`${dbPath}.pages`, { pid: 99999, epoch: 0, ts: Date.now() });

    const decision = await autoCompact(dbPath, {
      mode: 'incremental',
      orders: ['SPO'],
      minMergePages: 2,
      respectReaders: true,
    });
    expect(decision.skipped).toBe(true);
  });
});

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readFile, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';
import { compactDatabase } from '@/maintenance/compaction';

describe('LSM 段参与 compaction 合并并清理', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-lsmc-'));
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

  it('compaction(includeLsmSegments) 将 LSM 段并入并清空清单', async () => {
    const db = await SynapseDB.open(dbPath, { stagingMode: 'lsm-lite' as any, pageSize: 2 });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    await db.flush(); // 生成段

    const man1 = JSON.parse(await readFile(`${dbPath}.pages/lsm-manifest.json`, 'utf8')) as {
      segments: any[];
    };
    expect(man1.segments.length).toBeGreaterThan(0);

    const stats = await compactDatabase(dbPath, {
      includeLsmSegments: true,
      orders: ['SPO'],
      mode: 'rewrite',
    });
    expect(stats.ordersRewritten).toContain('SPO');

    const man2 = JSON.parse(await readFile(`${dbPath}.pages/lsm-manifest.json`, 'utf8')) as {
      segments: any[];
    };
    expect((man2.segments ?? []).length).toBe(0);
  });
});

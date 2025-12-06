import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { NervusDB } from '@/synapseDb';

describe('WAL 恢复', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-wal-'));
    dbPath = join(workspace, 'wal.synapsedb');
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

  it('未 flush 的写入可通过 WAL 重放恢复', async () => {
    const db1 = await NervusDB.open(dbPath);
    db1.addFact({ subject: 'class:User', predicate: 'HAS_METHOD', object: 'method:login' });
    // 模拟崩溃：不调用 flush，直接新开一个实例

    const db2 = await NervusDB.open(dbPath);
    const facts = db2.find({ subject: 'class:User', predicate: 'HAS_METHOD' }).all();
    expect(facts).toHaveLength(1);
    expect(facts[0].object).toBe('method:login');
    await db2.flush();
  });
});

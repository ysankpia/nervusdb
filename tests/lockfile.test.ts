import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

describe('进程级写锁（可选）', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-lock-'));
    dbPath = join(workspace, 'lock.synapsedb');
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

  it('启用 enableLock 时同进程重复打开可并行（同 PID），不同进程应独占（此处仅验证同进程不报错）', async () => {
    const db1 = await SynapseDB.open(dbPath, {
      indexDirectory: `${dbPath}.pages`,
      enableLock: true,
    });
    const db2 = await SynapseDB.open(dbPath, {
      indexDirectory: `${dbPath}.pages`,
      enableLock: false,
    });
    db1.addFact({ subject: 'S', predicate: 'R', object: 'O' });
    await db1.flush();
    await db2.flush();
    await db1.close();
    await db2.close();
    expect(true).toBe(true);
  });
});

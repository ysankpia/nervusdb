import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { NervusDB } from '@/synapseDb';

describe('持久化 txId 去重（跨周期）', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-wal-txid-persist-'));
    dbPath = join(workspace, 'wal_txid_persist.synapsedb');
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

  it('同一 txId 在 flush 后的下一次重放中被忽略', async () => {
    // 周期 1：提交并 flush，持久化注册表记录 txId
    const db1 = await NervusDB.open(dbPath, {
      enablePersistentTxDedupe: true,
      maxRememberTxIds: 100,
    });
    db1.beginBatch({ txId: 'PTX' });
    db1.addFact({ subject: 'A', predicate: 'R', object: 'X' });
    db1.commitBatch();
    await db1.flush();
    await db1.close();

    // 周期 2：再次使用相同 txId，提交但不 flush，模拟崩溃
    const db2 = await NervusDB.open(dbPath, { enablePersistentTxDedupe: true });
    db2.beginBatch({ txId: 'PTX' });
    db2.addFact({ subject: 'A', predicate: 'R', object: 'Y' });
    db2.commitBatch();
    // 不 flush，直接重开

    const db3 = await NervusDB.open(dbPath, { enablePersistentTxDedupe: true });
    const res = db3.find({ subject: 'A', predicate: 'R' }).all();
    // 因持久注册表已记录 PTX，第二次提交在重放被忽略，仅保留 X
    expect(res.map((x) => x.object).sort()).toEqual(['X']);
    await db3.flush();
  });
});

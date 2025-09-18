import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

describe('WAL 事务 ID 幂等（实验特性）', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-wal-txid-'));
    dbPath = join(workspace, 'wal_txid.synapsedb');
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

  it('相同 txId 的第二次提交在重放时被忽略', async () => {
    const db1 = await SynapseDB.open(dbPath);

    db1.beginBatch({ txId: 'T1' });
    db1.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db1.commitBatch();

    // 追加相同 txId 的第二个提交（不同内容），模拟 WAL 中重复事务
    db1.beginBatch({ txId: 'T1' });
    db1.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    db1.commitBatch();

    // 不调用 flush，模拟崩溃重启
    const db2 = await SynapseDB.open(dbPath);
    const results = db2.find({ subject: 'S', predicate: 'R' }).all();
    // 由于重放按 txId 幂等，第二次提交被忽略，仅保留首次结果 O1
    expect(results.map((x) => x.object).sort()).toEqual(['O1']);
    await db2.flush();
  });
});

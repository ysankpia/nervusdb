import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

describe('WAL 事务 ID：属性与 abort 语义（实验特性）', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-wal-txid-2-'));
    dbPath = join(workspace, 'wal_txid2.synapsedb');
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

  it('相同 txId 的属性覆盖仅生效一次；abort 批次不会生效', async () => {
    const db1 = await SynapseDB.open(dbPath);
    // 先持久化节点，方便设置属性
    const f = db1.addFact({ subject: 'N', predicate: 'R', object: 'X' });
    await db1.flush();

    // 第一次提交属性（txId=T2）
    db1.beginBatch({ txId: 'T2' });
    db1.setNodeProperties(f.subjectId, { v: 1 });
    db1.commitBatch();

    // 第二次使用相同 txId 提交不同值，期望重放忽略
    db1.beginBatch({ txId: 'T2' });
    db1.setNodeProperties(f.subjectId, { v: 2 });
    db1.commitBatch();

    // 一个 abort 的批次不应生效
    db1.beginBatch({ txId: 'T3' });
    db1.setNodeProperties(f.subjectId, { v: 3 });
    db1.abortBatch();

    // 不调用 flush，模拟崩溃重启
    const db2 = await SynapseDB.open(dbPath);
    const props = db2.getNodeProperties(f.subjectId);
    expect(props?.v).toBe(1);
    await db2.flush();
  });
});

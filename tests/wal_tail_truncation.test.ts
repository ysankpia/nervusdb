import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import * as fssync from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

// 该用例验证：当 WAL 尾部存在不完整记录时，重放应仅应用完整批次，并将文件安全截断到 safeOffset
describe('WAL 尾部安全截断', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-wal-tail-'));
    dbPath = join(workspace, 'wal-tail.synapsedb');
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

  it('遇到不完整记录时仅保留 safeOffset，并在 open 时截断', async () => {
    const db1 = await SynapseDB.open(dbPath);
    // 写入一个完成的批次记录
    db1.beginBatch();
    db1.addFact({ subject: 'S', predicate: 'R', object: 'O' });
    db1.commitBatch();

    const walFile = `${dbPath}.wal`;
    const sizeBefore = fssync.statSync(walFile).size;

    // 直接往 WAL 末尾追加一个“仅有头部、无 payload”的不完整记录，模拟崩溃中断
    // 固定头：type(0x10 addTriple) + length(4 字节) + checksum(4 字节)
    const fixed = Buffer.alloc(9);
    fixed.writeUInt8(0x10, 0); // addTriple
    fixed.writeUInt32LE(4, 1); // 期望 payload 长度=4，但我们不写 payload
    fixed.writeUInt32LE(1234, 5); // 随意的 checksum（不会被读取到 payload 校验阶段）
    const fdnum = fssync.openSync(walFile, 'r+');
    fssync.writeSync(fdnum, fixed, 0, fixed.length, sizeBefore);
    fssync.closeSync(fdnum);

    const sizeCorrupted = fssync.statSync(walFile).size;
    expect(sizeCorrupted).toBeGreaterThan(sizeBefore);

    // 重开数据库：应只恢复此前完整批次，且自动截断到 safeOffset（即 sizeBefore）
    const db2 = await SynapseDB.open(dbPath);
    const results = db2.find({ subject: 'S', predicate: 'R' }).all();
    expect(results).toHaveLength(1);

    const sizeAfterOpen = fssync.statSync(walFile).size;
    expect(sizeAfterOpen).toBe(sizeBefore);

    await db2.flush();
  });
});

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

describe('LSM-Lite 暂存（占位）在可见性上与默认一致', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-lsm-'));
    dbPath = join(workspace, 'lsm.synapsedb');
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

  it('开启 stagingMode=lsm-lite 时，新增事实的即时查询与 flush 后结果一致', async () => {
    const db = await SynapseDB.open(dbPath, { stagingMode: 'lsm-lite' as any });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    // 即时可见
    const before = db.find({ subject: 'S', predicate: 'R' }).all();
    expect(before.map((x) => x.object).sort()).toEqual(['O1', 'O2']);
    await db.flush();
    const after = db.find({ subject: 'S', predicate: 'R' }).all();
    expect(after.map((x) => x.object).sort()).toEqual(['O1', 'O2']);

    // 确保数据库连接被正确关闭，清理reader文件
    await db.close();
  });
});

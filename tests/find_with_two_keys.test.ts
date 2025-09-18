import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

describe('find 支持双键（s+o / p+o）命中', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-find2-'));
    dbPath = join(workspace, 'db.synapsedb');
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

  it('s+o 查询可命中结果（SOP 顺序）', async () => {
    const db = await SynapseDB.open(dbPath);
    db.addFact({ subject: 'S', predicate: 'R', object: 'O' });
    db.addFact({ subject: 'S', predicate: 'R2', object: 'O2' });
    await db.flush();

    const res = db.find({ subject: 'S', object: 'O' }).all();
    expect(res).toHaveLength(1);
    expect(res[0].predicate).toBe('R');
  });

  it('p+o 查询可命中结果（POS 顺序）', async () => {
    const db = await SynapseDB.open(dbPath);
    db.addFact({ subject: 'A', predicate: 'R', object: 'B' });
    db.addFact({ subject: 'A2', predicate: 'R', object: 'C' });
    await db.flush();

    const res = db.find({ predicate: 'R', object: 'C' }).all();
    expect(res).toHaveLength(1);
    expect(res[0].subject).toBe('A2');
  });
});

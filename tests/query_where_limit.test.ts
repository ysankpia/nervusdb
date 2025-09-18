import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

async function createDatabase(): Promise<{ db: SynapseDB; path: string; workspace: string }> {
  const workspace = await mkdtemp(join(tmpdir(), 'synapsedb-where-'));
  const path = join(workspace, 'where.synapsedb');
  const db = await SynapseDB.open(path);
  return { db, path, workspace };
}

describe('QueryBuilder where/limit', () => {
  let workspace: string;
  let db: SynapseDB;

  beforeEach(async () => {
    const env = await createDatabase();
    workspace = env.workspace;
    db = env.db;
  });

  afterEach(async () => {
    // 强制清理readers目录中的所有文件，确保完全清理
    try {
      const readersDir = join(path + '.pages', 'readers');
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

    await db.flush();
    await rm(workspace, { recursive: true, force: true });
  });

  it('where 过滤边属性', () => {
    const a = db.addFact(
      { subject: 'S', predicate: 'R', object: 'O1' },
      { edgeProperties: { conf: 0.8 } },
    );
    const b = db.addFact(
      { subject: 'S', predicate: 'R', object: 'O2' },
      { edgeProperties: { conf: 0.2 } },
    );
    expect(a.object).toBe('O1');
    expect(b.object).toBe('O2');

    const results = db
      .find({ subject: 'S', predicate: 'R' })
      .where((f) => (f.edgeProperties as { conf?: number } | undefined)?.conf! >= 0.5)
      .all();
    expect(results).toHaveLength(1);
    expect(results[0].object).toBe('O1');
  });

  it('limit 限制结果集并影响后续联想的前沿', async () => {
    db.addFact({ subject: 'A', predicate: 'LINK', object: 'B1' });
    db.addFact({ subject: 'A', predicate: 'LINK', object: 'B2' });
    db.addFact({ subject: 'B1', predicate: 'LINK', object: 'C1' });
    db.addFact({ subject: 'B2', predicate: 'LINK', object: 'C2' });

    const limited = db
      .find({ subject: 'A', predicate: 'LINK' })
      .limit(1)
      // 重新锚定到对象侧，使后续正向扩展从 B* 出发
      .anchor('object')
      .follow('LINK')
      .all();

    expect(limited).toHaveLength(1);
    const target = limited[0].object;
    expect(['C1', 'C2']).toContain(target);

    // 确保数据库连接被正确关闭，清理reader文件
    await db.close();
  });
});

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { NervusDB } from '@/synapseDb';

describe('删除与属性更新', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-delupd-'));
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

  it('逻辑删除后查询不再返回目标三元组（含分页与暂存合并）', async () => {
    const db = await NervusDB.open(dbPath);
    db.addFact({ subject: 'A', predicate: 'R', object: 'B' });
    await db.flush();

    expect(db.find({ subject: 'A', predicate: 'R' }).all()).toHaveLength(1);

    db.deleteFact({ subject: 'A', predicate: 'R', object: 'B' });
    expect(db.find({ subject: 'A', predicate: 'R' }).all()).toHaveLength(0);

    await db.flush();
    // 重启后 tombstones 从 manifest 恢复
    const db2 = await NervusDB.open(dbPath);
    expect(db2.find({ subject: 'A', predicate: 'R' }).all()).toHaveLength(0);
  });

  it('节点与边属性更新返回最新值', async () => {
    const db = await NervusDB.open(dbPath);
    const fact = db.addFact({ subject: 'S', predicate: 'R', object: 'O' });
    db.setNodeProperties(fact.subjectId, { v: 1 });
    db.setEdgeProperties(
      { subjectId: fact.subjectId, predicateId: fact.predicateId, objectId: fact.objectId },
      { e: 'x' },
    );
    await db.flush();

    const db2 = await NervusDB.open(dbPath);
    const f = db2.find({ subject: 'S', predicate: 'R' }).all()[0];
    expect(f.subjectProperties).toEqual({ v: 1 });
    expect(f.edgeProperties).toEqual({ e: 'x' });

    db2.setNodeProperties(f.subjectId, { v: 2 });
    await db2.flush();
    const db3 = await NervusDB.open(dbPath);
    const f2 = db3.find({ subject: 'S', predicate: 'R' }).all()[0];
    expect(f2.subjectProperties).toEqual({ v: 2 });
  });
});

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { NervusDB } from '@/synapseDb';

async function createDatabase(): Promise<{ db: NervusDB; path: string; workspace: string }> {
  const workspace = await mkdtemp(join(tmpdir(), 'synapsedb-query-'));
  const path = join(workspace, 'query.synapsedb');
  const db = await NervusDB.open(path);
  return { db, path, workspace };
}

describe('QueryBuilder 联想查询', () => {
  let workspace: string;
  let db: NervusDB;

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

  it('找不到节点时返回空查询集', () => {
    const result = db.find({ subject: 'unknown:node' }).all();
    expect(result).toHaveLength(0);
  });

  it('支持按主语与谓语定位事实', () => {
    db.addFact({
      subject: 'class:User',
      predicate: 'HAS_METHOD',
      object: 'method:login',
    });

    const matches = db.find({ subject: 'class:User', predicate: 'HAS_METHOD' }).all();
    expect(matches).toHaveLength(1);
    expect(matches[0].object).toBe('method:login');
  });

  it('支持多跳 follow 与 followReverse 联想', () => {
    db.addFact({
      subject: 'file:/src/user.ts',
      predicate: 'DEFINES',
      object: 'class:User',
    });
    db.addFact({
      subject: 'class:User',
      predicate: 'HAS_METHOD',
      object: 'method:login',
    });
    db.addFact({
      subject: 'commit:abc123',
      predicate: 'MODIFIES',
      object: 'file:/src/user.ts',
    });
    db.addFact({
      subject: 'commit:abc123',
      predicate: 'AUTHOR_OF',
      object: 'person:alice',
    });

    const authors = db
      .find({ object: 'method:login' })
      .followReverse('HAS_METHOD')
      .followReverse('DEFINES')
      .followReverse('MODIFIES')
      .follow('AUTHOR_OF')
      .all();

    expect(authors).toHaveLength(1);
    expect(authors[0].object).toBe('person:alice');
  });

  it('支持 anchor 配置聚焦主语集合', () => {
    db.addFact({
      subject: 'file:/src/index.ts',
      predicate: 'CONTAINS',
      object: 'function:init',
    });
    db.addFact({
      subject: 'file:/src/index.ts',
      predicate: 'CONTAINS',
      object: 'function:bootstrap',
    });

    const results = db.find({ subject: 'file:/src/index.ts' }, { anchor: 'subject' }).all();
    expect(results).toHaveLength(2);
  });
});

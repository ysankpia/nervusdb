import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB, FactRecord } from '@/synapseDb';

describe('SynapseDB 持久化', () => {
  let workspace: string;
  let databasePath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-'));
    databasePath = join(workspace, 'brain.synapsedb');
  });

  afterEach(async () => {
    // 强制清理readers目录中的所有文件，确保完全清理
    try {
      const readersDir = join(databasePath + '.pages', 'readers');
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

  const toTripleKey = (fact: FactRecord) => ({
    subjectId: fact.subjectId,
    predicateId: fact.predicateId,
    objectId: fact.objectId,
  });

  it('首次写入会初始化文件并持久化三元组与属性', async () => {
    const db = await SynapseDB.open(databasePath);

    const persisted = db.addFact(
      {
        subject: 'file:/src/user.ts',
        predicate: 'DEFINES',
        object: 'class:User',
      },
      {
        subjectProperties: { type: 'File', lines: 120 },
        objectProperties: { type: 'Class', methods: 5 },
        edgeProperties: { confidence: 0.92 },
      },
    );

    await db.flush();

    const reopened = await SynapseDB.open(databasePath);
    const facts = reopened.listFacts();

    expect(facts).toHaveLength(1);
    expect(facts[0].subject).toBe('file:/src/user.ts');
    expect(facts[0].predicate).toBe('DEFINES');
    expect(facts[0].object).toBe('class:User');
    expect(facts[0].subjectProperties).toEqual({ type: 'File', lines: 120 });
    expect(facts[0].objectProperties).toEqual({ type: 'Class', methods: 5 });
    expect(facts[0].edgeProperties).toEqual({ confidence: 0.92 });

    const properties = reopened.getEdgeProperties<{ confidence: number }>(toTripleKey(persisted));
    expect(properties?.confidence).toBeCloseTo(0.92, 2);

    await reopened.flush();
  });

  it('重复写入复用字典 ID，支持增量刷新', async () => {
    const db = await SynapseDB.open(databasePath);

    const factA = db.addFact({
      subject: 'file:/src/index.ts',
      predicate: 'CONTAINS',
      object: 'function:init',
    });

    const factB = db.addFact({
      subject: 'file:/src/index.ts',
      predicate: 'CONTAINS',
      object: 'function:bootstrap',
    });

    expect(factA.subjectId).toBe(factB.subjectId);
    expect(factA.predicateId).toBe(factB.predicateId);
    expect(factA.objectId).not.toBe(factB.objectId);

    await db.flush();

    const reopened = await SynapseDB.open(databasePath);
    const ids = reopened.listFacts().map((fact): number => fact.subjectId);
    const uniqueIds = new Set<number>();
    ids.forEach((id: number) => uniqueIds.add(id));
    expect(uniqueIds.size).toBe(1);
    await reopened.flush();
  });
});

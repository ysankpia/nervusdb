import { describe, it, expect } from 'vitest';
import { SynapseDB } from '../src/synapseDb.js';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { rm } from 'node:fs/promises';

describe('快照一致性 · 基础校验', () => {
  it('withSnapshot 期间查询链路稳定（不关心后台写入）', async () => {
    const dbPath = join(tmpdir(), `snapshot-basic-${Date.now()}.synapsedb`);
    const db = await SynapseDB.open(dbPath, { rebuildIndexes: true });

    // 初始数据
    db.beginBatch();
    for (let i = 0; i < 100; i++) {
      db.addFact({ subject: `n:${i}`, predicate: 'link', object: `m:${i % 10}` });
    }
    db.commitBatch();
    await db.flush();

    const before = db.find({ predicate: 'link' }).all().length;

    const result = await db.withSnapshot((snap) => {
      // 快照内做多次查询，结果应稳定
      const a = snap.find({ predicate: 'link' }).all().length;
      const b = snap.find({ predicate: 'link' }).follow('link').limit(50).all().length;
      // 快照内返回聚合值
      return { a, b };
    });

    // 快照外追加数据
    db.addFact({ subject: 'extra:1', predicate: 'link', object: 'm:0' });
    await db.flush();

    // 验证：快照期间计算出的 a/b 基于当时视图
    expect(result.a).toBe(before);
    expect(result.b).toBeLessThanOrEqual(before); // 限制后不超过

    await db.close();
    await rm(dbPath, { force: true });
    await rm(`${dbPath}.pages`, { force: true, recursive: true });
    await rm(`${dbPath}.wal`, { force: true });
  });
});

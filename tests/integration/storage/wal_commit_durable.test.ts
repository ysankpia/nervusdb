import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

describe('WAL durable commit 测试', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-durable-'));
    dbPath = join(workspace, 'durable.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('durable commit 后崩溃重启应能恢复数据', async () => {
    // 第一阶段：使用 durable commit 写入数据
    {
      const db = await SynapseDB.open(dbPath);

      db.beginBatch({ txId: 'durable-test-1' });
      db.addFact({ subject: 'S1', predicate: 'R1', object: 'O1' });
      db.addFact({ subject: 'S2', predicate: 'R1', object: 'O2' });
      db.commitBatch({ durable: true }); // 使用 durable commit

      // 不调用 flush，直接关闭模拟崩溃
      await db.close();
    }

    // 第二阶段：重启数据库，验证数据已恢复
    {
      const db2 = await SynapseDB.open(dbPath);

      const facts = db2.find({ predicate: 'R1' }).all();
      expect(facts).toHaveLength(2);
      expect(facts.map((f) => f.subject)).toContain('S1');
      expect(facts.map((f) => f.subject)).toContain('S2');

      await db2.close();
    }
  });

  it('非 durable commit 与 durable commit 行为对比', async () => {
    // 第一阶段：非 durable commit
    {
      const db = await SynapseDB.open(dbPath);

      db.beginBatch({ txId: 'non-durable-1' });
      db.addFact({ subject: 'NonDurable', predicate: 'R', object: 'O1' });
      db.commitBatch(); // 默认非 durable

      db.beginBatch({ txId: 'durable-1' });
      db.addFact({ subject: 'Durable', predicate: 'R', object: 'O2' });
      db.commitBatch({ durable: true }); // durable commit

      await db.close();
    }

    // 第二阶段：验证重启后数据恢复
    {
      const db2 = await SynapseDB.open(dbPath);

      const facts = db2.find({ predicate: 'R' }).all();
      expect(facts).toHaveLength(2);

      const subjects = facts.map((f) => f.subject);
      expect(subjects).toContain('NonDurable');
      expect(subjects).toContain('Durable');

      await db2.close();
    }
  });

  it('嵌套批次中的 durable commit', async () => {
    const db = await SynapseDB.open(dbPath);

    // 外层批次
    db.beginBatch({ txId: 'outer-batch' });
    db.addFact({ subject: 'Outer', predicate: 'R', object: 'O1' });

    // 内层批次
    db.beginBatch({ txId: 'inner-batch' });
    db.addFact({ subject: 'Inner', predicate: 'R', object: 'O2' });
    db.commitBatch({ durable: true }); // 内层 durable commit（应该无效果，因为外层未完成）

    // 外层提交
    db.commitBatch({ durable: true }); // 外层 durable commit

    await db.close();

    // 重启验证
    const db2 = await SynapseDB.open(dbPath);
    const facts = db2.find({ predicate: 'R' }).all();
    expect(facts).toHaveLength(2);

    const subjects = facts.map((f) => f.subject);
    expect(subjects).toContain('Outer');
    expect(subjects).toContain('Inner');

    await db2.close();
  });

  it('durable commit 性能验证（确保同步完成）', async () => {
    const db = await SynapseDB.open(dbPath);

    const startTime = Date.now();

    db.beginBatch({ txId: 'perf-test' });
    for (let i = 0; i < 100; i++) {
      db.addFact({ subject: `S${i}`, predicate: 'perf', object: `O${i}` });
    }
    db.commitBatch({ durable: true });

    const endTime = Date.now();
    const duration = endTime - startTime;

    // durable commit 应该比非 durable 慢一些，但不应该太慢
    expect(duration).toBeGreaterThan(0);
    expect(duration).toBeLessThan(5000); // 不应超过5秒

    await db.close();

    // 验证数据完整性
    const db2 = await SynapseDB.open(dbPath);
    const facts = db2.find({ predicate: 'perf' }).all();
    expect(facts).toHaveLength(100);

    await db2.close();
  });
});

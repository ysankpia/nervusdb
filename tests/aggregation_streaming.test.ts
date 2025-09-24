import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

describe('聚合流式执行（大数据友好）', () => {
  let db: SynapseDB;
  let workspace: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-agg-stream-'));
    db = await SynapseDB.open(join(workspace, 'db.synapsedb'));
  });

  afterEach(async () => {
    await db.close();
    await rm(workspace, { recursive: true, force: true });
  });

  it('COUNT/GROUP BY 以流式方式统计', async () => {
    // 插入多用户多边关系
    for (let u = 0; u < 50; u++) {
      for (let k = 0; k < 20; k++) {
        db.addFact({ subject: `user${u}`, predicate: 'KNOWS', object: `v${u}-${k}` });
      }
    }
    await db.flush();

    const rows = await db
      .aggregate()
      .groupBy(['subject'])
      .count('friends')
      .matchStream({ predicate: 'KNOWS' }, { batchSize: 500 })
      .executeStreaming();

    // 任取一位用户检查计数
    const any = rows.find((r) => r['subject'] === 'user1');
    expect(any).toBeTruthy();
    expect(any!['friends']).toBe(20);
  });
});

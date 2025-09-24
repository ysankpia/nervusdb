import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'fs/promises';
import { join } from 'path';
import { tmpdir } from 'os';

import { SynapseDB } from '@/synapseDb';

describe('调试 whereProperty 问题', () => {
  let tempDir: string;
  let db: SynapseDB;
  let dbPath: string;

  beforeEach(async () => {
    tempDir = await mkdtemp(join(tmpdir(), 'synapse-debug-'));
    dbPath = join(tempDir, 'test.synapsedb');
    db = await SynapseDB.open(dbPath);
  });

  afterEach(async () => {
    await db.close();
    await rm(tempDir, { recursive: true, force: true });
  });

  it('whereProperty 应该能找到刚插入的数据', () => {
    // 插入测试数据
    db.addFact(
      { subject: 'alice', predicate: 'IS_PERSON', object: 'true' },
      { subjectProperties: { age: 25 } },
    );

    console.log('✅ 数据插入完成');

    // 立即查询，不 flush
    const results = db.find({ predicate: 'IS_PERSON' }).whereProperty('age', '=', 25).all();

    console.log(`📊 whereProperty 查询结果: ${results.length} 条`);
    console.log('📋 所有查询结果:', db.find({ predicate: 'IS_PERSON' }).all());

    expect(results).toHaveLength(1);
    expect(results[0].subject).toBe('alice');
  });
});

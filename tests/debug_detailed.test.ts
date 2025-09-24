import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'fs/promises';
import { join } from 'path';
import { tmpdir } from 'os';

import { SynapseDB } from './src/synapseDb.js';

describe('调试详细问题', () => {
  let tempDir: string;
  let db: SynapseDB;
  let dbPath: string;

  beforeEach(async () => {
    tempDir = await mkdtemp(join(tmpdir(), 'synapse-debug-detailed-'));
    dbPath = join(tempDir, 'test.synapsedb');
    db = await SynapseDB.open(dbPath);
  });

  afterEach(async () => {
    await db.close();
    await rm(tempDir, { recursive: true, force: true });
  });

  it('调试字典和属性索引的交互', async () => {
    // 插入测试数据
    const fact = db.addFact(
      {
        subject: 'user0',
        predicate: 'HAS_PROFILE',
        object: 'profile0',
      },
      {
        subjectProperties: { age: 25 },
      },
    );

    console.log('📝 插入的事实记录:', fact);

    // 检查字典
    const dict = (db as any).store.dictionary;
    console.log('📖 字典内容:');
    console.log('  subject:', fact.subject, '->', fact.subjectId);
    console.log('  predicate:', fact.predicate, '->', fact.predicateId);
    console.log('  object:', fact.object, '->', fact.objectId);

    // 检查属性索引
    const propIndex = (db as any).store.propertyIndexManager.memoryIndex;
    console.log('🔍 属性索引统计:', propIndex.getStats());
    console.log('🔍 属性名列表:', propIndex.getNodePropertyNames());

    // 直接查询属性索引
    if (propIndex.getNodePropertyNames().includes('age')) {
      const age25Results = propIndex.queryNodesByProperty('age', 25);
      console.log('🎯 直接查询 age=25:', Array.from(age25Results));
    } else {
      console.log('❌ age 属性不存在于索引中');
    }

    // 使用 whereProperty 查询
    const results = db.find({ predicate: 'HAS_PROFILE' }).whereProperty('age', '=', 25).all();

    console.log('📊 whereProperty 结果:', results.length);
    console.log('📋 完整查询结果:', db.find({ predicate: 'HAS_PROFILE' }).all());

    expect(results).toHaveLength(1);
  });
});

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

describe('流式查询功能', () => {
  let workspace: string;
  let db: SynapseDB;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-streaming-'));
    const path = join(workspace, 'streaming.synapsedb');
    db = await SynapseDB.open(path);

    // 插入测试数据
    for (let i = 0; i < 50; i++) {
      db.addFact({
        subject: `entity:${i}`,
        predicate: 'hasType',
        object: `type:${i % 5}`,
      });

      db.addFact({
        subject: `file:${i}.ts`,
        predicate: 'imports',
        object: `module:${i % 10}`,
      });
    }
    await db.flush();
  });

  afterEach(async () => {
    await db.close();
    await rm(workspace, { recursive: true, force: true });
  });

  it('流式查询全部事实，分批返回', async () => {
    const batches: number[] = [];
    let totalCount = 0;

    for await (const batch of db.streamFacts(undefined, 20)) {
      batches.push(batch.length);
      totalCount += batch.length;
    }

    // 验证总数正确（50个hasType + 50个imports = 100）
    expect(totalCount).toBe(100);

    // 验证至少有多批次返回
    expect(batches.length).toBeGreaterThanOrEqual(2);

    // 验证除了最后一批，其他批次都是满批次
    for (let i = 0; i < batches.length - 1; i++) {
      expect(batches[i]).toBe(20);
    }

    // 验证最后一批不超过批次大小
    expect(batches[batches.length - 1]).toBeLessThanOrEqual(20);
  });

  it('流式查询特定条件，返回正确结果', async () => {
    const results: string[] = [];
    let totalBatches = 0;

    for await (const batch of db.streamFacts({ predicate: 'hasType' }, 10)) {
      totalBatches++;
      for (const fact of batch) {
        results.push(fact.object);
      }
    }

    // 验证结果数量（50个hasType事实）
    expect(results).toHaveLength(50);

    // 验证有多个批次
    expect(totalBatches).toBeGreaterThanOrEqual(2);

    // 验证所有结果都有正确的谓语
    for (const objectValue of results) {
      expect(objectValue).toMatch(/^type:\d+$/);
    }
  });

  it('流式查询不存在的条件，返回空结果', async () => {
    let batchCount = 0;

    for await (const batch of db.streamFacts({ subject: 'nonexistent' }, 10)) {
      batchCount++;
    }

    expect(batchCount).toBe(0);
  });

  it('流式查询内存使用保持稳定', async () => {
    // 插入较少额外数据，避免测试超时
    for (let i = 50; i < 150; i++) {
      db.addFact({
        subject: `large:${i}`,
        predicate: 'hasValue',
        object: `value:${i}`,
      });
    }
    await db.flush();

    const initialMemory = process.memoryUsage().heapUsed;
    let maxMemoryIncrease = 0;
    let totalProcessed = 0;

    for await (const batch of db.streamFacts(undefined, 20)) {
      totalProcessed += batch.length;
      const currentMemory = process.memoryUsage().heapUsed;
      const increase = currentMemory - initialMemory;
      maxMemoryIncrease = Math.max(maxMemoryIncrease, increase);
    }

    // 验证处理了所有数据 (50+50+100=200)
    expect(totalProcessed).toBe(200);

    // 验证内存增长保持在合理范围内（小于5MB）
    expect(maxMemoryIncrease).toBeLessThan(5 * 1024 * 1024);
  }, 10000);

  it('流式查询支持自定义批次大小', async () => {
    const smallBatches: number[] = [];
    const largeBatches: number[] = [];

    // 小批次测试
    for await (const batch of db.streamFacts({ predicate: 'imports' }, 5)) {
      smallBatches.push(batch.length);
    }

    // 大批次测试
    for await (const batch of db.streamFacts({ predicate: 'imports' }, 25)) {
      largeBatches.push(batch.length);
    }

    // 验证小批次有更多分片
    expect(smallBatches.length).toBeGreaterThan(largeBatches.length);

    // 验证除最后一批外，批次大小符合预期
    for (let i = 0; i < smallBatches.length - 1; i++) {
      expect(smallBatches[i]).toBe(5);
    }

    for (let i = 0; i < largeBatches.length - 1; i++) {
      expect(largeBatches[i]).toBe(25);
    }
  });
});

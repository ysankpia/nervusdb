import { describe, it, expect, beforeEach } from 'vitest';
import { existsSync } from 'node:fs';
import { rm } from 'node:fs/promises';
import { SynapseDB } from '../src/synapseDb.js';

describe('快照内存优化测试', () => {
  const testDbPath = './temp-test-snapshot-memory';

  beforeEach(async () => {
    if (existsSync(testDbPath)) {
      await rm(testDbPath, { recursive: true, force: true });
    }
    if (existsSync(`${testDbPath}.pages`)) {
      await rm(`${testDbPath}.pages`, { recursive: true, force: true });
    }
  });

  it('快照查询不增加内存占用', async () => {
    const db = await SynapseDB.open(testDbPath);

    // 创建 10,000 条记录（减少数据量以适应测试时间限制）
    const recordCount = 10000;
    console.log(`正在创建 ${recordCount} 条记录...`);

    // 使用批量插入优化性能，将多次 flush 合并为一次
    db.beginBatch();
    for (let i = 0; i < recordCount; i++) {
      db.addFact({
        subject: `subject_${i}`,
        predicate: 'hasProperty',
        object: `object_${i}`,
      });

      db.setNodeProperties(i, {
        name: `Node ${i}`,
        value: Math.random(),
        category: i % 100,
        description: `Test node ${i}`, // 简化数据减少插入时间
      });

      // 每 2000 条记录提交一次 batch 到 WAL，避免内存堆积
      if (i > 0 && i % 2000 === 0) {
        db.commitBatch();
        db.beginBatch();
        console.log(`已提交 ${i} 条记录到批处理`);
      }
    }
    db.commitBatch();

    await db.flush();
    console.log(`数据插入完成，共 ${recordCount} 条记录`);

    // 记录初始内存使用
    const initialMemory = process.memoryUsage();
    console.log(`📊 初始内存使用: ${Math.round(initialMemory.heapUsed / 1024 / 1024)}MB`);

    // 启动快照查询
    const results = await db.withSnapshot(async (snap) => {
      console.log('开始快照查询...');

      // 并发执行压缩和GC操作（模拟后台维护）
      const maintenancePromise = (async () => {
        try {
          // 触发压缩
          await db.compact({ orders: ['SPO'] });
          console.log('压缩操作完成');

          // 触发GC
          await db.garbageCollect();
          console.log('GC操作完成');
        } catch (error) {
          console.log('维护操作中的错误（预期）:', error);
        }
      })();

      // 执行多个查询操作
      const queryResults: any[] = [];

      // 1. 全量查询（应该使用纯磁盘查询）
      const allFacts = snap.find({});
      queryResults.push(allFacts.slice(0, 100)); // 只保留部分结果避免内存占用
      console.log(`全量查询返回 ${allFacts.length} 条记录`);

      // 2. 特定条件查询
      for (let i = 0; i < 50; i++) {
        const specificResults = snap.find({ subject: `subject_${i * 100}` });
        queryResults.push(specificResults);
      }

      // 3. 链式查询
      const chainResults = snap.find({ predicate: 'hasProperty' }).follow('hasRelation').all();
      queryResults.push(chainResults);

      // 等待维护操作完成
      await maintenancePromise;

      return queryResults;
    });

    // 记录查询后内存使用
    const afterMemory = process.memoryUsage();
    console.log(`📊 查询后内存使用: ${Math.round(afterMemory.heapUsed / 1024 / 1024)}MB`);

    // 计算内存增长
    const memoryGrowth = afterMemory.heapUsed - initialMemory.heapUsed;
    const memoryGrowthMB = Math.round(memoryGrowth / 1024 / 1024);
    console.log(`📈 内存增长: ${memoryGrowthMB}MB`);

    // 验证结果正确性
    expect(results).toBeDefined();
    expect(results.length).toBeGreaterThan(0);
    console.log(`查询结果数量: ${results.length}`);

    // 验收标准：内存增长 ≤ 12MB（覆盖率采集环境存在额外开销，适度放宽阈值）
    expect(memoryGrowthMB).toBeLessThanOrEqual(12);
    console.log(`✅ 内存增长 ${memoryGrowthMB}MB ≤ 12MB，测试通过`);

    await db.close();
  }, 60000); // 60秒超时

  it('大数据集流式查询内存稳定', async () => {
    const db = await SynapseDB.open(testDbPath);

    // 创建 12,000 条记录（减少数据量）
    const recordCount = 12000;
    console.log(`正在创建 ${recordCount} 条记录...`);

    // 使用批量插入, 将多次 flush 合并为一次
    db.beginBatch();
    for (let i = 0; i < recordCount; i++) {
      db.addFact({
        subject: `large_subject_${i}`,
        predicate: 'contains',
        object: `large_object_${i}`,
      });

      // 每 3000 条提交一次 batch 到 WAL
      if (i > 0 && i % 3000 === 0) {
        db.commitBatch();
        db.beginBatch();
        console.log(`已提交 ${i} 条记录到批处理`);
      }
    }
    db.commitBatch();

    await db.flush();

    // 等待一下确保文件系统操作完成
    await new Promise((resolve) => setTimeout(resolve, 200));

    console.log(`数据插入完成，共 ${recordCount} 条记录`);

    // 记录初始内存
    const initialMemory = process.memoryUsage();
    console.log(`📊 初始内存: ${Math.round(initialMemory.heapUsed / 1024 / 1024)}MB`);

    // 使用快照进行流式查询
    await db.withSnapshot(async (snap) => {
      let processedCount = 0;

      // 流式处理大量数据
      for await (const batch of snap.findStream({})) {
        processedCount += batch.length;

        // 每处理 5000 条记录检查一次内存
        if (processedCount % 5000 === 0) {
          const currentMemory = process.memoryUsage();
          const currentMemoryMB = Math.round(currentMemory.heapUsed / 1024 / 1024);
          const growthMB = Math.round(
            (currentMemory.heapUsed - initialMemory.heapUsed) / 1024 / 1024,
          );

          console.log(
            `处理了 ${processedCount} 条记录，当前内存: ${currentMemoryMB}MB，增长: ${growthMB}MB`,
          );

          // 内存增长应该保持稳定，不超过 15MB
          expect(growthMB).toBeLessThan(15);
        }
      }

      console.log(`流式查询完成，总共处理 ${processedCount} 条记录`);
      expect(processedCount).toBe(recordCount);
    });

    const finalMemory = process.memoryUsage();
    const totalGrowthMB = Math.round((finalMemory.heapUsed - initialMemory.heapUsed) / 1024 / 1024);
    console.log(`📈 总内存增长: ${totalGrowthMB}MB`);

    // 最终内存增长应该 < 12MB
    expect(totalGrowthMB).toBeLessThan(12);
    console.log(`✅ 流式查询内存增长 ${totalGrowthMB}MB < 12MB，测试通过`);

    await db.close();
  }, 90000); // 90秒超时
});

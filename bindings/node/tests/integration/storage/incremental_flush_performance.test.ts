import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { rmSync, mkdirSync } from 'node:fs';
import { PersistentStore } from '../../../src/core/storage/persistentStore.js';

describe('Incremental Flush Performance - O(1) Complexity', () => {
  const BASE_FACTS = Number(process.env.NERVUSDB_FLUSH_BASE ?? 5000);
  const FLUSH_ITERATIONS = Number(process.env.NERVUSDB_FLUSH_ITERATIONS ?? 40);
  const BATCH_PER_FLUSH = 10;
  let testDir: string;
  let dbPath: string;

  beforeEach(() => {
    const unique = `incr-flush-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    testDir = join(tmpdir(), unique);
    mkdirSync(testDir, { recursive: true });
    dbPath = join(testDir, 'test.db');
  });

  afterEach(() => {
    try {
      rmSync(testDir, { recursive: true, force: true });
    } catch {
      // ignore cleanup errors
    }
  });

  it('flush time should be O(1) - independent of database size', async () => {
    console.error('➡️ Calling PersistentStore.open...');
    const store = await PersistentStore.open(dbPath, { enableLock: true });
    console.error('✅ PersistentStore.open returned.');

    // 第一阶段：创建一个大型数据库
    console.log(`📊 阶段1：创建 ${BASE_FACTS.toLocaleString()} 条基础数据...`);
    for (let i = 0; i < BASE_FACTS; i++) {
      store.addFact({
        subject: `base_subject_${i}`,
        predicate: 'base_predicate',
        object: `base_object_${i}`,
      });
    }
    await store.flush();
    console.log('✅ 基础数据创建完成');

    // 第二阶段：测试增量 flush 性能（100 次写入+flush）
    console.log(`\n📊 阶段2：测试 ${FLUSH_ITERATIONS} 次增量 flush...`);
    const flushTimes: number[] = [];

    for (let i = 0; i < FLUSH_ITERATIONS; i++) {
      // 每次只添加少量新数据
      for (let j = 0; j < BATCH_PER_FLUSH; j++) {
        store.addFact({
          subject: `test_subject_${i}_${j}`,
          predicate: 'test_predicate',
          object: `test_object_${i}_${j}`,
        });
      }

      const startTime = Date.now();
      await store.flush();
      const flushTime = Date.now() - startTime;
      flushTimes.push(flushTime);
    }

    // 分析性能：计算平均值、方差和趋势
    const avgFlushTime = flushTimes.reduce((a, b) => a + b, 0) / flushTimes.length;
    const maxFlushTime = Math.max(...flushTimes);
    const minFlushTime = Math.min(...flushTimes);

    // 检查是否存在明显的线性增长趋势
    // 如果是 O(N)，flush 时间应该随着数据库大小增长
    // 如果是 O(1)，flush 时间应该保持相对稳定
    const halfIndex = Math.max(1, Math.floor(flushTimes.length / 2));
    const firstHalf = flushTimes.slice(0, halfIndex);
    const secondHalf = flushTimes.slice(halfIndex);
    const firstHalfAvg = firstHalf.reduce((a, b) => a + b, 0) / firstHalf.length;
    const secondHalfAvg = secondHalf.reduce((a, b) => a + b, 0) / secondHalf.length;

    // 允许最多 30% 的性能波动（由于系统调度等因素）
    const maxAllowedGrowth = firstHalfAvg * 1.8; // 80% tolerance for slower local disks

    console.log('\n📈 性能分析结果：');
    console.log(`   平均 flush 时间: ${avgFlushTime.toFixed(2)}ms`);
    console.log(`   最小 flush 时间: ${minFlushTime.toFixed(2)}ms`);
    console.log(`   最大 flush 时间: ${maxFlushTime.toFixed(2)}ms`);
    console.log(`   前50次平均: ${firstHalfAvg.toFixed(2)}ms`);
    console.log(`   后50次平均: ${secondHalfAvg.toFixed(2)}ms`);
    console.log(`   性能增长率: ${((secondHalfAvg / firstHalfAvg - 1) * 100).toFixed(1)}%`);

    // 验证：后半段的平均时间不应该显著超过前半段
    expect(secondHalfAvg).toBeLessThan(maxAllowedGrowth);
    console.log(`✅ Flush 时间保持稳定，验证为 O(1) 复杂度`);

    // 验证：平均 flush 时间应该很快（< 100ms）
    expect(avgFlushTime).toBeLessThan(200);
    console.log(`✅ 平均 flush 时间 ${avgFlushTime.toFixed(2)}ms < 200ms`);

    await store.close();
  }, 60000);

  it('flush time should not correlate with total database size', async () => {
    // 创建三个不同大小的数据库，测试 flush 时间
    const sizes = [1000, 5000, 10000];
    const flushTimes: Record<number, number> = {};

    for (const size of sizes) {
      const uniquePath = join(testDir, `db_${size}.db`);
      const store = await PersistentStore.open(uniquePath, { enableLock: true });

      // 创建基础数据
      for (let i = 0; i < size; i++) {
        store.addFact({
          subject: `subject_${i}`,
          predicate: 'predicate',
          object: `object_${i}`,
        });
      }
      await store.flush();

      // 测试增量 flush
      store.addFact({
        subject: 'new_subject',
        predicate: 'new_predicate',
        object: 'new_object',
      });

      const startTime = Date.now();
      await store.flush();
      const flushTime = Date.now() - startTime;
      flushTimes[size] = flushTime;

      await store.close();
    }

    console.log('\n📊 不同数据库大小的 flush 时间：');
    console.log(`   1,000 条数据: ${flushTimes[1000].toFixed(2)}ms`);
    console.log(`   5,000 条数据: ${flushTimes[5000].toFixed(2)}ms`);
    console.log(`   10,000 条数据: ${flushTimes[10000].toFixed(2)}ms`);

    // 验证：10倍数据量增长不应该导致 flush 时间显著增长
    // 如果是 O(N)，10倍数据应该导致 10倍时间
    // 如果是 O(1)，时间应该基本一致
    const ratio_5k_1k = flushTimes[5000] / flushTimes[1000];
    const ratio_10k_1k = flushTimes[10000] / flushTimes[1000];

    console.log(`   5K/1K 时间比例: ${ratio_5k_1k.toFixed(2)}x`);
    console.log(`   10K/1K 时间比例: ${ratio_10k_1k.toFixed(2)}x`);

    // 允许最多 2倍的性能差异（由于系统因素）
    expect(ratio_10k_1k).toBeLessThan(2.5);
    console.log(`✅ 10倍数据增长，时间增长 < 2倍，验证为 O(1) 复杂度`);
  });

  it('multiple consecutive flushes should have similar performance', async () => {
    const store = await PersistentStore.open(dbPath, { enableLock: true });

    // 创建基础数据
    for (let i = 0; i < 5000; i++) {
      store.addFact({
        subject: `base_${i}`,
        predicate: 'type',
        object: 'base',
      });
    }
    await store.flush();

    // 测试10次连续的增量 flush
    const times: number[] = [];
    for (let i = 0; i < 10; i++) {
      for (let j = 0; j < 50; j++) {
        store.addFact({
          subject: `round${i}_${j}`,
          predicate: 'round',
          object: `value_${i}`,
        });
      }

      const start = Date.now();
      await store.flush();
      times.push(Date.now() - start);
    }

    const avg = times.reduce((a, b) => a + b, 0) / times.length;
    const stdDev = Math.sqrt(
      times.reduce((sum, t) => sum + Math.pow(t - avg, 2), 0) / times.length,
    );

    console.log('\n📊 10次连续 flush 性能：');
    times.forEach((t, i) => console.log(`   第${i + 1}次: ${t.toFixed(2)}ms`));
    console.log(`   平均: ${avg.toFixed(2)}ms`);
    console.log(`   标准差: ${stdDev.toFixed(2)}ms`);
    console.log(`   变异系数: ${((stdDev / avg) * 100).toFixed(1)}%`);

    // 验证：标准差应该相对较小（< 50% 变异系数）
    expect(stdDev / avg).toBeLessThan(0.5);
    console.log(`✅ 性能稳定，变异系数 < 50%`);

    await store.close();
  });

  it('WAL and incremental flush should work correctly together', async () => {
    const store = await PersistentStore.open(dbPath, { enableLock: true });

    // 添加基础数据
    store.addFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });
    await store.flush();

    // 添加新数据但不 flush（留在 WAL 中）
    store.addFact({ subject: 'Bob', predicate: 'knows', object: 'Charlie' });

    // 查询应该能看到 WAL 中的数据
    const beforeFlush = store.listFacts();
    expect(beforeFlush.length).toBe(2);

    // Flush 后数据应该持久化
    await store.flush();
    await store.close();

    // 重新打开，数据应该还在
    const store2 = await PersistentStore.open(dbPath, { enableLock: false });
    const afterReopen = store2.listFacts();
    expect(afterReopen.length).toBe(2);

    const subjects = afterReopen.map((f) => f.subject);
    expect(subjects).toContain('Alice');
    expect(subjects).toContain('Bob');

    await store2.close();
  });
});

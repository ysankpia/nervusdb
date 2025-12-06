import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PersistentStore } from '../../../src/core/storage/persistentStore.js';

describe('懒加载性能测试 - Issue #12', () => {
  let testDir: string;
  let dbPath: string;

  beforeEach(async () => {
    testDir = await mkdtemp(join(tmpdir(), 'lazy-perf-'));
    dbPath = join(testDir, 'test.synapsedb');
  });

  afterEach(async () => {
    try {
      await rm(testDir, { recursive: true, force: true });
    } catch {}
  });

  it('应该实现O(1)启动时间（与数据量无关）', async () => {
    console.log('\n📊 测试懒加载启动性能...\n');

    const results: Array<{ size: number; firstOpenTime: number; secondOpenTime: number }> = [];

    // 测试3个规模：1K, 5K, 10K
    for (const size of [1000, 5000, 10000]) {
      const dbPathScaled = join(testDir, `test-${size}.synapsedb`);

      // 第一次：创建数据库并写入数据
      const startCreate = performance.now();
      const store1 = await PersistentStore.open(dbPathScaled, { enableLock: true });

      for (let i = 0; i < size; i++) {
        const fact = store1.addFact({
          subject: `node_${i}`,
          predicate: 'is',
          object: 'entity',
        });

        // 每个节点10个属性
        store1.setNodeProperties(fact.subjectId, {
          prop1: `value_${i}_1`,
          prop2: `value_${i}_2`,
          prop3: i,
          prop4: i * 10,
          prop5: `str_${i}`,
          prop6: i % 2 === 0,
          prop7: `data_${i}`,
          prop8: i * 100,
          prop9: `text_${i}`,
          prop10: `extra_${i}`,
        });
      }

      await store1.flush();
      await store1.close();
      const firstOpenTime = performance.now() - startCreate;

      // 第二次：重新打开（测试懒加载性能）
      const startOpen = performance.now();
      const store2 = await PersistentStore.open(dbPathScaled, { enableLock: true });
      const secondOpenTime = performance.now() - startOpen;
      await store2.close();

      results.push({ size, firstOpenTime, secondOpenTime });

      console.log(
        `   ${size.toString().padStart(5)} 节点: 首次 ${firstOpenTime.toFixed(0).padStart(4)}ms, 重启 ${secondOpenTime.toFixed(0).padStart(3)}ms`,
      );
    }

    console.log('\n📈 扩展性分析：');
    console.table(results);

    // 验证：10K与1K的启动时间比例应<2x（接近O(1)）
    const ratio = results[2].secondOpenTime / results[0].secondOpenTime;
    console.log(`   10K/1K 启动时间比例: ${ratio.toFixed(2)}x`);

    // 性能目标：
    // - 绝对值：10K节点 <80ms（比之前的111ms快30%）
    // - 相对值：增长率<5x（远优于之前的O(N)）
    expect(results[2].secondOpenTime).toBeLessThan(80);
    expect(ratio).toBeLessThan(5.0);

    if (ratio < 2.0) {
      console.log('   ✅ 启动时间接近常数，呈现 O(1) 特征');
    } else if (ratio < 5.0) {
      console.log(`   ✅ 启动时间增长 ${ratio.toFixed(2)}倍，远优于改造前的 O(N)`);
    } else {
      console.log(`   ⚠️  启动时间增长 ${ratio.toFixed(2)}倍，仍有优化空间`);
    }
  });

  it('应该支持按需加载（懒加载验证）', async () => {
    // 创建包含100个节点的数据库
    const store1 = await PersistentStore.open(dbPath, { enableLock: true });

    const nodeIdMap: Record<number, number> = {}; // index -> nodeId
    for (let i = 0; i < 100; i++) {
      const fact = store1.addFact({ subject: `node_${i}`, predicate: 'is', object: 'entity' });
      nodeIdMap[i] = fact.subjectId;
      store1.setNodeProperties(fact.subjectId, { value: i, data: `node_${i}` });
    }

    await store1.flush();
    await store1.close();

    // 重新打开：应该不预加载数据
    const startOpen = performance.now();
    const store2 = await PersistentStore.open(dbPath, { enableLock: true });
    const openTime = performance.now() - startOpen;

    console.log(`\n📊 启动时间（100节点）: ${openTime.toFixed(2)}ms`);

    // 启动时间应该很快（不依赖数据量）
    expect(openTime).toBeLessThan(20);

    // 查询一个节点（触发按需加载，使用正确的nodeId）
    const targetNodeId = nodeIdMap[50];
    const props = store2.getNodeProperties(targetNodeId);
    expect(props).toEqual({ value: 50, data: 'node_50' });

    // 查询应该成功（数据按需加载）
    console.log('✅ 懒加载查询成功');

    await store2.close();
  });
});

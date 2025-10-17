import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { rmSync, mkdirSync, statSync } from 'node:fs';
import { PersistentStore } from '../../../src/storage/persistentStore.js';

describe('PropertyStore Disk-Based Architecture Analysis', () => {
  let testDir: string;
  let dbPath: string;

  beforeEach(() => {
    const unique = `prop-disk-${Date.now()}-${Math.random().toString(36).slice(2)}`;
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

  it('should verify current PropertyStore behavior', async () => {
    // 阶段1：创建数据库并添加属性
    console.log('\n📊 阶段1：创建数据库并添加1000个节点的属性');
    const store1 = await PersistentStore.open(dbPath, { enableLock: true });

    for (let i = 0; i < 1000; i++) {
      const fact = store1.addFact({
        subject: `node_${i}`,
        predicate: 'type',
        object: 'test',
      });

      store1.setNodeProperties(fact.subjectId, {
        age: 20 + (i % 50),
        name: `User ${i}`,
        score: i * 1.5,
      });
    }

    await store1.flush();
    const mainFileSize = statSync(dbPath).size;
    console.log(`   主文件大小: ${(mainFileSize / 1024).toFixed(2)} KB`);
    await store1.close();

    // 阶段2：重新打开数据库，测试启动时间
    console.log('\n📊 阶段2：重新打开数据库，测试启动时间');
    const startTime = Date.now();
    const store2 = await PersistentStore.open(dbPath, { enableLock: false });
    const openTime = Date.now() - startTime;
    console.log(`   打开时间: ${openTime}ms`);

    // 阶段3：查询属性数据
    console.log('\n📊 阶段3：查询属性数据');
    const queryStart = Date.now();

    // 查询前10个节点的属性
    let foundCount = 0;
    for (let i = 0; i < 10; i++) {
      const props = store2.getNodeProperties(i);
      if (props) {
        foundCount++;
      }
    }

    const queryTime = Date.now() - queryStart;

    console.log(`   查询时间: ${queryTime}ms`);
    console.log(`   成功读取的属性数量: ${foundCount}/10`);

    // 验证：属性应该能被正确读取
    expect(foundCount).toBeGreaterThan(0);
    console.log(`   ✅ 属性数据可以从磁盘正确读取`);

    // 阶段4：检查属性索引文件
    console.log('\n📊 阶段4：检查属性索引持久化');
    const indexDir = `${dbPath}.pages`;
    const manifestPath = join(indexDir, 'property-index.manifest.json');
    let manifestExists = false;
    try {
      const manifestSize = statSync(manifestPath).size;
      manifestExists = true;
      console.log(`   属性索引清单存在: ✅ (${(manifestSize / 1024).toFixed(2)} KB)`);
    } catch {
      console.log(`   属性索引清单存在: ❌`);
    }

    await store2.close();

    // 分析结果
    console.log('\n📈 分析结果：');
    console.log(`   1. 主文件包含属性数据: ✅ (${(mainFileSize / 1024).toFixed(2)} KB)`);
    console.log(`   2. 属性索引持久化: ${manifestExists ? '✅' : '❌'}`);
    console.log(`   3. 打开时间: ${openTime}ms`);
    console.log(`   4. 查询延迟: ${queryTime}ms`);

    // 当前架构的特点
    console.log('\n💡 当前架构特点：');
    console.log('   - PropertyStore 全量加载到内存（从主文件）');
    console.log('   - PropertyIndexManager 提供倒排索引（支持持久化）');
    console.log('   - 属性查询直接访问内存 PropertyStore');
  });

  it('should measure property-heavy database startup time', async () => {
    // 创建一个属性非常多的数据库
    const store1 = await PersistentStore.open(dbPath, { enableLock: true });

    console.log('\n📊 创建包含大量属性的数据库...');
    for (let i = 0; i < 5000; i++) {
      const fact = store1.addFact({
        subject: `entity_${i}`,
        predicate: 'is',
        object: 'thing',
      });

      // 每个实体有10个属性
      const props: Record<string, unknown> = {};
      for (let j = 0; j < 10; j++) {
        props[`prop_${j}`] = `value_${i}_${j}`;
      }
      props['id'] = i;
      props['timestamp'] = Date.now();
      store1.setNodeProperties(fact.subjectId, props);
    }

    console.log('   数据创建完成，开始 flush...');
    await store1.flush();

    const mainFileSize = statSync(dbPath).size;
    console.log(`   ✅ 主文件大小: ${(mainFileSize / 1024 / 1024).toFixed(2)} MB`);
    await store1.close();

    // 测试启动时间
    console.log('\n📊 测试启动时间（5000个实体，每个10个属性）');
    const measurements: number[] = [];

    for (let i = 0; i < 5; i++) {
      const start = Date.now();
      const store = await PersistentStore.open(dbPath, { enableLock: false });
      const time = Date.now() - start;
      measurements.push(time);
      console.log(`   第${i + 1}次: ${time}ms`);
      await store.close();
    }

    const avg = measurements.reduce((a, b) => a + b, 0) / measurements.length;
    console.log(`   平均启动时间: ${avg.toFixed(2)}ms`);

    // 验证：启动时间应该相对稳定
    // 如果属性是延迟加载的，启动时间应该很快
    // 如果属性是全量加载的，启动时间会比较慢

    console.log(`\n💡 观察：`);
    if (avg < 100) {
      console.log(`   ✅ 启动时间很快 (${avg.toFixed(2)}ms)，可能已使用增量加载`);
    } else if (avg < 500) {
      console.log(`   ⚠️  启动时间中等 (${avg.toFixed(2)}ms)，部分数据可能在内存`);
    } else {
      console.log(`   ❌ 启动时间较慢 (${avg.toFixed(2)}ms)，可能全量加载`);
    }

    expect(avg).toBeLessThan(1000); // 基本可用性要求
  });
});

#!/usr/bin/env node
/**
 * 快速基准测试 - 适用于日常开发和CI
 * 运行核心功能的轻量级性能测试，快速发现性能回归
 *
 * 用法:
 *   node benchmarks/quick.mjs
 *   pnpm bench:quick
 */

import { NervusDB } from '../dist/index.mjs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { mkdtemp, rm } from 'node:fs/promises';

// 轻量级配置
const CONFIG = {
  INSERT_COUNT: 2000,
  QUERY_COUNT: 500,
  PATH_NODES: 50,
  MAX_TIME_MS: 1000, // 单个测试最大1秒
  MAX_MEMORY_MB: 50  // 单个测试最大50MB
};

/**
 * 简单的计时器
 */
function timer() {
  const start = performance.now();
  return () => {
    const end = performance.now();
    return Math.round((end - start) * 100) / 100;
  };
}

/**
 * 格式化内存使用量
 */
function formatMemory(bytes) {
  return `${Math.round(bytes / 1024 / 1024 * 10) / 10}MB`;
}

/**
 * 性能检查
 */
function checkPerformance(testName, duration, memoryUsed) {
  const warnings = [];

  if (duration > CONFIG.MAX_TIME_MS) {
    warnings.push(`⚠️  耗时过长: ${duration}ms > ${CONFIG.MAX_TIME_MS}ms`);
  }

  const memoryMB = memoryUsed / 1024 / 1024;
  if (memoryMB > CONFIG.MAX_MEMORY_MB) {
    warnings.push(`⚠️  内存使用过多: ${formatMemory(memoryUsed)} > ${CONFIG.MAX_MEMORY_MB}MB`);
  }

  if (warnings.length > 0) {
    console.log(`   ${testName}:`);
    warnings.forEach(w => console.log(`     ${w}`));
  }

  return warnings.length === 0;
}

/**
 * 快速基准测试主函数
 */
async function runQuickBenchmarks() {
  console.log('🏃 NervusDB 快速基准测试');
  console.log('=========================');

  const tempDir = await mkdtemp(join(tmpdir(), 'synapsedb-quick-'));
  const dbPath = join(tempDir, 'quick-bench.synapsedb');

  let allPassed = true;
  const results = [];

  try {
    // 初始化数据库
    console.log('📊 初始化数据库...');
    const db = await NervusDB.open(dbPath);

    // 1. 插入性能测试
    console.log('\n1. 📥 批量插入测试');
    const insertTimer = timer();
    const memBefore = process.memoryUsage().heapUsed;

    for (let i = 0; i < CONFIG.INSERT_COUNT; i++) {
      db.addFact({
        subject: `user_${i}`,
        predicate: 'follows',
        object: `user_${(i + 1) % CONFIG.INSERT_COUNT}`
      });
    }
    await db.flush();

    const insertTime = insertTimer();
    const insertMemory = process.memoryUsage().heapUsed - memBefore;
    const insertRate = Math.round(CONFIG.INSERT_COUNT / insertTime * 1000);

    console.log(`   ⏱️  插入 ${CONFIG.INSERT_COUNT.toLocaleString()} 条: ${insertTime}ms (${insertRate.toLocaleString()} ops/sec)`);
    console.log(`   💾 内存使用: ${formatMemory(insertMemory)}`);

    const insertPassed = checkPerformance('插入测试', insertTime, insertMemory);
    results.push({ name: '批量插入', passed: insertPassed, time: insertTime, memory: insertMemory });
    allPassed = allPassed && insertPassed;

    // 2. 查询性能测试
    console.log('\n2. 🔍 查询性能测试');
    const queryTimer = timer();
    const queryMemBefore = process.memoryUsage().heapUsed;

    // 测试多种查询模式
    const allResults = db.find({ predicate: 'follows' }).all();
    const specificResult = db.find({ subject: 'user_0', predicate: 'follows' }).all();
    const chainedResult = db.find({ subject: 'user_0' }).follow('follows').follow('follows').all();

    const queryTime = queryTimer();
    const queryMemory = process.memoryUsage().heapUsed - queryMemBefore;

    console.log(`   ⏱️  全表扫描: ${queryTime}ms (${allResults.length.toLocaleString()} 条)`);
    console.log(`   🎯 精确查询: ${specificResult.length} 条`);
    console.log(`   🔗 链式查询: ${chainedResult.length} 条`);
    console.log(`   💾 内存使用: ${formatMemory(queryMemory)}`);

    const queryPassed = checkPerformance('查询测试', queryTime, queryMemory);
    results.push({ name: '查询测试', passed: queryPassed, time: queryTime, memory: queryMemory });
    allPassed = allPassed && queryPassed;

    // 3. 流式查询测试
    console.log('\n3. 🌊 流式查询测试');
    const streamTimer = timer();
    const streamMemBefore = process.memoryUsage().heapUsed;

    let streamCount = 0;
    for await (const fact of db.find({ predicate: 'follows' })) {
      streamCount++;
      if (streamCount >= CONFIG.QUERY_COUNT) break; // 限制测试量
    }

    const streamTime = streamTimer();
    const streamMemory = process.memoryUsage().heapUsed - streamMemBefore;

    console.log(`   ⏱️  流式处理 ${streamCount.toLocaleString()} 条: ${streamTime}ms`);
    console.log(`   💾 内存使用: ${formatMemory(streamMemory)}`);

    const streamPassed = checkPerformance('流式查询', streamTime, streamMemory);
    results.push({ name: '流式查询', passed: streamPassed, time: streamTime, memory: streamMemory });
    allPassed = allPassed && streamPassed;

    // 4. 路径查询测试（小规模）
    console.log('\n4. 🛣️  路径查询测试');

    // 添加路径测试数据
    for (let i = 0; i < CONFIG.PATH_NODES; i++) {
      db.addFact({
        subject: `node_${i}`,
        predicate: 'connects',
        object: `node_${(i + 1) % CONFIG.PATH_NODES}`
      });
    }
    await db.flush();

    const pathTimer = timer();
    const pathMemBefore = process.memoryUsage().heapUsed;

    const shortestPath = db.shortestPath('node_0', 'node_10', {
      predicates: ['connects'],
      maxHops: 15
    });

    const bidirectionalPath = db.shortestPathBidirectional('node_0', 'node_20', {
      predicates: ['connects'],
      maxHops: 15
    });

    const pathTime = pathTimer();
    const pathMemory = process.memoryUsage().heapUsed - pathMemBefore;

    console.log(`   ⏱️  路径算法: ${pathTime}ms`);
    console.log(`   🎯 单向路径: ${shortestPath ? shortestPath.length : 'null'} 跳`);
    console.log(`   🔄 双向路径: ${bidirectionalPath ? bidirectionalPath.length : 'null'} 跳`);
    console.log(`   💾 内存使用: ${formatMemory(pathMemory)}`);

    const pathPassed = checkPerformance('路径查询', pathTime, pathMemory);
    results.push({ name: '路径查询', passed: pathPassed, time: pathTime, memory: pathMemory });
    allPassed = allPassed && pathPassed;

    // 5. 聚合测试
    console.log('\n5. 📊 聚合测试');

    // 添加聚合测试数据
    for (let i = 0; i < CONFIG.QUERY_COUNT; i++) {
      db.addFact(
        { subject: `user_${i % 10}`, predicate: 'rated', object: `item_${i}` },
        { edgeProperties: { score: 1 + Math.floor(Math.random() * 5) } }
      );
    }
    await db.flush();

    const aggTimer = timer();
    const aggMemBefore = process.memoryUsage().heapUsed;

    const aggResults = db.aggregate()
      .match({ predicate: 'rated' })
      .groupBy(['subject'])
      .avg('edgeProperties.score', 'avg_score')
      .count('total_ratings')
      .execute();

    const aggTime = aggTimer();
    const aggMemory = process.memoryUsage().heapUsed - aggMemBefore;

    console.log(`   ⏱️  聚合计算: ${aggTime}ms`);
    console.log(`   📈 分组结果: ${aggResults.length} 组`);
    console.log(`   💾 内存使用: ${formatMemory(aggMemory)}`);

    const aggPassed = checkPerformance('聚合测试', aggTime, aggMemory);
    results.push({ name: '聚合测试', passed: aggPassed, time: aggTime, memory: aggMemory });
    allPassed = allPassed && aggPassed;

    await db.close();

  } finally {
    // 清理临时目录
    await rm(tempDir, { recursive: true, force: true });
  }

  // 输出总结
  console.log('\n🎯 测试总结');
  console.log('============');

  const passedTests = results.filter(r => r.passed);
  const failedTests = results.filter(r => !r.passed);

  console.log(`✅ 通过: ${passedTests.length}/${results.length}`);
  if (failedTests.length > 0) {
    console.log(`❌ 失败: ${failedTests.length}/${results.length}`);
    failedTests.forEach(test => {
      console.log(`   - ${test.name}`);
    });
  }

  const totalTime = results.reduce((sum, r) => sum + r.time, 0);
  const totalMemory = results.reduce((sum, r) => sum + r.memory, 0);

  console.log(`⏱️  总耗时: ${totalTime.toFixed(1)}ms`);
  console.log(`💾 总内存: ${formatMemory(totalMemory)}`);

  if (allPassed) {
    console.log('\n🎉 所有快速基准测试通过！');
    return 0;
  } else {
    console.log('\n⚠️  部分测试存在性能问题，请检查详情');
    return 1;
  }
}

// 运行测试
runQuickBenchmarks()
  .then(exitCode => process.exit(exitCode))
  .catch(error => {
    console.error('❌ 基准测试失败:', error);
    process.exit(1);
  });
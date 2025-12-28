#!/usr/bin/env node
/**
 * NervusDB npm 包验证测试
 * 
 * 用途：验证从 npm 安装的 @nervusdb/core 包功能完整性
 * 运行：node examples/npm-package-test.mjs
 * 
 * 测试项：
 * - 数据库打开/关闭
 * - 添加三元组（带属性）
 * - 基础查询
 * - 属性查询
 * - 数据持久化
 */

import { NervusDB } from '@nervusdb/core';
import { tmpdir } from 'os';
import { join } from 'path';
import { rm } from 'fs/promises';

console.log('🧪 NervusDB npm 包功能验证测试\n');

const testDbPath = join(tmpdir(), `nervusdb-test-${Date.now()}.nervusdb`);
let testsPassed = 0;
let testsFailed = 0;

function assert(condition, message) {
  if (condition) {
    console.log(`  ✅ ${message}`);
    testsPassed++;
  } else {
    console.log(`  ❌ ${message}`);
    testsFailed++;
    throw new Error(`Assertion failed: ${message}`);
  }
}

try {
  // Test 1: 打开数据库
  console.log('📝 Test 1: 打开数据库');
  const db = await NervusDB.open(testDbPath, {
    enableLock: true,
    registerReader: true,
  });
  assert(db !== null, '数据库打开成功');

  // Test 2: 添加三元组（带属性）
  console.log('\n📝 Test 2: 添加三元组（带属性）');
  db.addFact(
    { subject: 'Alice', predicate: 'IS_A', object: 'Engineer' },
    {
      subjectProperties: { name: 'Alice', age: 30, city: 'SF' },
      objectProperties: { category: 'Job' },
    }
  );

  db.addFact(
    { subject: 'Bob', predicate: 'IS_A', object: 'Designer' },
    {
      subjectProperties: { name: 'Bob', age: 25, city: 'NY' },
    }
  );

  db.addFact(
    { subject: 'Charlie', predicate: 'IS_A', object: 'Manager' },
    {
      subjectProperties: { name: 'Charlie', age: 35, city: 'SF' },
    }
  );

  db.addFact(
    { subject: 'Alice', predicate: 'KNOWS', object: 'Bob' },
    { edgeProperties: { since: 2020, closeness: 8 } }
  );

  db.addFact(
    { subject: 'Bob', predicate: 'KNOWS', object: 'Charlie' },
    { edgeProperties: { since: 2021, closeness: 6 } }
  );

  db.addFact({ subject: 'Alice', predicate: 'REPORTS_TO', object: 'Charlie' });

  assert(true, '添加 6 个三元组成功');

  // Test 3: 基础查询
  console.log('\n📝 Test 3: 基础查询');
  const allFacts = db.listFacts();
  assert(allFacts.length === 6, `查询所有事实：${allFacts.length} 条`);

  const aliceFacts = db.find({ subject: 'Alice' }).all();
  assert(aliceFacts.length === 3, `查询 Alice 的事实：${aliceFacts.length} 条`);

  const knowsRelations = db.find({ predicate: 'KNOWS' }).all();
  assert(knowsRelations.length === 2, `查询 KNOWS 关系：${knowsRelations.length} 条`);

  // Test 4: 属性查询
  console.log('\n📝 Test 4: 属性查询');
  const sfPeople = db.findByNodeProperty({ propertyName: 'city', value: 'SF' }).all();
  assert(sfPeople.length >= 2, `查询 SF 的人：${sfPeople.length} 个`);

  const youngPeople = db
    .findByNodeProperty({
      propertyName: 'age',
      operator: '<',
      value: 30,
    })
    .all();
  assert(youngPeople.length >= 1, `查询年龄 < 30 的人：${youngPeople.length} 个`);

  // Test 5: 边属性查询（等值查询）
  console.log('\n📝 Test 5: 边属性查询');
  await db.flush(); // 确保属性索引已构建
  
  // 使用 whereProperty 方法查询边属性（边属性只支持等值查询）
  const strongRelations = db
    .find({ predicate: 'KNOWS' })
    .whereProperty('closeness', '=', 8, 'edge')  // 第4个参数指定为 'edge'，只支持 '=' 操作符
    .all();
  assert(strongRelations.length >= 1, `查询亲密度 = 8 的关系：${strongRelations.length} 条`);

  // Test 6: 数据持久化
  console.log('\n📝 Test 6: 数据持久化');
  await db.flush();
  assert(true, '数据刷新到磁盘成功');

  // Test 7: 关闭数据库
  console.log('\n📝 Test 7: 关闭数据库');
  await db.close();
  assert(true, '数据库关闭成功');

  // Test 8: 重新打开验证持久化
  console.log('\n📝 Test 8: 重新打开验证持久化');
  const db2 = await NervusDB.open(testDbPath);
  const factsAfterReopen = db2.listFacts();
  assert(factsAfterReopen.length === 6, `重新打开后数据完整：${factsAfterReopen.length} 条`);
  await db2.close();

  // 总结
  console.log('\n' + '='.repeat(60));
  console.log('✅ 测试总结');
  console.log('='.repeat(60));
  console.log(`✅ 通过: ${testsPassed}`);
  console.log(`❌ 失败: ${testsFailed}`);
  console.log('='.repeat(60));

  if (testsFailed === 0) {
    console.log('\n🎉 所有测试通过！@nervusdb/core 包功能完整！');
    process.exit(0);
  } else {
    console.log('\n❌ 部分测试失败');
    process.exit(1);
  }
} catch (error) {
  console.error('\n❌ 测试执行失败:', error.message);
  console.error(error.stack);
  process.exit(1);
} finally {
  // 清理测试数据库
  try {
    await rm(testDbPath, { recursive: true, force: true });
    await rm(`${testDbPath}.pages`, { recursive: true, force: true });
    await rm(`${testDbPath}.wal`, { force: true });
  } catch (e) {
    // 忽略清理错误
  }
}

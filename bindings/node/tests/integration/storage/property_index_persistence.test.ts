import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { NervusDB } from '@/synapseDb';

describe('属性索引持久化测试', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-property-persistence-'));
    dbPath = join(workspace, 'test.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('属性索引持久化后重启应该能正确工作', async () => {
    const db1 = await NervusDB.open(dbPath);

    // 插入测试数据
    console.log('🚀 开始插入数据...');
    const startTime = Date.now();

    // 先用少量简单数据测试
    for (let i = 0; i < 10; i++) {
      db1.addFact(
        {
          subject: `user${i}`,
          predicate: 'HAS_PROFILE',
          object: `profile${i}`,
        },
        {
          subjectProperties: {
            age: 25 + i, // 简单的递增年龄
          },
        },
      );
    }

    console.log(`✅ 插入完成，耗时: ${Date.now() - startTime}ms`);

    // 使用 whereProperty 验证属性索引工作（flush前）
    console.log('🧪 测试属性索引（flush前）...');
    const beforeFlushResults = db1
      .find({ predicate: 'HAS_PROFILE' })
      .whereProperty('age', '=', 25)
      .all();
    console.log(`   flush前 age=25 的用户数: ${beforeFlushResults.length}`);
    expect(beforeFlushResults.length).toBe(1);

    // flush 以持久化属性索引
    console.log('💾 持久化属性索引...');
    const flushStart = Date.now();
    await db1.flush();
    console.log(`✅ 持久化完成，耗时: ${Date.now() - flushStart}ms`);

    // 检查属性索引文件是否已创建
    const indexDir = dbPath + '.pages';
    const { readdir } = await import('node:fs/promises');
    let files: string[] = [];
    try {
      files = await readdir(indexDir);
    } catch (e) {
      // 目录可能不存在
    }

    const propertyFiles = files.filter((f) => f.startsWith('property-') && f.endsWith('.idx'));
    const manifestFile = files.find((f) => f === 'property-index.manifest.json');

    console.log(`📁 属性索引文件:`);
    console.log(`   - 清单文件: ${manifestFile ? '✅' : '❌'}`);
    console.log(`   - 索引文件: ${propertyFiles.length} 个`);

    expect(manifestFile).toBeDefined();
    expect(propertyFiles.length).toBeGreaterThan(0);

    await db1.close();

    // 重启数据库，测试属性索引加载
    console.log('🔄 重启数据库...');
    const reopenStart = Date.now();
    const db2 = await NervusDB.open(dbPath);
    console.log(`✅ 重启完成，耗时: ${Date.now() - reopenStart}ms`);

    // 使用 whereProperty 测试属性查询是否工作正常（持久化加载后）
    console.log('🧪 测试属性索引（重启后）...');
    const queryStart = Date.now();

    // 等值查询
    const age25Results = db2.find({ predicate: 'HAS_PROFILE' }).whereProperty('age', '=', 25).all();
    console.log(`   重启后 age=25 的用户数: ${age25Results.length}`);

    // 范围查询
    const ageRangeResults = db2
      .find({ predicate: 'HAS_PROFILE' })
      .whereProperty('age', '>=', 20)
      .whereProperty('age', '<=', 30)
      .all();
    console.log(`   重启后 age 20-30 的用户数: ${ageRangeResults.length}`);

    // 部门查询（注释掉因为没有相关数据）
    // const engResults = db2
    //   .find({ predicate: 'HAS_PROFILE' })
    //   .whereProperty('department', '=', 'Engineering')
    //   .all();
    // console.log(`   重启后 department=Engineering 的用户数: ${engResults.length}`);

    console.log(`✅ 查询完成，耗时: ${Date.now() - queryStart}ms`);

    // 验证结果正确性和一致性
    expect(age25Results.length).toBe(1);
    expect(ageRangeResults.length).toBeGreaterThan(5);
    // expect(engResults.length).toBeGreaterThan(0); // 注释掉因为没有相关数据

    // 验证持久化前后的结果一致
    expect(age25Results.length).toBe(beforeFlushResults.length);

    await db2.close();
  });

  it('属性索引应该能正确处理复杂类型的值', async () => {
    const db1 = await NervusDB.open(dbPath);

    // 插入包含复杂类型的属性
    db1.addFact(
      {
        subject: 'user1',
        predicate: 'HAS_PROFILE',
        object: 'profile1',
      },
      {
        subjectProperties: {
          tags: ['javascript', 'typescript', 'nodejs'],
          metadata: { level: 'senior', years: 5 },
          settings: { theme: 'dark', notifications: true },
        },
      },
    );

    db1.addFact(
      {
        subject: 'user2',
        predicate: 'HAS_PROFILE',
        object: 'profile2',
      },
      {
        subjectProperties: {
          tags: ['python', 'django'],
          metadata: { level: 'junior', years: 2 },
          settings: { theme: 'light', notifications: false },
        },
      },
    );

    // 使用 whereProperty 验证复杂类型被索引
    const seniorUsers = db1
      .find({ predicate: 'HAS_PROFILE' })
      .whereProperty('metadata', '=', { level: 'senior', years: 5 })
      .all();
    expect(seniorUsers).toHaveLength(1);
    expect(seniorUsers[0].subject).toBe('user1');

    await db1.flush();
    await db1.close();

    // 重启后验证复杂类型属性索引被持久化
    const db2 = await NervusDB.open(dbPath);

    // 使用 whereProperty 查询复杂类型
    const juniorUsers = db2
      .find({ predicate: 'HAS_PROFILE' })
      .whereProperty('metadata', '=', { level: 'junior', years: 2 })
      .all();

    expect(juniorUsers).toHaveLength(1);
    expect(juniorUsers[0].subject).toBe('user2');

    // 验证复杂类型的完整属性被正确存储
    const user1 = db2.find({ subject: 'user1' }).all()[0];
    const user2 = db2.find({ subject: 'user2' }).all()[0];

    expect(user1.subjectProperties?.tags).toEqual(['javascript', 'typescript', 'nodejs']);
    expect(user2.subjectProperties?.metadata).toEqual({ level: 'junior', years: 2 });

    await db2.close();
  });

  it('属性索引更新后持久化应该正确', async () => {
    const db1 = await NervusDB.open(dbPath);

    // 初始数据
    db1.addFact(
      {
        subject: 'user1',
        predicate: 'HAS_PROFILE',
        object: 'profile1',
      },
      {
        subjectProperties: { status: 'active', department: 'Engineering' },
      },
    );

    await db1.flush();

    // 更新属性
    db1.setNodeProperties('user1', {
      status: 'inactive',
      department: 'Marketing',
      level: 'senior',
    });

    await db1.flush();
    await db1.close();

    // 重启验证更新
    const db2 = await NervusDB.open(dbPath);
    const results = db2.find({ subject: 'user1' }).all();

    expect(results).toHaveLength(1);
    expect(results[0].subjectProperties).toEqual({
      status: 'inactive',
      department: 'Marketing',
      level: 'senior',
    });

    await db2.close();
  });
});

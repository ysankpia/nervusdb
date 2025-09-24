/**
 * TypeScript 类型系统增强测试
 * 验证类型安全的 SynapseDB API 和编译时类型检查
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  TypedSynapseDB,
  PersonNode,
  RelationshipEdge,
  EntityNode,
  KnowledgeEdge,
} from '../../src/index.js';
import { unlink } from 'fs/promises';

describe('TypedSynapseDB 类型安全测试', () => {
  let testDbPath: string;

  beforeEach(() => {
    testDbPath = `./test-typed-db-${Date.now()}-${Math.random()}.synapsedb`;
  });

  afterEach(async () => {
    try {
      await unlink(testDbPath);
    } catch {
      // 忽略文件不存在的错误
    }
  });

  it('应该支持带类型的数据库创建和基本操作', async () => {
    const db = await TypedSynapseDB.open<PersonNode, RelationshipEdge>(testDbPath);

    // 添加类型化的 fact
    const fact = db.addFact(
      { subject: 'Alice', predicate: 'FRIEND_OF', object: 'Bob' },
      {
        subjectProperties: { name: 'Alice', age: 30, labels: ['Person'] },
        objectProperties: { name: 'Bob', age: 25, labels: ['Person'] },
        edgeProperties: { since: new Date(), strength: 0.8, type: 'friend' },
      },
    );

    expect(fact.subjectProperties?.name).toBe('Alice');
    expect(fact.objectProperties?.name).toBe('Bob');
    expect(fact.edgeProperties?.type).toBe('friend');

    await db.close();
  });

  it('应该支持类型安全的属性查询', async () => {
    const db = await TypedSynapseDB.open<PersonNode, RelationshipEdge>(testDbPath);

    db.addFact(
      { subject: 'Alice', predicate: 'FRIEND_OF', object: 'Bob' },
      {
        subjectProperties: { name: 'Alice', age: 30 },
        objectProperties: { name: 'Bob', age: 25 },
        edgeProperties: { strength: 0.8 },
      },
    );

    // 按节点属性查询
    const results = db.findByNodeProperty({ propertyName: 'age', value: 30 }).all();
    expect(results).toHaveLength(1);
    expect(results[0]?.subjectProperties?.name).toBe('Alice');

    // 范围查询
    const ageResults = db
      .findByNodeProperty({
        propertyName: 'age',
        range: { min: 20, max: 35, includeMin: true, includeMax: true },
      })
      .all();
    expect(ageResults.length).toBeGreaterThan(0);

    await db.close();
  });

  it('应该支持类型安全的链式查询', async () => {
    const db = await TypedSynapseDB.open<PersonNode, RelationshipEdge>(testDbPath);

    db.addFact(
      { subject: 'Alice', predicate: 'FRIEND_OF', object: 'Bob' },
      {
        subjectProperties: { name: 'Alice', age: 30 },
        objectProperties: { name: 'Bob', age: 25 },
      },
    );

    db.addFact(
      { subject: 'Bob', predicate: 'FRIEND_OF', object: 'Charlie' },
      {
        subjectProperties: { name: 'Bob', age: 25 },
        objectProperties: { name: 'Charlie', age: 28 },
      },
    );

    // 首先验证基本查询工作
    const aliceFriends = db.find({ subject: 'Alice' }).all();
    expect(aliceFriends).toHaveLength(1);

    const bobFriends = db.find({ subject: 'Bob' }).all();
    expect(bobFriends).toHaveLength(1);

    // 链式查询：找到 Bob 的朋友（从 Alice -> Bob -> Charlie 的路径中的最后一跳）
    const friends = db
      .find({ subject: 'Bob' })
      .follow('FRIEND_OF')
      .where((record) => record.objectProperties?.name === 'Charlie')
      .all();

    expect(friends).toHaveLength(1);
    expect(friends[0]?.objectProperties?.name).toBe('Charlie');

    await db.close();
  });

  it('应该支持知识图谱类型', async () => {
    const db = await TypedSynapseDB.open<EntityNode, KnowledgeEdge>(testDbPath);

    const fact = db.addFact(
      { subject: 'Entity1', predicate: 'RELATED_TO', object: 'Entity2' },
      {
        subjectProperties: {
          type: 'Person',
          title: '张三',
          confidence: 0.95,
          labels: ['Person', 'Individual'],
        },
        objectProperties: {
          type: 'Organization',
          title: '某公司',
          confidence: 0.9,
          labels: ['Company'],
        },
        edgeProperties: {
          confidence: 0.85,
          source: 'knowledge_base',
          timestamp: Date.now(),
          weight: 0.7,
        },
      },
    );

    expect(fact.subjectProperties?.type).toBe('Person');
    expect(fact.objectProperties?.type).toBe('Organization');
    expect(fact.edgeProperties?.confidence).toBe(0.85);

    await db.close();
  });

  it('应该支持标签查询', async () => {
    const db = await TypedSynapseDB.open<PersonNode, RelationshipEdge>(testDbPath);

    db.addFact(
      { subject: 'Alice', predicate: 'WORKS_WITH', object: 'Bob' },
      {
        subjectProperties: { name: 'Alice', age: 30, labels: ['Person', 'Employee'] },
        objectProperties: { name: 'Bob', age: 25, labels: ['Person', 'Manager'] },
      },
    );

    // 单标签查询
    const persons = db.findByLabel('Person').all();
    expect(persons.length).toBeGreaterThan(0);

    // 多标签 AND 查询
    const employees = db.findByLabel(['Person', 'Employee'], { mode: 'AND' }).all();
    expect(employees).toHaveLength(1);
    expect(employees[0]?.subjectProperties?.name).toBe('Alice');

    // 多标签 OR 查询
    const workers = db.findByLabel(['Employee', 'Manager'], { mode: 'OR' }).all();
    expect(workers.length).toBeGreaterThan(0);

    await db.close();
  });

  it('应该支持属性直接访问', async () => {
    const db = await TypedSynapseDB.open<PersonNode, RelationshipEdge>(testDbPath);

    const fact = db.addFact(
      { subject: 'Alice', predicate: 'FRIEND_OF', object: 'Bob' },
      {
        subjectProperties: { name: 'Alice', age: 30 },
        objectProperties: { name: 'Bob', age: 25 },
        edgeProperties: { strength: 0.8 },
      },
    );

    // 直接通过 ID 获取属性
    const aliceProps = db.getNodeProperties(fact.subjectId);
    expect(aliceProps?.name).toBe('Alice');
    expect(aliceProps?.age).toBe(30);

    const bobProps = db.getNodeProperties(fact.objectId);
    expect(bobProps?.name).toBe('Bob');
    expect(bobProps?.age).toBe(25);

    const edgeProps = db.getEdgeProperties({
      subjectId: fact.subjectId,
      predicateId: fact.predicateId,
      objectId: fact.objectId,
    });
    expect(edgeProps?.strength).toBe(0.8);

    // 设置属性
    db.setNodeProperties(fact.subjectId, {
      name: 'Alice Smith',
      age: 31,
      email: 'alice@example.com',
    });
    const updatedProps = db.getNodeProperties(fact.subjectId);
    expect(updatedProps?.name).toBe('Alice Smith');
    expect(updatedProps?.age).toBe(31);
    expect(updatedProps?.email).toBe('alice@example.com');

    await db.close();
  });

  it('应该支持异步迭代器', async () => {
    const db = await TypedSynapseDB.open<PersonNode, RelationshipEdge>(testDbPath);

    // 添加多个 facts，确保每个都有 subject 属性
    db.addFact(
      { subject: 'Alice', predicate: 'FRIEND_OF', object: 'Bob' },
      {
        subjectProperties: { name: 'Alice', age: 30 },
        objectProperties: { name: 'Bob', age: 25 },
      },
    );
    db.addFact(
      { subject: 'Bob', predicate: 'FRIEND_OF', object: 'Charlie' },
      {
        subjectProperties: { name: 'Bob', age: 25 },
        objectProperties: { name: 'Charlie', age: 28 },
      },
    );
    db.addFact(
      { subject: 'Charlie', predicate: 'FRIEND_OF', object: 'Dave' },
      {
        subjectProperties: { name: 'Charlie', age: 28 },
        objectProperties: { name: 'Dave', age: 30 },
      },
    );

    const results: PersonNode[] = [];
    const query = db.find({ predicate: 'FRIEND_OF' });

    // 使用异步迭代器
    for await (const record of query) {
      if (record.subjectProperties) {
        results.push(record.subjectProperties);
      }
    }

    expect(results).toHaveLength(3);
    expect(results.map((p) => p.name)).toContain('Alice');
    expect(results.map((p) => p.name)).toContain('Bob');
    expect(results.map((p) => p.name)).toContain('Charlie');

    await db.close();
  });

  it('应该支持原始数据库访问', async () => {
    const db = await TypedSynapseDB.open<PersonNode, RelationshipEdge>(testDbPath);

    // 通过 raw 访问原始 API
    const rawDb = db.raw;
    expect(rawDb).toBeDefined();

    // 原始 API 应该仍然可用
    const fact = rawDb.addFact({ subject: 'Test', predicate: 'IS', object: 'Valid' });
    expect(fact.subject).toBe('Test');

    await db.close();
  });
});

describe('TypeSafeQueries 辅助函数测试', () => {
  let testDbPath: string;

  beforeEach(() => {
    testDbPath = `./test-queries-db-${Date.now()}-${Math.random()}.synapsedb`;
  });

  afterEach(async () => {
    try {
      await unlink(testDbPath);
    } catch {
      // 忽略文件不存在的错误
    }
  });

  it('应该支持类型安全的属性过滤器创建', async () => {
    const { TypeSafeQueries } = await import('../../src/index.js');

    // 精确值过滤器
    const nameFilter = TypeSafeQueries.propertyFilter('name', 'Alice');
    expect(nameFilter.propertyName).toBe('name');
    expect(nameFilter.value).toBe('Alice');

    // 范围过滤器
    const ageRange = TypeSafeQueries.rangeFilter('age', 20, 40, {
      includeMin: true,
      includeMax: false,
    });
    expect(ageRange.propertyName).toBe('age');
    expect(ageRange.range?.min).toBe(20);
    expect(ageRange.range?.max).toBe(40);
    expect(ageRange.range?.includeMin).toBe(true);
    expect(ageRange.range?.includeMax).toBe(false);

    const db = await TypedSynapseDB.open<PersonNode, RelationshipEdge>(testDbPath);

    db.addFact(
      { subject: 'Alice', predicate: 'HAS_AGE', object: '30' },
      { subjectProperties: { name: 'Alice', age: 30 } },
    );
    db.addFact(
      { subject: 'Bob', predicate: 'HAS_AGE', object: '35' },
      { subjectProperties: { name: 'Bob', age: 35 } },
    );

    // 使用过滤器查询
    const results = db.findByNodeProperty(ageRange).all();
    expect(results.length).toBeGreaterThan(0);

    await db.close();
  });
});

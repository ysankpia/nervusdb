import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { mkdtemp, rm } from 'fs/promises';
import { join } from 'path';
import { tmpdir } from 'os';

describe('属性索引下推查询', () => {
  let tempDir: string;
  let db: SynapseDB;

  beforeEach(async () => {
    tempDir = await mkdtemp(join(tmpdir(), 'synapse-property-pushdown-test-'));
    const dbPath = join(tempDir, 'test.synapsedb');
    db = await SynapseDB.open(dbPath);
  });

  afterEach(async () => {
    await db.close();
    await rm(tempDir, { recursive: true, force: true });
  });

  it('whereProperty 等值查询应该正确过滤', async () => {
    // 插入测试数据
    db.addFact(
      { subject: 'alice', predicate: 'IS_PERSON', object: 'true' },
      { subjectProperties: { age: 25, city: 'Beijing' } },
    );
    db.addFact(
      { subject: 'bob', predicate: 'IS_PERSON', object: 'true' },
      { subjectProperties: { age: 30, city: 'Shanghai' } },
    );
    db.addFact(
      { subject: 'charlie', predicate: 'IS_PERSON', object: 'true' },
      { subjectProperties: { age: 35, city: 'Beijing' } },
    );
    await db.flush();

    // 测试等值查询
    const beijingPeople = db
      .find({ predicate: 'IS_PERSON' })
      .whereProperty('city', '=', 'Beijing')
      .all();

    expect(beijingPeople).toHaveLength(2);
    const subjects = beijingPeople.map((r) => r.subject).sort();
    expect(subjects).toEqual(['alice', 'charlie']);
  });

  it('whereProperty 范围查询应该正确过滤', async () => {
    // 插入测试数据
    for (let i = 1; i <= 10; i++) {
      db.addFact(
        { subject: `user${i}`, predicate: 'HAS_SCORE', object: 'score' },
        { subjectProperties: { score: i * 10 } },
      );
    }
    await db.flush();

    // 测试 > 操作符
    const highScorers = db.find({ predicate: 'HAS_SCORE' }).whereProperty('score', '>', 70).all();
    expect(highScorers).toHaveLength(3); // 80, 90, 100

    // 测试 >= 操作符
    const midHighScorers = db
      .find({ predicate: 'HAS_SCORE' })
      .whereProperty('score', '>=', 70)
      .all();
    expect(midHighScorers).toHaveLength(4); // 70, 80, 90, 100

    // 测试 < 操作符
    const lowScorers = db.find({ predicate: 'HAS_SCORE' }).whereProperty('score', '<', 30).all();
    expect(lowScorers).toHaveLength(2); // 10, 20

    // 测试 <= 操作符
    const midLowScorers = db
      .find({ predicate: 'HAS_SCORE' })
      .whereProperty('score', '<=', 30)
      .all();
    expect(midLowScorers).toHaveLength(3); // 10, 20, 30
  });

  it('边属性索引下推应该正确工作', async () => {
    // 插入带边属性的数据
    db.addFact(
      { subject: 'alice', predicate: 'KNOWS', object: 'bob' },
      { edgeProperties: { since: '2020', strength: 'strong' } },
    );
    db.addFact(
      { subject: 'alice', predicate: 'KNOWS', object: 'charlie' },
      { edgeProperties: { since: '2021', strength: 'weak' } },
    );
    db.addFact(
      { subject: 'bob', predicate: 'KNOWS', object: 'charlie' },
      { edgeProperties: { since: '2020', strength: 'medium' } },
    );
    await db.flush();

    // 测试边属性等值查询
    const strongConnections = db
      .find({ predicate: 'KNOWS' })
      .whereProperty('strength', '=', 'strong', 'edge')
      .all();

    expect(strongConnections).toHaveLength(1);
    expect(strongConnections[0].subject).toBe('alice');
    expect(strongConnections[0].object).toBe('bob');

    // 测试多个边属性查询
    const since2020 = db
      .find({ predicate: 'KNOWS' })
      .whereProperty('since', '=', '2020', 'edge')
      .all();

    expect(since2020).toHaveLength(2);
  });

  it('whereProperty 与链式查询的组合', async () => {
    // 构建测试数据：用户 -> 项目 -> 标签
    db.addFact(
      { subject: 'alice', predicate: 'WORKS_ON', object: 'project1' },
      { subjectProperties: { level: 'senior' } },
    );
    db.addFact(
      { subject: 'bob', predicate: 'WORKS_ON', object: 'project2' },
      { subjectProperties: { level: 'junior' } },
    );
    db.addFact({ subject: 'project1', predicate: 'HAS_TAG', object: 'backend' });
    db.addFact({ subject: 'project2', predicate: 'HAS_TAG', object: 'frontend' });
    await db.flush();

    // 查询：senior 开发者 -> 项目 -> 标签
    const seniorTags = db
      .find({ predicate: 'WORKS_ON' })
      .whereProperty('level', '=', 'senior')
      .follow('HAS_TAG')
      .all();

    expect(seniorTags).toHaveLength(1);
    expect(seniorTags[0].object).toBe('backend');
  });

  it('属性索引下推性能测试', async () => {
    const startTime = Date.now();

    // 插入大量数据
    for (let i = 0; i < 1000; i++) {
      db.addFact(
        { subject: `user${i}`, predicate: 'HAS_PROFILE', object: 'profile' },
        {
          subjectProperties: {
            age: 20 + (i % 50), // 年龄 20-69
            status: i % 2 === 0 ? 'active' : 'inactive',
          },
        },
      );
    }
    await db.flush();

    const insertTime = Date.now() - startTime;

    // 测试普通 where 过滤性能
    const filterStart = Date.now();
    const normalResults = db
      .find({ predicate: 'HAS_PROFILE' })
      .where((r) => r.subjectProperties?.status === 'active')
      .all();
    const filterTime = Date.now() - filterStart;

    // 测试属性索引下推性能
    const pushdownStart = Date.now();
    const pushdownResults = db
      .find({ predicate: 'HAS_PROFILE' })
      .whereProperty('status', '=', 'active')
      .all();
    const pushdownTime = Date.now() - pushdownStart;

    // 验证结果正确性
    expect(normalResults).toHaveLength(500); // 一半是 active
    expect(pushdownResults).toHaveLength(500);

    // 属性索引下推应该显著更快
    console.log(`插入时间: ${insertTime}ms`);
    console.log(`普通过滤时间: ${filterTime}ms`);
    console.log(`索引下推时间: ${pushdownTime}ms`);
    console.log(`性能提升: ${Math.round(filterTime / pushdownTime)}x`);

    // 属性索引下推应该不慢于普通过滤（在小数据集上差异可能不明显）
    expect(pushdownTime).toBeLessThanOrEqual(filterTime * 2);
  });

  it('不存在的属性查询应该返回空结果', async () => {
    db.addFact(
      { subject: 'test', predicate: 'IS', object: 'valid' },
      { subjectProperties: { existing: 'value' } },
    );
    await db.flush();

    const results = db
      .find({ predicate: 'IS' })
      .whereProperty('nonexistent', '=', 'anything')
      .all();

    expect(results).toHaveLength(0);
  });

  it('边属性范围查询应该抛出错误', async () => {
    db.addFact(
      { subject: 'test', predicate: 'TEST', object: 'test' },
      { edgeProperties: { weight: 5 } },
    );
    await db.flush();

    expect(() => {
      db.find({ predicate: 'TEST' }).whereProperty('weight', '>', 3, 'edge');
    }).toThrow('边属性暂不支持范围查询操作符');
  });

  it('orientation 应该影响属性过滤的方向', async () => {
    // 插入测试数据
    db.addFact(
      { subject: 'source', predicate: 'POINTS_TO', object: 'target' },
      {
        subjectProperties: { type: 'pointer' },
        objectProperties: { type: 'data' },
      },
    );
    await db.flush();

    // 测试 subject orientation
    const subjectResults = db
      .find({ predicate: 'POINTS_TO' })
      .anchor('subject')
      .whereProperty('type', '=', 'pointer')
      .all();
    expect(subjectResults).toHaveLength(1);

    // 测试 object orientation
    const objectResults = db
      .find({ predicate: 'POINTS_TO' })
      .anchor('object')
      .whereProperty('type', '=', 'data')
      .all();
    expect(objectResults).toHaveLength(1);

    // 测试 both orientation
    const bothResults = db
      .find({ predicate: 'POINTS_TO' })
      .anchor('both')
      .whereProperty('type', '=', 'pointer')
      .all();
    expect(bothResults).toHaveLength(1);
  });
});

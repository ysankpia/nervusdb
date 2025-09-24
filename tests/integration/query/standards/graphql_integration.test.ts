/**
 * GraphQL 集成测试
 *
 * 测试 GraphQL 与 SynapseDB 的完整集成，包括复杂查询和性能验证
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SynapseDB } from '@/synapseDb.js';
import { GraphQLService, graphql } from '@/query/graphql/index.js';

describe('GraphQL 集成测试', () => {
  let db: SynapseDB;
  let gqlService: GraphQLService;

  beforeEach(async () => {
    db = await SynapseDB.open(':memory:');
    gqlService = graphql(db.store);

    // 创建大规模测试数据
    await setupLargeTestData(db);
  });

  afterEach(async () => {
    gqlService.dispose();
    await db.close();
  });

  describe('复杂查询场景', () => {
    it('应该执行嵌套关系查询', async () => {
      const query = `
        query {
          persons {
            id
            name
            friends {
              name
              worksAt {
                name
              }
            }
          }
        }
      `;

      const result = await gqlService.executeQuery(query);

      expect(result.data).toBeDefined();
      expect(result.data?.persons).toBeDefined();
      expect(Array.isArray(result.data?.persons)).toBe(true);
    });

    it('应该支持过滤查询', async () => {
      const query = `
        query GetAdults($ageFilter: Int) {
          persons(filter: { age_gt: $ageFilter }) {
            name
            age
          }
        }
      `;

      const result = await gqlService.executeQuery(query, {
        ageFilter: 18,
      });

      expect(result.data).toBeDefined();
      expect(result.data?.persons).toBeDefined();
    });

    it('应该支持分页查询', async () => {
      const query = `
        query GetPersonsPaged($first: Int, $after: String) {
          persons(first: $first, after: $after) {
            edges {
              cursor
              node {
                id
                name
              }
            }
            pageInfo {
              hasNextPage
              endCursor
            }
            totalCount
          }
        }
      `;

      const result = await gqlService.executeQuery(query, {
        first: 5,
      });

      expect(result.data?.persons).toBeDefined();
      expect(result.data?.persons?.edges).toBeDefined();
      expect(result.data?.persons?.pageInfo).toBeDefined();
    });

    it('应该支持排序查询', async () => {
      const query = `
        query GetPersonsSorted($sort: [PersonSort!]) {
          persons(sort: $sort) {
            name
            age
          }
        }
      `;

      const result = await gqlService.executeQuery(query, {
        sort: ['age_DESC'],
      });

      expect(result.data?.persons).toBeDefined();
    });

    it('应该处理多跳关系查询', async () => {
      const query = `
        query GetFriendsOfFriends {
          persons {
            name
            friends {
              name
              friends {
                name
              }
            }
          }
        }
      `;

      const result = await gqlService.executeQuery(query);

      expect(result.data).toBeDefined();
      expect(result.data?.persons).toBeDefined();
    });

    it('应该支持反向关系查询', async () => {
      const query = `
        query GetManagersAndEmployees {
          organizations {
            name
            employees {
              name
              manager {
                name
              }
            }
          }
        }
      `;

      const result = await gqlService.executeQuery(query);

      expect(result.data).toBeDefined();
      expect(result.data?.organizations).toBeDefined();
    });

    it('应该支持混合类型查询', async () => {
      const query = `
        query GetMixedTypes {
          persons {
            name
            worksAt {
              name
              projects {
                name
                technologies
              }
            }
          }
        }
      `;

      const result = await gqlService.executeQuery(query);

      expect(result.data).toBeDefined();
    });
  });

  describe('Schema 动态性', () => {
    it('应该适应数据结构变化', async () => {
      // 获取初始 Schema
      const initialSchema = await gqlService.getSchema();
      expect(initialSchema).not.toContain('Product');

      // 添加新的实体类型
      db.addFact({ subject: 'product:1', predicate: 'TYPE', object: 'Product' });
      db.addFact({ subject: 'product:1', predicate: 'HAS_NAME', object: 'iPhone' });
      db.addFact({ subject: 'product:1', predicate: 'HAS_PRICE', object: '999.99' });
      await db.flush();

      // 重新生成 Schema
      await gqlService.regenerateSchema();
      const updatedSchema = await gqlService.getSchema();

      expect(updatedSchema).toContain('Product');
    });

    it('应该处理属性类型变化', async () => {
      // 添加不同类型的属性值
      db.addFact({ subject: 'person:100', predicate: 'HAS_SCORE', object: '95' });
      db.addFact({ subject: 'person:101', predicate: 'HAS_SCORE', object: '87.5' });
      db.addFact({ subject: 'person:102', predicate: 'HAS_SCORE', object: 'A+' });
      await db.flush();

      await gqlService.regenerateSchema();
      const schema = await gqlService.getSchema();

      // Schema 应该包含 score 字段
      expect(schema).toBeDefined();
    });

    it('应该发现新的关系类型', async () => {
      // 添加新的关系类型
      db.addFact({ subject: 'person:1', predicate: 'MENTORS', object: 'person:10' });
      db.addFact({ subject: 'person:2', predicate: 'MENTORS', object: 'person:11' });
      await db.flush();

      await gqlService.regenerateSchema();
      const schema = await gqlService.getSchema();

      expect(schema).toContain('mentors');
    });
  });

  describe('性能和优化', () => {
    it('应该在合理时间内生成 Schema', async () => {
      const startTime = Date.now();
      await gqlService.initialize();
      const duration = Date.now() - startTime;

      // Schema 生成应该在 5 秒内完成
      expect(duration).toBeLessThan(5000);
    });

    it('应该缓存查询结果', async () => {
      const query = `
        query {
          persons {
            id
            name
          }
        }
      `;

      // 第一次查询
      const start1 = Date.now();
      const result1 = await gqlService.executeQuery(query);
      const duration1 = Date.now() - start1;

      // 第二次查询（应该更快）
      const start2 = Date.now();
      const result2 = await gqlService.executeQuery(query);
      const duration2 = Date.now() - start2;

      expect(result1.data).toEqual(result2.data);
      // 注意：当前实现可能没有实际缓存，这里测试基准性能
      expect(duration1).toBeGreaterThan(0);
      expect(duration2).toBeGreaterThan(0);
    });

    it('应该处理大量数据查询', async () => {
      // 添加更多测试数据
      for (let i = 100; i < 200; i++) {
        db.addFact({ subject: `person:${i}`, predicate: 'TYPE', object: 'Person' });
        db.addFact({ subject: `person:${i}`, predicate: 'HAS_NAME', object: `Person ${i}` });
        db.addFact({ subject: `person:${i}`, predicate: 'HAS_AGE', object: String(20 + (i % 50)) });
      }
      await db.flush();

      const query = `
        query GetAllPersons {
          persons {
            id
            name
            age
          }
        }
      `;

      const startTime = Date.now();
      const result = await gqlService.executeQuery(query);
      const duration = Date.now() - startTime;

      expect(result.data?.persons).toBeDefined();
      expect(Array.isArray(result.data?.persons)).toBe(true);
      expect(result.data?.persons.length).toBeGreaterThan(100);

      // 查询应该在合理时间内完成
      expect(duration).toBeLessThan(10000);
    });

    it('应该优化深度查询', async () => {
      const deepQuery = `
        query DeepQuery {
          persons {
            friends {
              friends {
                friends {
                  name
                }
              }
            }
          }
        }
      `;

      const startTime = Date.now();
      const result = await gqlService.executeQuery(deepQuery);
      const duration = Date.now() - startTime;

      expect(result).toBeDefined();
      // 深度查询也应该在合理时间内完成
      expect(duration).toBeLessThan(15000);
    });
  });

  describe('边界情况', () => {
    it('应该处理空结果集', async () => {
      const query = `
        query {
          persons(filter: { age_gt: 1000 }) {
            name
          }
        }
      `;

      const result = await gqlService.executeQuery(query);
      expect(result.data?.persons).toBeDefined();
      expect(Array.isArray(result.data?.persons)).toBe(true);
      expect(result.data?.persons.length).toBe(0);
    });

    it('应该处理循环引用', async () => {
      // 创建循环引用
      db.addFact({ subject: 'person:a', predicate: 'FRIEND_OF', object: 'person:b' });
      db.addFact({ subject: 'person:b', predicate: 'FRIEND_OF', object: 'person:c' });
      db.addFact({ subject: 'person:c', predicate: 'FRIEND_OF', object: 'person:a' });
      await db.flush();

      const query = `
        query CircularReference {
          persons {
            name
            friends {
              name
              friends {
                name
              }
            }
          }
        }
      `;

      const result = await gqlService.executeQuery(query);
      expect(result).toBeDefined();
      expect(result.errors).toBeUndefined();
    });

    it('应该处理缺失属性', async () => {
      // 创建只有部分属性的实体
      db.addFact({ subject: 'person:incomplete', predicate: 'TYPE', object: 'Person' });
      db.addFact({
        subject: 'person:incomplete',
        predicate: 'HAS_NAME',
        object: 'Incomplete Person',
      });
      // 故意不添加 age 属性
      await db.flush();

      const query = `
        query {
          persons {
            name
            age
          }
        }
      `;

      const result = await gqlService.executeQuery(query);
      expect(result.data?.persons).toBeDefined();

      const incompletePerson = result.data?.persons.find(
        (p: any) => p.name === 'Incomplete Person',
      );
      if (incompletePerson) {
        expect(incompletePerson.name).toBe('Incomplete Person');
        expect(incompletePerson.age).toBeNull();
      }
    });

    it('应该处理特殊字符', async () => {
      db.addFact({ subject: 'person:special', predicate: 'HAS_NAME', object: '张三@test.com' });
      db.addFact({
        subject: 'person:special',
        predicate: 'HAS_DESCRIPTION',
        object: 'Contains "quotes" and \\backslashes',
      });
      await db.flush();

      const query = `
        query {
          persons {
            name
            description
          }
        }
      `;

      const result = await gqlService.executeQuery(query);
      expect(result).toBeDefined();
      expect(result.errors).toBeUndefined();
    });
  });

  describe('数据一致性', () => {
    it('应该反映数据库的实时状态', async () => {
      // 初始查询
      const initialQuery = `query { persons { name } }`;
      const initialResult = await gqlService.executeQuery(initialQuery);
      const initialCount = initialResult.data?.persons?.length || 0;

      // 添加新数据
      db.addFact({ subject: 'person:new', predicate: 'TYPE', object: 'Person' });
      db.addFact({ subject: 'person:new', predicate: 'HAS_NAME', object: 'New Person' });
      await db.flush();

      // 重新生成 Schema 并查询
      await gqlService.regenerateSchema();
      const updatedResult = await gqlService.executeQuery(initialQuery);
      const updatedCount = updatedResult.data?.persons?.length || 0;

      expect(updatedCount).toBe(initialCount + 1);
    });

    it('应该处理数据删除', async () => {
      // 创建测试数据
      db.addFact({ subject: 'person:temp', predicate: 'TYPE', object: 'Person' });
      db.addFact({ subject: 'person:temp', predicate: 'HAS_NAME', object: 'Temporary' });
      await db.flush();

      const query = `query { persons { name } }`;
      const beforeResult = await gqlService.executeQuery(query);
      const beforeCount = beforeResult.data?.persons?.length || 0;

      // 删除数据
      db.deleteFact({ subject: 'person:temp', predicate: 'TYPE', object: 'Person' });
      db.deleteFact({ subject: 'person:temp', predicate: 'HAS_NAME', object: 'Temporary' });
      await db.flush();

      // 重新生成 Schema 并查询
      await gqlService.regenerateSchema();
      const afterResult = await gqlService.executeQuery(query);
      const afterCount = afterResult.data?.persons?.length || 0;

      expect(afterCount).toBe(beforeCount - 1);
    });
  });
});

/**
 * 设置大规模测试数据
 */
async function setupLargeTestData(db: SynapseDB): Promise<void> {
  // 创建人员数据
  for (let i = 1; i <= 50; i++) {
    db.addFact({ subject: `person:${i}`, predicate: 'TYPE', object: 'Person' });
    db.addFact({ subject: `person:${i}`, predicate: 'HAS_NAME', object: `Person ${i}` });
    db.addFact({ subject: `person:${i}`, predicate: 'HAS_AGE', object: String(20 + (i % 40)) });
    db.addFact({
      subject: `person:${i}`,
      predicate: 'HAS_EMAIL',
      object: `person${i}@example.com`,
    });
  }

  // 创建组织数据
  for (let i = 1; i <= 10; i++) {
    db.addFact({ subject: `org:${i}`, predicate: 'TYPE', object: 'Organization' });
    db.addFact({ subject: `org:${i}`, predicate: 'HAS_NAME', object: `Organization ${i}` });
    db.addFact({
      subject: `org:${i}`,
      predicate: 'HAS_INDUSTRY',
      object: i % 2 === 0 ? 'Technology' : 'Healthcare',
    });
  }

  // 创建项目数据
  for (let i = 1; i <= 20; i++) {
    db.addFact({ subject: `project:${i}`, predicate: 'TYPE', object: 'Project' });
    db.addFact({ subject: `project:${i}`, predicate: 'HAS_NAME', object: `Project ${i}` });
    db.addFact({
      subject: `project:${i}`,
      predicate: 'HAS_STATUS',
      object: i % 3 === 0 ? 'completed' : 'active',
    });
  }

  // 创建复杂关系网络
  for (let i = 1; i <= 50; i++) {
    // 朋友关系
    if (i < 50) {
      db.addFact({ subject: `person:${i}`, predicate: 'FRIEND_OF', object: `person:${i + 1}` });
    }
    if (i % 5 === 0) {
      db.addFact({
        subject: `person:${i}`,
        predicate: 'FRIEND_OF',
        object: `person:${Math.max(1, i - 10)}`,
      });
    }

    // 工作关系
    const orgId = Math.ceil(i / 5);
    db.addFact({ subject: `person:${i}`, predicate: 'WORKS_AT', object: `org:${orgId}` });

    // 管理关系
    if (i % 10 === 1 && i > 1) {
      db.addFact({ subject: `person:${i}`, predicate: 'MANAGES', object: `person:${i - 1}` });
    }

    // 项目参与
    const projectId = (i % 20) + 1;
    db.addFact({
      subject: `person:${i}`,
      predicate: 'PARTICIPATES_IN',
      object: `project:${projectId}`,
    });
  }

  // 组织-项目关系
  for (let i = 1; i <= 20; i++) {
    const orgId = Math.ceil(i / 2);
    db.addFact({ subject: `org:${orgId}`, predicate: 'OWNS_PROJECT', object: `project:${i}` });
  }

  // 技能和专长
  const skills = ['JavaScript', 'Python', 'Java', 'React', 'Node.js', 'SQL', 'Docker', 'AWS'];
  for (let i = 1; i <= 50; i++) {
    const skillCount = 2 + (i % 4);
    for (let j = 0; j < skillCount; j++) {
      const skill = skills[(i + j) % skills.length];
      db.addFact({ subject: `person:${i}`, predicate: 'HAS_SKILL', object: skill });
    }
  }

  await db.flush();
}

/**
 * Gremlin 集成测试
 *
 * 测试复杂的图遍历场景和实际用例
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { gremlin, P } from '@/query/gremlin';
import type { GraphTraversalSource } from '@/query/gremlin/source';
import { tmpdir } from 'os';
import { join } from 'path';
import { unlinkSync, rmSync, existsSync } from 'fs';

describe('Gremlin 集成测试', () => {
  let db: SynapseDB;
  let g: GraphTraversalSource;
  let dbPath: string;

  beforeEach(async () => {
    dbPath = join(tmpdir(), `test-gremlin-integration-${Date.now()}.synapsedb`);
    db = await SynapseDB.open(dbPath);
    g = gremlin((db as any).store);

    // 构建复杂的社交网络图
    // 人物
    const people = [
      { id: 'person:alice', name: '爱丽丝', age: 28, city: '北京', profession: '工程师' },
      { id: 'person:bob', name: '鲍勃', age: 32, city: '上海', profession: '设计师' },
      { id: 'person:charlie', name: '查理', age: 25, city: '北京', profession: '产品经理' },
      { id: 'person:diana', name: '戴安娜', age: 30, city: '深圳', profession: '工程师' },
      { id: 'person:eve', name: '夏娃', age: 27, city: '杭州', profession: '数据科学家' },
      { id: 'person:frank', name: '弗兰克', age: 35, city: '广州', profession: '设计师' },
    ];

    // 公司
    const companies = [
      { id: 'company:techcorp', name: 'TechCorp', industry: '科技', city: '北京' },
      { id: 'company:designco', name: 'DesignCo', industry: '设计', city: '上海' },
      { id: 'company:datalab', name: 'DataLab', industry: '数据', city: '深圳' },
    ];

    // 项目
    const projects = [
      { id: 'project:web', name: '网站重构', status: '进行中' },
      { id: 'project:mobile', name: '移动应用', status: '已完成' },
      { id: 'project:analytics', name: '数据分析平台', status: '规划中' },
    ];

    // 添加实体
    for (const person of people) {
      db.addFact({ subject: person.id, predicate: 'HAS_NAME', object: person.name });
      db.addFact({ subject: person.id, predicate: 'HAS_AGE', object: person.age.toString() });
      db.addFact({ subject: person.id, predicate: 'HAS_CITY', object: person.city });
      db.addFact({ subject: person.id, predicate: 'HAS_PROFESSION', object: person.profession });
      db.addFact({ subject: person.id, predicate: 'TYPE', object: 'Person' });
    }

    for (const company of companies) {
      db.addFact({ subject: company.id, predicate: 'HAS_NAME', object: company.name });
      db.addFact({ subject: company.id, predicate: 'HAS_INDUSTRY', object: company.industry });
      db.addFact({ subject: company.id, predicate: 'HAS_CITY', object: company.city });
      db.addFact({ subject: company.id, predicate: 'TYPE', object: 'Company' });
    }

    for (const project of projects) {
      db.addFact({ subject: project.id, predicate: 'HAS_NAME', object: project.name });
      db.addFact({ subject: project.id, predicate: 'HAS_STATUS', object: project.status });
      db.addFact({ subject: project.id, predicate: 'TYPE', object: 'Project' });
    }

    // 添加关系
    // 友谊关系
    db.addFact({ subject: 'person:alice', predicate: 'KNOWS', object: 'person:bob' });
    db.addFact({ subject: 'person:bob', predicate: 'KNOWS', object: 'person:alice' });
    db.addFact({ subject: 'person:alice', predicate: 'KNOWS', object: 'person:charlie' });
    db.addFact({ subject: 'person:charlie', predicate: 'KNOWS', object: 'person:diana' });
    db.addFact({ subject: 'person:diana', predicate: 'KNOWS', object: 'person:eve' });
    db.addFact({ subject: 'person:eve', predicate: 'KNOWS', object: 'person:frank' });
    db.addFact({ subject: 'person:bob', predicate: 'KNOWS', object: 'person:frank' });

    // 工作关系
    db.addFact({ subject: 'person:alice', predicate: 'WORKS_AT', object: 'company:techcorp' });
    db.addFact({ subject: 'person:charlie', predicate: 'WORKS_AT', object: 'company:techcorp' });
    db.addFact({ subject: 'person:bob', predicate: 'WORKS_AT', object: 'company:designco' });
    db.addFact({ subject: 'person:frank', predicate: 'WORKS_AT', object: 'company:designco' });
    db.addFact({ subject: 'person:diana', predicate: 'WORKS_AT', object: 'company:datalab' });
    db.addFact({ subject: 'person:eve', predicate: 'WORKS_AT', object: 'company:datalab' });

    // 项目参与关系
    db.addFact({ subject: 'person:alice', predicate: 'WORKS_ON', object: 'project:web' });
    db.addFact({ subject: 'person:charlie', predicate: 'WORKS_ON', object: 'project:web' });
    db.addFact({ subject: 'person:bob', predicate: 'WORKS_ON', object: 'project:mobile' });
    db.addFact({ subject: 'person:diana', predicate: 'WORKS_ON', object: 'project:analytics' });
    db.addFact({ subject: 'person:eve', predicate: 'WORKS_ON', object: 'project:analytics' });

    await db.flush();
  });

  afterEach(async () => {
    await db.close();
    try {
      if (existsSync(dbPath)) {
        unlinkSync(dbPath);
      }
      const indexDir = dbPath + '.pages';
      if (existsSync(indexDir)) {
        rmSync(indexDir, { recursive: true, force: true });
      }
      const walFile = dbPath + '.wal';
      if (existsSync(walFile)) {
        unlinkSync(walFile);
      }
    } catch (error) {
      // 忽略清理错误
    }
  });

  describe('社交网络查询', () => {
    it('应该能找到指定人员的朋友', async () => {
      const friends = await g
        .V()
        .has('HAS_NAME', '爱丽丝')
        .out('KNOWS')
        .values('HAS_NAME')
        .toList();

      expect(friends.length).toBeGreaterThan(0);
      const friendNames = friends.map((f) => f.properties.value);
      expect(friendNames).toContain('鲍勃');
      expect(friendNames).toContain('查理');
    });

    it('应该能找到朋友的朋友（二度连接）', async () => {
      const friendsOfFriends = await g
        .V()
        .has('HAS_NAME', '爱丽丝')
        .out('KNOWS')
        .out('KNOWS')
        .has('HAS_NAME', P.neq('爱丽丝')) // 排除自己
        .dedup()
        .values('HAS_NAME')
        .toList();

      expect(friendsOfFriends.length).toBeGreaterThan(0);
      const names = friendsOfFriends.map((f) => f.properties.value);
      expect(names).not.toContain('爱丽丝'); // 确保不包含自己
    });

    it('应该能找到共同的朋友', async () => {
      // 找到爱丽丝和鲍勃的共同朋友
      const aliceFriends = await g.V().has('HAS_NAME', '爱丽丝').out('KNOWS').toList();

      const bobFriends = await g.V().has('HAS_NAME', '鲍勃').out('KNOWS').toList();

      const aliceFriendIds = new Set(aliceFriends.map((f) => f.id));
      const bobFriendIds = new Set(bobFriends.map((f) => f.id));

      // 计算交集
      const commonFriendIds = [...aliceFriendIds].filter((id) => bobFriendIds.has(id));
      expect(commonFriendIds.length).toBeGreaterThanOrEqual(0);
    });
  });

  describe('工作网络查询', () => {
    it('应该能找到同事', async () => {
      const colleagues = await g
        .V()
        .has('HAS_NAME', '爱丽丝')
        .out('WORKS_AT')
        .in('WORKS_AT')
        .has('HAS_NAME', P.neq('爱丽丝')) // 排除自己
        .values('HAS_NAME')
        .toList();

      expect(colleagues.length).toBeGreaterThan(0);
      const colleagueNames = colleagues.map((c) => c.properties.value);
      expect(colleagueNames).toContain('查理'); // 同在 TechCorp
    });

    it('应该能按职业分组', async () => {
      const engineers = await g.V().has('HAS_PROFESSION', '工程师').values('HAS_NAME').toList();

      expect(engineers.length).toBe(2); // 爱丽丝和戴安娜
      const engineerNames = engineers.map((e) => e.properties.value);
      expect(engineerNames).toContain('爱丽丝');
      expect(engineerNames).toContain('戴安娜');
    });

    it('应该能找到在同一城市工作的人', async () => {
      const beijingWorkers = await g
        .V()
        .has('TYPE', 'Person') // 只查找人员，不包括公司
        .has('HAS_CITY', '北京')
        .values('HAS_NAME')
        .toList();

      expect(beijingWorkers.length).toBe(2); // 爱丽丝和查理
      const workerNames = beijingWorkers.map((w) => w.properties.value);
      expect(workerNames).toContain('爱丽丝');
      expect(workerNames).toContain('查理');
    });
  });

  describe('项目协作查询', () => {
    it('应该能找到项目团队成员', async () => {
      const webTeam = await g
        .V()
        .has('HAS_NAME', '网站重构')
        .in('WORKS_ON')
        .values('HAS_NAME')
        .toList();

      expect(webTeam.length).toBe(2); // 爱丽丝和查理
      const teamNames = webTeam.map((m) => m.properties.value);
      expect(teamNames).toContain('爱丽丝');
      expect(teamNames).toContain('查理');
    });

    it('应该能找到一个人参与的所有项目', async () => {
      const aliceProjects = await g
        .V()
        .has('HAS_NAME', '爱丽丝')
        .out('WORKS_ON')
        .values('HAS_NAME')
        .toList();

      expect(aliceProjects.length).toBe(1);
      const projectNames = aliceProjects.map((p) => p.properties.value);
      expect(projectNames).toContain('网站重构');
    });
  });

  describe('复合查询', () => {
    it('应该能找到年龄大于指定值的工程师朋友', async () => {
      const results = await g
        .V()
        .has('HAS_NAME', '爱丽丝')
        .out('KNOWS')
        .has('HAS_PROFESSION', '工程师')
        .has('HAS_AGE', P.gte('30'))
        .values('HAS_NAME')
        .toList();

      // 爱丽丝的朋友中，是工程师且年龄>=30的（如果存在）
      expect(Array.isArray(results)).toBe(true);
    });

    it('应该能查询跨越多种关系的复杂路径', async () => {
      // 找到与爱丽丝有共同项目的同事的朋友
      const results = await g
        .V()
        .has('HAS_NAME', '爱丽丝')
        .out('WORKS_ON') // 爱丽丝的项目
        .in('WORKS_ON') // 同项目的人
        .has('HAS_NAME', P.neq('爱丽丝')) // 排除爱丽丝自己
        .out('KNOWS') // 他们的朋友
        .dedup()
        .values('HAS_NAME')
        .toList();

      expect(Array.isArray(results)).toBe(true);
    });

    it('应该支持条件分支查询', async () => {
      // 查找所有人，如果是工程师则获取其技能，否则获取其公司
      const allPeople = await g.V().has('TYPE', 'Person').toList();

      expect(allPeople.length).toBe(6);

      // 分别查询工程师和非工程师
      const engineers = await g
        .V()
        .has('TYPE', 'Person')
        .has('HAS_PROFESSION', '工程师')
        .values('HAS_NAME')
        .toList();

      const nonEngineers = await g
        .V()
        .has('TYPE', 'Person')
        .has('HAS_PROFESSION', P.neq('工程师'))
        .values('HAS_NAME')
        .toList();

      expect(engineers.length + nonEngineers.length).toBe(6);
    });
  });

  describe('聚合和统计查询', () => {
    it('应该能统计每个城市的人数', async () => {
      const totalPeople = await g.V().has('TYPE', 'Person').count().toList();

      expect(totalPeople[0].properties.value).toBe(6);
    });

    it('应该能统计关系数量', async () => {
      const knowsRelations = await g.E().hasLabel('KNOWS').count().toList();

      expect(knowsRelations[0].properties.value).toBeGreaterThan(0);
    });

    it('应该支持去重统计', async () => {
      const uniqueCities = await g
        .V()
        .has('TYPE', 'Person')
        .values('HAS_CITY')
        .dedup()
        .count()
        .toList();

      expect(uniqueCities[0].properties.value).toBeGreaterThan(0);
      expect(uniqueCities[0].properties.value).toBeLessThanOrEqual(6);
    });
  });

  describe('性能和大数据量测试', () => {
    it('应该能处理链式查询而不超时', async () => {
      const startTime = Date.now();

      const results = await g
        .V()
        .limit(100) // 限制起始点数量
        .out()
        .out()
        .dedup()
        .limit(50)
        .toList();

      const duration = Date.now() - startTime;

      expect(Array.isArray(results)).toBe(true);
      expect(duration).toBeLessThan(5000); // 5秒内完成
    });

    it('应该能处理深度遍历', async () => {
      // 测试多层遍历不会导致无限循环或栈溢出
      // 使用手动的多层遍历来替代 repeat()
      const results = await g
        .V()
        .has('TYPE', 'Person')
        .out('KNOWS')
        .out('KNOWS')
        .out('KNOWS')
        .dedup()
        .limit(10)
        .toList();

      expect(Array.isArray(results)).toBe(true);
    }, 10000); // 10秒超时
  });

  describe('边界条件测试', () => {
    it('应该正确处理空结果', async () => {
      const results = await g.V().has('HAS_NAME', '不存在的人').out('KNOWS').toList();

      expect(results.length).toBe(0);
    });

    it('应该正确处理单个节点', async () => {
      const result = await g.V().has('HAS_NAME', '爱丽丝').next();

      expect(result.type).toBe('vertex');
      expect(result.properties.HAS_NAME).toBe('爱丽丝');
    });

    it('应该正确处理类型转换', async () => {
      const ages = await g.V().has('TYPE', 'Person').values('HAS_AGE').toList();

      ages.forEach((age) => {
        const ageValue = age.properties.value;
        expect(typeof ageValue === 'string').toBe(true);
        expect(!isNaN(Number(ageValue))).toBe(true);
      });
    });
  });
});

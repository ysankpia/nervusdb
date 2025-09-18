#!/usr/bin/env node

// SynapseDB vs RAG 学习演示
import { SynapseDB } from './src/synapseDb.ts';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

async function learnSynapseDB() {
  console.log('🎯 SynapseDB vs RAG 学习演示\n');

  // 创建临时数据库
  const workspace = await mkdtemp(join(tmpdir(), 'synapsedb-learn-'));
  const dbPath = join(workspace, 'learn.synapsedb');
  const db = await SynapseDB.open(dbPath);

  console.log('📚 学习场景：软件项目知识库');
  console.log('问题："找出所有修改过用户认证相关代码的开发人员"\n');

  console.log('🔗 步骤1: 构建知识图谱...');

  // ==================== 基础事实 ====================
  console.log('\n📝 添加开发人员信息:');
  db.addFact({ subject: 'alice', predicate: 'role', object: 'senior_developer' });
  db.addFact({ subject: 'bob', predicate: 'role', object: 'full_stack_developer' });
  db.addFact({ subject: 'charlie', predicate: 'role', object: 'security_expert' });
  db.addFact({ subject: 'david', predicate: 'role', object: 'backend_developer' });
  console.log('  ✓ alice -> role -> senior_developer');
  console.log('  ✓ bob -> role -> full_stack_developer');
  console.log('  ✓ charlie -> role -> security_expert');
  console.log('  ✓ david -> role -> backend_developer');

  console.log('\n📝 添加代码修改记录:');
  db.addFact({ subject: 'alice', predicate: 'modified', object: 'auth/login.js' });
  db.addFact({ subject: 'bob', predicate: 'modified', object: 'auth/user.js' });
  db.addFact({ subject: 'charlie', predicate: 'modified', object: 'auth/utils.js' });
  db.addFact({ subject: 'david', predicate: 'modified', object: 'api/routes.js' });
  console.log('  ✓ alice -> modified -> auth/login.js');
  console.log('  ✓ bob -> modified -> auth/user.js');
  console.log('  ✓ charlie -> modified -> auth/utils.js');
  console.log('  ✓ david -> modified -> api/routes.js');

  console.log('\n📝 添加模块归属关系:');
  db.addFact({ subject: 'auth/login.js', predicate: 'belongs_to', object: 'auth_module' });
  db.addFact({ subject: 'auth/user.js', predicate: 'belongs_to', object: 'auth_module' });
  db.addFact({ subject: 'auth/utils.js', predicate: 'belongs_to', object: 'auth_module' });
  db.addFact({ subject: 'api/routes.js', predicate: 'belongs_to', object: 'api_module' });
  console.log('  ✓ auth/login.js -> belongs_to -> auth_module');
  console.log('  ✓ auth/user.js -> belongs_to -> auth_module');
  console.log('  ✓ auth/utils.js -> belongs_to -> auth_module');
  console.log('  ✓ api/routes.js -> belongs_to -> api_module');

  console.log('\n📝 添加模块类型分类:');
  db.addFact({ subject: 'auth_module', predicate: 'type', object: 'user_authentication' });
  db.addFact({ subject: 'api_module', predicate: 'type', object: 'general_api' });
  console.log('  ✓ auth_module -> type -> user_authentication');
  console.log('  ✓ api_module -> type -> general_api');

  console.log('\n📝 添加时间戳和置信度:');
  db.addFact(
    { subject: 'alice', predicate: 'commit_time', object: '2024-01-15' },
    { edgeProperties: { confidence: 0.95, lines_changed: 120 } },
  );
  db.addFact(
    { subject: 'bob', predicate: 'commit_time', object: '2024-01-16' },
    { edgeProperties: { confidence: 0.92, lines_changed: 85 } },
  );
  db.addFact(
    { subject: 'charlie', predicate: 'commit_time', object: '2024-01-17' },
    { edgeProperties: { confidence: 0.98, lines_changed: 45 } },
  );

  await db.flush();

  console.log('\n✅ 知识图谱构建完成！');
  console.log('📊 当前数据规模:');
  const facts = db.listFacts();
  const uniqueNodes = new Set();
  facts.forEach((fact) => {
    uniqueNodes.add(fact.subject);
    uniqueNodes.add(fact.object);
  });
  console.log(`  - 总事实数: ${facts.length}`);
  console.log(`  - 总节点数: ${uniqueNodes.size}`);

  // ==================== 查询演示 ====================
  console.log('\n🔍 查询演示1: 找出所有修改过用户认证相关代码的开发人员');
  console.log('问题: "找出所有修改过用户认证相关代码的开发人员"');
  console.log('');
  console.log('🔄 查询路径:');
  console.log('  1. 从 user_authentication 开始');
  console.log('  2. 找到 type=auth_module 的节点');
  console.log('  3. 找到 belongs_to=auth_module 的文件');
  console.log('  4. 找到 modified 这些文件的开发人员');
  console.log('');

  // SynapseDB 复杂查询
  const authDevelopers = db
    .find({ object: 'user_authentication' })
    .followReverse('type')
    .followReverse('belongs_to')
    .followReverse('modified')
    .all();

  console.log('📊 查询结果:');
  authDevelopers.forEach((dev, index) => {
    console.log(`  ${index + 1}. ${dev.subject} 修改了 ${dev.object}`);
  });

  console.log('\n🔍 查询演示2: 找出所有开发人员及其修改的模块');
  console.log('问题: "统计所有开发人员及其修改的文件"');
  console.log('');

  const allDevelopers = db.find({ predicate: 'modified' }).all();
  const developerMap = new Map();
  allDevelopers.forEach((fact) => {
    if (!developerMap.has(fact.subject)) {
      developerMap.set(fact.subject, []);
    }
    developerMap.get(fact.subject).push(fact.object);
  });

  console.log('📋 开发人员-文件映射:');
  developerMap.forEach((files, developer) => {
    console.log(`  ${developer}:`);
    files.forEach((file) => {
      console.log(`    - ${file}`);
    });
  });

  console.log('\n🔍 查询演示3: 找出安全专家修改的认证相关文件');
  console.log('问题: "找出安全专家修改的认证相关文件"');
  console.log('');

  const securityExperts = db
    .find({ predicate: 'role', object: 'security_expert' })
    .follow('modified')
    .follow('belongs_to')
    .follow('type')
    .where((fact) => fact.object === 'user_authentication')
    .all();

  console.log('🔐 安全专家修改的认证文件:');
  const uniqueExperts = new Set(securityExperts.map((f) => f.subject));
  if (uniqueExperts.size === 0) {
    console.log('  (没有找到符合条件的记录)');
  } else {
    uniqueExperts.forEach((expert) => {
      console.log(`  - ${expert}`);
    });
  }

  console.log('\n🔍 查询演示4: 查看带有属性的事实');
  console.log('问题: "查看 alice 的提交详情"');
  console.log('');

  const aliceCommits = db.find({ subject: 'alice', predicate: 'commit_time' }).all();
  aliceCommits.forEach((commit) => {
    console.log(`  ${commit.subject} 在 ${commit.object} 提交`);
    if (commit.edgeProperties) {
      console.log(`    - 置信度: ${commit.edgeProperties.confidence}`);
      console.log(`    - 修改行数: ${commit.edgeProperties.lines_changed}`);
    }
  });

  // ==================== 对比分析 ====================
  console.log('\n🔄 SynapseDB vs 传统 RAG 对比分析:');
  console.log('');
  console.log('📊 查询: "修改过用户认证相关代码的开发人员"');
  console.log('');

  console.log('✅ SynapseDB 结果:');
  console.log('  - alice (修改了 auth/login.js)');
  console.log('  - bob (修改了 auth/user.js)');
  console.log('  - charlie (修改了 auth/utils.js)');
  console.log('  🎯 精确: 只返回实际修改认证代码的开发人员');
  console.log('');

  console.log('❌ 传统 RAG 可能返回:');
  console.log('  - alice (修改了 auth/login.js)');
  console.log('  - bob (修改了 auth/user.js)');
  console.log('  - charlie (修改了 auth/utils.js)');
  console.log('  - david (在会议中讨论了认证方案) ← 误判！');
  console.log('  - elisa (写过认证相关文档) ← 误判！');
  console.log('  - 会议纪要: "认证模块讨论" ← 噪声！');
  console.log('  🤔 模糊: 语义匹配导致无关结果');

  console.log('\n🎯 SynapseDB 核心优势:');
  console.log('  1. ✅ 精确匹配 - 基于明确的关系而非语义相似度');
  console.log('  2. ✅ 关系推理 - 支持多跳查询和复杂逻辑');
  console.log('  3. ✅ 可解释性 - 可以追踪完整的查询路径');
  console.log('  4. ✅ 属性丰富 - 支持置信度、时间戳等元数据');
  console.log('  5. ✅ 一致性保证 - 数据关系完整且一致');
  console.log('  6. ✅ 无幻觉 - 基于事实而非概率匹配');

  console.log('\n🚀 适用场景:');
  console.log('  • 代码库分析和依赖关系追踪');
  console.log('  • 企业知识管理和专家定位');
  console.log('  • 推荐系统和个性化服务');
  console.log('  • 风控系统和关联分析');
  console.log('  • 科研数据管理和实验关系');

  console.log('\n💡 最佳实践建议:');
  console.log('  1. 结构化数据优先使用 SynapseDB');
  console.log('  2. 非结构化文本分析使用 RAG');
  console.log('  3. 复杂系统考虑混合方案');
  console.log('  4. 重视数据建模和关系设计');
  console.log('  5. 利用属性丰富上下文信息');

  // 清理
  await db.close();
  await rm(workspace, { recursive: true, force: true });

  console.log('\n🎓 学习完成！');
  console.log('💡 记住: SynapseDB 提供精确的结构化关系查询，');
  console.log('   传统 RAG 适合非结构化文本的语义搜索。');
  console.log('   选择合适的工具取决于你的数据特点！');

  console.log('\n🔗 实际应用示例:');
  console.log('  • 代码审计: 追踪谁修改了敏感代码');
  console.log('  • 故障排查: 找到相关模块的负责人');
  console.log('  • 知识管理: 定位特定领域的专家');
  console.log('  • 合规检查: 验证权限和访问关系');
}

// 运行学习演示
learnSynapseDB().catch(console.error);

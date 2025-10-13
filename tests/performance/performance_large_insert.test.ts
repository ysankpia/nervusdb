import { describe, it, expect } from 'vitest';
import { NervusDB } from '../../src/index.js';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { mkdtemp, rm } from 'node:fs/promises';

describe('性能大规模插入', () => {
  it('插入 100k 记录的端到端性能测试', async () => {
    const testDir = await mkdtemp(join(tmpdir(), 'nervusdb-large-insert-'));
    const dbPath = join(testDir, 'large.nervusdb');

    try {
      // 1. 打开数据库
      const db = await NervusDB.open(dbPath, {
        rebuildIndexes: true,
      });

      const recordCount = 100_000;
      console.log(`\n🚀 开始插入 ${recordCount.toLocaleString()} 条记录...`);

      const startTime = performance.now();

      // 2. 批量插入
      db.beginBatch();

      for (let i = 0; i < recordCount; i++) {
        const userId = `user${i}`;
        const age = 20 + (i % 60);
        const score = Math.floor(Math.random() * 1000);

        db.addFact({
          subject: userId,
          predicate: 'hasAge',
          object: `${age}`,
        });

        db.addFact({
          subject: userId,
          predicate: 'hasScore',
          object: `${score}`,
        });

        // 进度显示
        if ((i + 1) % 10000 === 0) {
          console.log(`   已插入 ${(i + 1).toLocaleString()} 条记录...`);
        }
      }

      // 3. 提交批处理
      db.commitBatch();
      await db.flush();

      const endTime = performance.now();
      const elapsed = endTime - startTime;
      const throughput = (recordCount * 2) / (elapsed / 1000); // 每条记录2个fact

      console.log(`\n✅ 插入完成！`);
      console.log(`   总记录数: ${recordCount.toLocaleString()} records`);
      console.log(`   总事实数: ${(recordCount * 2).toLocaleString()} facts`);
      console.log(`   总耗时: ${elapsed.toFixed(2)}ms (${(elapsed / 1000).toFixed(2)}s)`);
      console.log(`   吞吐量: ${throughput.toFixed(0)} facts/sec`);
      console.log(`   平均延迟: ${(elapsed / recordCount).toFixed(3)}ms/record`);

      // 4. 验证查询性能
      console.log(`\n🔍 测试查询性能...`);

      const queryStart = performance.now();
      const results = db.find({ predicate: 'hasAge', object: '25' }).all();
      const queryEnd = performance.now();

      console.log(`   查询 age=25 的用户: ${results.length} 条`);
      console.log(`   查询耗时: ${(queryEnd - queryStart).toFixed(2)}ms`);

      // 5. 性能断言
      expect(elapsed).toBeLessThan(60_000); // 应在 60s 内完成
      expect(throughput).toBeGreaterThan(1000); // 至少 1K facts/sec
      expect(results.length).toBeGreaterThan(0); // 查询应该有结果

      await db.close();

      console.log(`\n🎉 大规模性能测试通过！\n`);
    } finally {
      await rm(testDir, { recursive: true, force: true });
    }
  }, 120_000); // 120s timeout
});

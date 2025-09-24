import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';
import { autoCompact } from '@/maintenance/autoCompact';
import { garbageCollectPages } from '@/maintenance/gc';
import { compactDatabase } from '@/maintenance/compaction';

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

describe('查询快照隔离（withSnapshot）', () => {
  const FAST = process.env.FAST === '1' || process.env.FAST === 'true';
  const scale = FAST ? 0.25 : 1; // 快速模式缩短等待时间
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-snapshot-'));
    dbPath = join(workspace, 'snap.synapsedb');
  });

  afterEach(async () => {
    // 多次尝试强制清理，处理可能的竞态条件
    for (let attempts = 0; attempts < 3; attempts++) {
      try {
        await rm(workspace, { recursive: true, force: true });
        break; // 成功清理，退出循环
      } catch (error: any) {
        if (attempts === 2) {
          // 最后一次尝试，记录错误但继续
          console.warn(`Warning: Failed to clean workspace after 3 attempts: ${error.message}`);
        } else {
          // 等待一段时间后重试
          await new Promise((resolve) => setTimeout(resolve, 100));
        }
      }
    }
  });

  it('长查询期间 epoch 固定，后台 compaction 不影响链式结果', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 2 });
    // 构造多页：S->R->O1..O5
    for (let i = 1; i <= 5; i += 1) {
      db.addFact({ subject: 'S', predicate: 'R', object: `O${i}` });
    }
    await db.flush();

    const p = db.withSnapshot(async (snap) => {
      const q1 = snap.find({ subject: 'S', predicate: 'R' });
      // 等待 1.2s，确保 manifest 可能推进 epoch（FAST 缩短）
      await sleep(1200 * scale);
      const q2 = q1.follow('R');
      const all = q2.all();
      expect(all.map((x) => x.object).sort()).toEqual(['O1', 'O2', 'O3', 'O4', 'O5']);
    });

    // 并发后台增量合并 + 自动 GC
    const c = autoCompact(dbPath, {
      mode: 'incremental',
      orders: ['SPO'],
      minMergePages: 2,
      autoGC: true,
    });
    await Promise.all([p, c]);
  });

  it('链式查询期间独立 GC 操作不影响结果', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 4 });

    // 创建多个主题，每个有多个关联
    for (let i = 1; i <= 3; i++) {
      for (let j = 1; j <= 4; j++) {
        db.addFact({ subject: `Subject${i}`, predicate: 'hasChild', object: `Child${i}-${j}` });
        db.addFact({ subject: `Child${i}-${j}`, predicate: 'hasValue', object: `Value${i}-${j}` });
      }
    }
    await db.flush();

    // 先进行一次压缩，产生一些孤儿页面
    await compactDatabase(dbPath, { mode: 'incremental', orders: ['SPO'], minMergePages: 2 });

    const queryPromise = db.withSnapshot(async (snap) => {
      // 给异步reader注册一些时间完成（FAST 缩短）
      await sleep(50 * scale);

      const results = snap.find({ predicate: 'hasChild' }).follow('hasValue').all();

      // 在查询中途等待（FAST 缩短）
      await sleep(800 * scale);

      return results;
    });

    // 并发执行 GC
    const gcPromise = (async () => {
      await sleep(200 * scale); // 确保查询已开始（FAST 缩短）
      return garbageCollectPages(dbPath, { respectReaders: true });
    })();

    const [queryResults, gcStats] = await Promise.all([queryPromise, gcPromise]);

    // 验证查询结果完整性
    expect(queryResults).toHaveLength(12); // 3个主题 * 4个子项
    const values = queryResults.map((r) => r.object).sort();
    for (let i = 1; i <= 3; i++) {
      for (let j = 1; j <= 4; j++) {
        expect(values).toContain(`Value${i}-${j}`);
      }
    }

    await db.close();
  });

  it('多重嵌套链式查询与增量压缩并发', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 3 });

    // 创建复杂的关联结构：A -> B -> C -> D
    for (let i = 1; i <= 10; i++) {
      db.addFact({ subject: `A${i}`, predicate: 'linksTo', object: `B${i}` });
      db.addFact({ subject: `B${i}`, predicate: 'linksTo', object: `C${i}` });
      db.addFact({ subject: `C${i}`, predicate: 'linksTo', object: `D${i}` });
    }
    await db.flush();

    const longQueryPromise = db.withSnapshot(async (snap) => {
      // 执行复杂的链式查询
      const step1 = snap.find({ subject: 'A1' });
      await sleep(300 * scale);

      const step2 = step1.follow('linksTo');
      await sleep(300 * scale);

      const step3 = step2.follow('linksTo');
      await sleep(300 * scale);

      const step4 = step3.follow('linksTo');
      return step4.all();
    });

    // 在查询期间进行多次增量压缩
    const compactionPromise = (async () => {
      await sleep(100 * scale);
      await autoCompact(dbPath, {
        mode: 'incremental',
        orders: ['SPO', 'POS'],
        minMergePages: 2,
        respectReaders: true,
      });

      await sleep(200 * scale);
      await autoCompact(dbPath, {
        mode: 'incremental',
        orders: ['OSP'],
        minMergePages: 2,
        respectReaders: true,
      });
    })();

    const [queryResults] = await Promise.all([longQueryPromise, compactionPromise]);

    // 验证最终结果
    expect(queryResults).toHaveLength(1);
    expect(queryResults[0].object).toBe('D1');

    await db.close();
  }, 15000);

  it('并发读写与维护任务的隔离性', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 5 });

    // 初始数据
    for (let i = 1; i <= 15; i++) {
      db.addFact({ subject: `Entity${i}`, predicate: 'type', object: 'TestEntity' });
      db.addFact({ subject: `Entity${i}`, predicate: 'value', object: `${i * 10}` });
    }
    await db.flush();

    // 多个并发查询 - 避免竞态条件，确保每个快照都完全独立
    const queries = [];
    for (let q = 1; q <= 3; q++) {
      queries.push(
        db.withSnapshot(async (snap) => {
          // 确保快照完全建立后再进行查询
          await sleep(50 * scale); // 给读者注册一些时间

          const entities = snap.find({ predicate: 'type', object: 'TestEntity' });
          // 立即实体化，确保数据在当前 epoch 被固定
          const entityList = entities.all();
          expect(entityList).toHaveLength(15); // 确保初始数据正确

          await sleep((300 + q * 50) * scale); // 错开时间避免冲突（FAST 缩短）

          const withValues = entities.follow('value');
          return withValues.all();
        }),
      );
    }

    // 并发维护任务
    const maintenancePromise = (async () => {
      // 最小稳定化：略微延后维护启动，降低与查询初始种子构建的竞争
      await sleep(300 * scale);

      // 连续的维护操作
      await autoCompact(dbPath, {
        mode: 'incremental',
        respectReaders: true,
      });

      await sleep(100 * scale);

      await garbageCollectPages(dbPath, {
        respectReaders: true,
      });
    })();

    const [result1, result2, result3] = await Promise.all([...queries, maintenancePromise]);

    // 所有查询应该返回相同的结果
    expect(result1).toHaveLength(15);
    expect(result2).toHaveLength(15);
    expect(result3).toHaveLength(15);

    // 验证结果一致性
    const values1 = result1.map((r) => r.object).sort();
    const values2 = result2.map((r) => r.object).sort();
    const values3 = result3.map((r) => r.object).sort();

    expect(values1).toEqual(values2);
    expect(values2).toEqual(values3);

    await db.close();
  });

  it('快照期间新写入不影响当前查询', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 4 });

    // 初始数据
    for (let i = 1; i <= 8; i++) {
      db.addFact({ subject: 'Root', predicate: 'connects', object: `Node${i}` });
    }
    await db.flush();

    const snapshotQueryPromise = db.withSnapshot(async (snap) => {
      const initial = snap.find({ subject: 'Root', predicate: 'connects' });

      // 查询中途等待
      await sleep(600 * scale);

      return initial.all();
    });

    // 在快照查询期间添加新数据并进行维护
    const writeAndMaintenancePromise = (async () => {
      await sleep(200 * scale);

      // 添加新数据（不应影响快照）
      for (let i = 9; i <= 12; i++) {
        db.addFact({ subject: 'Root', predicate: 'connects', object: `Node${i}` });
      }
      await db.flush();

      // 执行压缩
      await autoCompact(dbPath, {
        mode: 'incremental',
        respectReaders: true,
      });
    })();

    const [snapshotResults] = await Promise.all([snapshotQueryPromise, writeAndMaintenancePromise]);

    // 快照应该只看到初始数据
    expect(snapshotResults).toHaveLength(8);

    // 验证快照外的查询能看到新数据
    const currentResults = db.find({ subject: 'Root', predicate: 'connects' }).all();
    expect(currentResults).toHaveLength(12);

    await db.close();
  });
});

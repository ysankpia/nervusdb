import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { NervusDB } from '@/synapseDb';
import { compactDatabase } from '@/maintenance/compaction';
import { garbageCollectPages } from '@/maintenance/gc';
import { setCrashPoint } from '@/utils/fault';

describe('崩溃注入（flush 路径）', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-crash-'));
    dbPath = join(workspace, 'crash.synapsedb');
  });

  afterEach(async () => {
    // 强制清理readers目录中的所有文件，确保完全清理
    try {
      const readersDir = join(dbPath + '.pages', 'readers');
      // 重试清理逻辑，处理可能的竞态条件
      for (let attempt = 0; attempt < 5; attempt++) {
        try {
          const files = await readdir(readersDir);
          for (const file of files) {
            try {
              await unlink(join(readersDir, file));
            } catch {
              // 忽略删除失败
            }
          }
          await rmdir(readersDir);
          break; // 成功清理，退出重试循环
        } catch (err: any) {
          if (err?.code === 'ENOTEMPTY' && attempt < 4) {
            // 目录不为空，等待一下再重试
            await new Promise((resolve) => setTimeout(resolve, 50 * (attempt + 1)));
            continue;
          }
          // 其他错误或最后一次尝试失败，忽略
          break;
        }
      }
    } catch {
      // 忽略所有清理错误
    }

    setCrashPoint(null);
    await rm(workspace, { recursive: true, force: true });
  });

  it('before-incremental-write: flush 增量写入中断但 WAL 可恢复', async () => {
    const db1 = await NervusDB.open(dbPath);
    db1.addFact({ subject: 'S', predicate: 'R', object: 'O' });
    setCrashPoint('before-incremental-write');
    await expect(db1.flush()).rejects.toThrow(/InjectedCrash:before-incremental-write/);

    const db2 = await NervusDB.open(dbPath);
    const facts = db2.find({ subject: 'S', predicate: 'R' }).all();
    expect(facts.length).toBeGreaterThanOrEqual(1);
  });

  it('before-page-append: 主文件已写入，索引增量未写，仍可读取（可能走 staging）', async () => {
    const db1 = await NervusDB.open(dbPath);
    db1.addFact({ subject: 'S', predicate: 'R', object: 'O' });
    setCrashPoint('before-page-append');
    await expect(db1.flush()).rejects.toThrow(/InjectedCrash:before-page-append/);

    const db2 = await NervusDB.open(dbPath);
    const facts = db2.find({ subject: 'S', predicate: 'R' }).all();
    expect(facts.length).toBeGreaterThanOrEqual(1);
  });

  it('before-manifest-write: manifest 未写但主数据持久，重启后可读', async () => {
    const db1 = await NervusDB.open(dbPath);
    db1.addFact({ subject: 'A', predicate: 'R', object: 'B' });
    setCrashPoint('before-manifest-write');
    await expect(db1.flush()).rejects.toThrow(/InjectedCrash:before-manifest-write/);

    const db2 = await NervusDB.open(dbPath);
    const facts = db2.find({ subject: 'A', predicate: 'R' }).all();
    expect(facts.length).toBeGreaterThanOrEqual(1);
  });

  it('before-wal-reset: WAL 尚未 reset，重启后不会重复可见（去重保障）', async () => {
    const db1 = await NervusDB.open(dbPath);
    db1.addFact({ subject: 'X', predicate: 'R', object: 'Y' });
    setCrashPoint('before-wal-reset');
    await expect(db1.flush()).rejects.toThrow(/InjectedCrash:before-wal-reset/);

    const db2 = await NervusDB.open(dbPath);
    const facts = db2.find({ subject: 'X', predicate: 'R' }).all();
    // 去重后不应出现重复
    expect(facts.length).toBe(1);
  });
});

describe('崩溃注入（维护工具）', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-maint-crash-'));
    dbPath = join(workspace, 'maint.synapsedb');
  });

  afterEach(async () => {
    setCrashPoint(null);
    await rm(workspace, { recursive: true, force: true });
  });

  async function prepareDatabase(): Promise<void> {
    const db = await NervusDB.open(dbPath);
    for (let i = 0; i < 10; i++) {
      db.addFact({
        subject: `S${i}`,
        predicate: 'R',
        object: `O${i}`,
      });
    }
    await db.flush();
    await db.close();
  }

  it('compaction.beforeRename 崩溃后仍可恢复', async () => {
    await prepareDatabase();
    setCrashPoint('compaction.beforeRename');
    await expect(
      compactDatabase(dbPath, {
        orders: ['SPO'],
        minMergePages: 1,
        dryRun: false,
        mode: 'rewrite',
      }),
    ).rejects.toThrow(/InjectedCrash:compaction.beforeRename/);

    setCrashPoint(null);
    const reopened = await NervusDB.open(dbPath);
    const facts = reopened.find({ predicate: 'R' }).all();
    expect(facts.length).toBeGreaterThan(0);
    await reopened.close();
  });

  it('gc.beforeRename 崩溃后数据仍一致', async () => {
    await prepareDatabase();
    setCrashPoint('gc.beforeRename');
    await expect(garbageCollectPages(dbPath, { dryRun: false })).rejects.toThrow(
      /InjectedCrash:gc.beforeRename/,
    );

    setCrashPoint(null);
    const stats = await garbageCollectPages(dbPath, { dryRun: true });
    expect(stats.dryRun).toBe(true);
    expect(stats.orders.length).toBeGreaterThan(0);
  });
});

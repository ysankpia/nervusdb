import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';
import { setCrashPoint } from '@/utils/fault';

describe('崩溃注入（flush 路径）', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-crash-'));
    dbPath = join(workspace, 'crash.synapsedb');
  });

  afterEach(async () => {
    setCrashPoint(null);
    await rm(workspace, { recursive: true, force: true });
  });

  it('before-main-write: flush 中断但 WAL 可恢复', async () => {
    const db1 = await SynapseDB.open(dbPath);
    db1.addFact({ subject: 'S', predicate: 'R', object: 'O' });
    setCrashPoint('before-main-write');
    await expect(db1.flush()).rejects.toThrow(/InjectedCrash:before-main-write/);

    const db2 = await SynapseDB.open(dbPath);
    const facts = db2.find({ subject: 'S', predicate: 'R' }).all();
    expect(facts.length).toBeGreaterThanOrEqual(1);
  });

  it('before-page-append: 主文件已写入，索引增量未写，仍可读取（可能走 staging）', async () => {
    const db1 = await SynapseDB.open(dbPath);
    db1.addFact({ subject: 'S', predicate: 'R', object: 'O' });
    setCrashPoint('before-page-append');
    await expect(db1.flush()).rejects.toThrow(/InjectedCrash:before-page-append/);

    const db2 = await SynapseDB.open(dbPath);
    const facts = db2.find({ subject: 'S', predicate: 'R' }).all();
    expect(facts.length).toBeGreaterThanOrEqual(1);
  });

  it('before-manifest-write: manifest 未写但主数据持久，重启后可读', async () => {
    const db1 = await SynapseDB.open(dbPath);
    db1.addFact({ subject: 'A', predicate: 'R', object: 'B' });
    setCrashPoint('before-manifest-write');
    await expect(db1.flush()).rejects.toThrow(/InjectedCrash:before-manifest-write/);

    const db2 = await SynapseDB.open(dbPath);
    const facts = db2.find({ subject: 'A', predicate: 'R' }).all();
    expect(facts.length).toBeGreaterThanOrEqual(1);
  });

  it('before-wal-reset: WAL 尚未 reset，重启后不会重复可见（去重保障）', async () => {
    const db1 = await SynapseDB.open(dbPath);
    db1.addFact({ subject: 'X', predicate: 'R', object: 'Y' });
    setCrashPoint('before-wal-reset');
    await expect(db1.flush()).rejects.toThrow(/InjectedCrash:before-wal-reset/);

    const db2 = await SynapseDB.open(dbPath);
    const facts = db2.find({ subject: 'X', predicate: 'R' }).all();
    // 去重后不应出现重复
    expect(facts.length).toBe(1);
  });
});

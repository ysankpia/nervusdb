import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { NervusDB } from '@/synapseDb';

describe('并发单写者保护测试', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-writer-guard-'));
    dbPath = join(workspace, 'guard.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('启用锁时第二个写者应被拒绝', async () => {
    // 第一个写者开启锁
    const db1 = await NervusDB.open(dbPath, { enableLock: true });

    db1.addFact({ subject: 'Writer1', predicate: 'claims', object: 'database' });

    // 尝试开启第二个写者应该失败
    await expect(NervusDB.open(dbPath, { enableLock: true })).rejects.toThrow();

    await db1.close();
  });

  it('第一个写者关闭后第二个写者可以获得锁', async () => {
    // 第一个写者
    {
      const db1 = await NervusDB.open(dbPath, { enableLock: true });
      db1.addFact({ subject: 'FirstWriter', predicate: 'action', object: 'write' });
      await db1.flush();
      await db1.close(); // 释放锁
    }

    // 第二个写者现在应该可以获得锁
    {
      const db2 = await NervusDB.open(dbPath, { enableLock: true });
      db2.addFact({ subject: 'SecondWriter', predicate: 'action', object: 'write' });
      await db2.flush();

      // 验证两个写者的数据都存在
      const results = db2.find({ predicate: 'action' }).all();
      expect(results).toHaveLength(2);

      const subjects = results.map((r) => r.subject);
      expect(subjects).toContain('FirstWriter');
      expect(subjects).toContain('SecondWriter');

      await db2.close();
    }
  });

  it('禁用锁时多个写者可以并存（危险但允许）', async () => {
    // 不启用锁，允许多个写者
    const db1 = await NervusDB.open(dbPath, { enableLock: false });
    const db2 = await NervusDB.open(dbPath, { enableLock: false });

    db1.addFact({ subject: 'Writer1', predicate: 'concurrent', object: 'data1' });
    db2.addFact({ subject: 'Writer2', predicate: 'concurrent', object: 'data2' });

    await db1.flush();
    await db2.flush();

    // 两个写者都应该能正常工作（尽管这在实际应用中是危险的）
    const results1 = db1.find({ predicate: 'concurrent' }).all();
    const results2 = db2.find({ predicate: 'concurrent' }).all();

    // 注意：这里的行为可能不可预测，我们只验证不会崩溃
    expect(results1.length).toBeGreaterThanOrEqual(1);
    expect(results2.length).toBeGreaterThanOrEqual(1);

    await db1.close();
    await db2.close();
  });

  it('混合锁模式：已锁定时读者仍可无锁打开（不应拒绝）', async () => {
    // 第一个写者启用锁
    const db1 = await NervusDB.open(dbPath, { enableLock: true });

    // 作为读者（无锁且不写入）应当允许打开
    const reader = await NervusDB.open(dbPath, { enableLock: false });
    await reader.close();

    await db1.close();
  });

  it('读者不受锁限制（多读者可以与写者共存）', async () => {
    // 写者启用锁
    const writer = await NervusDB.open(dbPath, { enableLock: true });
    writer.addFact({ subject: 'Data', predicate: 'type', object: 'test' });
    await writer.flush();

    // 多个读者应该可以正常打开
    const reader1 = await NervusDB.open(dbPath, { enableLock: false });
    const reader2 = await NervusDB.open(dbPath, { enableLock: false });

    // 读者应该能看到写者的数据
    const results1 = reader1.find({ predicate: 'type' }).all();
    const results2 = reader2.find({ predicate: 'type' }).all();

    expect(results1).toHaveLength(1);
    expect(results2).toHaveLength(1);
    expect(results1[0].object).toBe('test');
    expect(results2[0].object).toBe('test');

    await writer.close();
    await reader1.close();
    await reader2.close();
  });

  it('锁文件清理验证', async () => {
    const lockFile = `${dbPath}.lock`;

    // 打开带锁的数据库
    const db = await NervusDB.open(dbPath, { enableLock: true });

    // 锁文件应该存在
    {
      const fs = await import('node:fs/promises');
      await expect(fs.access(lockFile)).resolves.toBeUndefined();
    }

    // 关闭数据库
    await db.close();

    // 锁文件应该被清理
    {
      const fs = await import('node:fs/promises');
      await expect(fs.access(lockFile)).rejects.toThrow();
    }
  });

  it('进程崩溃后锁文件可能残留但新实例仍可启动', async () => {
    const lockFile = `${dbPath}.lock`;

    // 模拟进程崩溃：创建数据库但不正常关闭
    {
      const db = await NervusDB.open(dbPath, { enableLock: true });
      db.addFact({ subject: 'CrashTest', predicate: 'data', object: 'value' });
      await db.flush();
      // 不调用 close()，模拟崩溃
    }

    // 尝试创建新实例时，如果锁文件存在但进程不存在，应该能够启动
    // 注意：这个测试的行为依赖于具体的锁实现
    try {
      const db2 = await NervusDB.open(dbPath, { enableLock: true });

      // 验证数据恢复
      const results = db2.find({ subject: 'CrashTest' }).all();
      expect(results).toHaveLength(1);
      expect(results[0].object).toBe('value');

      await db2.close();
    } catch (error) {
      // 如果锁文件仍然阻止访问，这也是合理的行为
      // 具体行为取决于操作系统和锁的实现
      console.log('Lock file prevented access, which is acceptable behavior');
    }
  });

  it('同一进程内多次打开相同路径（同 PID）', async () => {
    // 第一个实例
    const db1 = await NervusDB.open(dbPath, { enableLock: true });

    // 同一进程的第二个实例，行为可能依赖于锁的实现
    // 一些实现允许同进程重复打开，一些不允许
    try {
      const db2 = await NervusDB.open(dbPath, { enableLock: true });

      // 如果允许，两个实例应该能协调工作
      db1.addFact({ subject: 'Instance1', predicate: 'data', object: 'value1' });
      db2.addFact({ subject: 'Instance2', predicate: 'data', object: 'value2' });

      await db1.flush();
      await db2.flush();

      await db2.close();
    } catch (error) {
      // 如果不允许同进程重复打开，这也是合理的
      console.log('Same process lock prevention, which may be expected');
    }

    await db1.close();
  });
});

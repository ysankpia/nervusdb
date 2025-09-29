import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { makeWorkspace, cleanupWorkspace, within } from '../../helpers/tempfs';
import { SynapseDB } from '@/synapseDb';
import { PluginManager, type SynapseDBPlugin } from '@/plugins/base';

describe('PluginManager 错误与边界', () => {
  let ws: string;
  beforeAll(async () => {
    ws = await makeWorkspace('plugin-errors');
  });
  afterAll(async () => {
    await cleanupWorkspace(ws);
  });

  it('重复插件名应报错；初始化后禁止注册', async () => {
    const dbPath = within(ws, 'db.synapsedb');
    const db = await SynapseDB.open(dbPath, { pageSize: 256 });
    const pm = new PluginManager(db, db.getStore());

    const ok: SynapseDBPlugin = { name: 'x', version: '1', initialize() {} };
    const dup: SynapseDBPlugin = { name: 'x', version: '2', initialize() {} };

    pm.register(ok);
    expect(() => pm.register(dup)).toThrow(/已存在/);

    await pm.initialize();
    expect(() => pm.register({ name: 'later', version: '1', initialize() {} })).toThrow(
      /初始化后注册插件/,
    );

    await db.close();
  });

  it('cleanup 应跳过未实现清理的插件，并并发执行', async () => {
    const dbPath = within(ws, 'db2.synapsedb');
    const db = await SynapseDB.open(dbPath, { pageSize: 256 });
    const pm = new PluginManager(db, db.getStore());

    let cleaned = 0;
    const a: SynapseDBPlugin = {
      name: 'a',
      version: '1',
      initialize() {},
      cleanup() {
        cleaned++;
      },
    };
    const b: SynapseDBPlugin = { name: 'b', version: '1', initialize() {} }; // 无 cleanup
    const c: SynapseDBPlugin = {
      name: 'c',
      version: '1',
      initialize() {},
      async cleanup() {
        await new Promise((r) => setTimeout(r, 10));
        cleaned++;
      },
    };

    pm.register(a);
    pm.register(b);
    pm.register(c);
    await pm.initialize();

    const t0 = Date.now();
    await pm.cleanup();
    const dt = Date.now() - t0;

    // 两个 cleanup 都执行了；并发 Promise.all 应小于串行的时间总和
    expect(cleaned).toBe(2);
    expect(dt).toBeLessThan(18);

    await db.close();
  });
});

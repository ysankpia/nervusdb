import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { makeWorkspace, cleanupWorkspace, within } from '../../helpers/tempfs';
import { ExtendedSynapseDB, type SynapseDBPlugin } from '@/plugins/base';

describe('PluginManager 生命周期', () => {
  let ws: string;
  beforeAll(async () => {
    ws = await makeWorkspace('plugin-lifecycle');
  });
  afterAll(async () => {
    await cleanupWorkspace(ws);
  });

  it('应按顺序初始化并在 close 时清理', async () => {
    const calls: string[] = [];

    const p1: SynapseDBPlugin = {
      name: 'p1',
      version: '1.0.0',
      initialize() {
        calls.push('p1:init');
      },
      cleanup() {
        calls.push('p1:cleanup');
      },
    };

    const p2: SynapseDBPlugin = {
      name: 'p2',
      version: '1.0.0',
      initialize() {
        calls.push('p2:init');
      },
      cleanup() {
        calls.push('p2:cleanup');
      },
    };

    const dbPath = within(ws, 'db.synapsedb');
    const db = await ExtendedSynapseDB.open(dbPath, { plugins: [p1, p2], pageSize: 256 });

    // 插件可查询
    expect(db.hasPlugin('p1')).toBe(true);
    expect(db.hasPlugin('p2')).toBe(true);
    expect(db.plugin('p1')?.name).toBe('p1');
    expect(db.listPlugins()).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ name: 'p1', version: '1.0.0' }),
        expect.objectContaining({ name: 'p2', version: '1.0.0' }),
      ]),
    );

    await db.close();

    // 初始化与清理都被调用
    expect(calls).toEqual(['p1:init', 'p2:init', 'p1:cleanup', 'p2:cleanup']);
  });
});

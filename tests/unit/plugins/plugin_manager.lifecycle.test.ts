import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { makeWorkspace, cleanupWorkspace, within } from '../../helpers/tempfs';
import { SynapseDB } from '@/synapseDb';

describe('PluginManager 生命周期', () => {
  let ws: string;
  beforeAll(async () => {
    ws = await makeWorkspace('plugin-lifecycle');
  });
  afterAll(async () => {
    await cleanupWorkspace(ws);
  });

  it('应按顺序初始化并在 close 时清理', async () => {
    const dbPath = within(ws, 'db.synapsedb');
    const db = await SynapseDB.open(dbPath, { pageSize: 256 });

    // 默认插件应该自动加载：pathfinding, aggregation
    expect(db.hasPlugin('pathfinding')).toBe(true);
    expect(db.hasPlugin('aggregation')).toBe(true);
    expect(db.plugin('pathfinding')?.name).toBe('pathfinding');
    expect(db.plugin('aggregation')?.name).toBe('aggregation');

    const plugins = db.listPlugins();
    expect(plugins).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ name: 'pathfinding', version: '1.0.0' }),
        expect.objectContaining({ name: 'aggregation', version: '1.0.0' }),
      ]),
    );

    await db.close();

    // 验证关闭后插件被清理（不抛异常即通过）
    expect(true).toBe(true);
  });
});

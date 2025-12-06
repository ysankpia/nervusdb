import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import {
  readTxIdRegistry,
  writeTxIdRegistry,
  toSet,
  mergeTxIds,
  type TxIdEntry,
  type TxIdRegistryData,
} from '@/core/storage/txidRegistry';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { rm, mkdir, writeFile, readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';

describe('事务ID注册表测试', () => {
  let testDir: string;

  beforeEach(async () => {
    // 创建临时测试目录
    testDir = join(
      tmpdir(),
      `test-txid-registry-${Date.now()}-${Math.random().toString(36).slice(2)}`,
    );
    await mkdir(testDir, { recursive: true });
  });

  afterEach(async () => {
    // 清理测试目录
    try {
      if (existsSync(testDir)) {
        await rm(testDir, { recursive: true });
      }
    } catch {
      // ignore cleanup errors
    }
  });

  describe('读取事务ID注册表', () => {
    it('应该能够读取有效的注册表文件', async () => {
      const testData: TxIdRegistryData = {
        version: 1,
        txIds: [
          { id: 'tx-001', ts: 1000 },
          { id: 'tx-002', ts: 2000, sessionId: 'session-1' },
        ],
        max: 100,
      };

      const txidsFile = join(testDir, 'txids.json');
      await writeFile(txidsFile, JSON.stringify(testData, null, 2), 'utf8');

      const result = await readTxIdRegistry(testDir);

      expect(result).toEqual(testData);
      expect(result.version).toBe(1);
      expect(result.txIds).toHaveLength(2);
      expect(result.max).toBe(100);
    });

    it('文件不存在时应该返回默认注册表', async () => {
      const result = await readTxIdRegistry(testDir);

      expect(result).toEqual({
        version: 1,
        txIds: [],
      });
    });

    it('文件损坏时应该返回默认注册表', async () => {
      const txidsFile = join(testDir, 'txids.json');
      await writeFile(txidsFile, 'invalid json content', 'utf8');

      const result = await readTxIdRegistry(testDir);

      expect(result).toEqual({
        version: 1,
        txIds: [],
      });
    });

    it('应该处理空的注册表文件', async () => {
      const txidsFile = join(testDir, 'txids.json');
      await writeFile(txidsFile, '{}', 'utf8');

      const result = await readTxIdRegistry(testDir);

      expect(result.version).toBeUndefined();
      expect(result.txIds).toBeUndefined();
    });
  });

  describe('写入事务ID注册表', () => {
    it('应该能够写入完整的注册表数据', async () => {
      const testData: TxIdRegistryData = {
        version: 1,
        txIds: [
          { id: 'tx-write-001', ts: 5000 },
          { id: 'tx-write-002', ts: 6000, sessionId: 'session-write' },
        ],
        max: 50,
      };

      await writeTxIdRegistry(testDir, testData);

      const txidsFile = join(testDir, 'txids.json');
      expect(existsSync(txidsFile)).toBe(true);

      const content = await readFile(txidsFile, 'utf8');
      const parsed = JSON.parse(content);

      expect(parsed).toEqual(testData);
    });

    it('应该原子性写入（先写临时文件再重命名）', async () => {
      const testData: TxIdRegistryData = {
        version: 1,
        txIds: [{ id: 'atomic-test', ts: 7000 }],
      };

      await writeTxIdRegistry(testDir, testData);

      const txidsFile = join(testDir, 'txids.json');
      const tmpFile = `${txidsFile}.tmp`;

      // 临时文件应该被删除
      expect(existsSync(tmpFile)).toBe(false);
      // 正式文件应该存在
      expect(existsSync(txidsFile)).toBe(true);
    });

    it('应该正确处理包含特殊字符的数据', async () => {
      const testData: TxIdRegistryData = {
        version: 1,
        txIds: [
          { id: 'tx-特殊字符-001', ts: 8000, sessionId: 'session-@#$%' },
          { id: 'tx-unicode-😀', ts: 9000 },
        ],
      };

      await writeTxIdRegistry(testDir, testData);

      const result = await readTxIdRegistry(testDir);
      expect(result.txIds[0].id).toBe('tx-特殊字符-001');
      expect(result.txIds[0].sessionId).toBe('session-@#$%');
      expect(result.txIds[1].id).toBe('tx-unicode-😀');
    });
  });

  describe('事务ID集合转换', () => {
    it('应该将注册表转换为ID集合', () => {
      const registry: TxIdRegistryData = {
        version: 1,
        txIds: [
          { id: 'tx-set-001', ts: 1000 },
          { id: 'tx-set-002', ts: 2000 },
          { id: 'tx-set-003', ts: 3000 },
        ],
      };

      const idSet = toSet(registry);

      expect(idSet).toBeInstanceOf(Set);
      expect(idSet.size).toBe(3);
      expect(idSet.has('tx-set-001')).toBe(true);
      expect(idSet.has('tx-set-002')).toBe(true);
      expect(idSet.has('tx-set-003')).toBe(true);
      expect(idSet.has('nonexistent')).toBe(false);
    });

    it('应该处理空注册表', () => {
      const registry: TxIdRegistryData = {
        version: 1,
        txIds: [],
      };

      const idSet = toSet(registry);

      expect(idSet.size).toBe(0);
    });
  });

  describe('事务ID合并', () => {
    it('应该合并新的事务ID', () => {
      const registry: TxIdRegistryData = {
        version: 1,
        txIds: [{ id: 'existing-tx', ts: 1000 }],
      };

      const newItems = [
        { id: 'new-tx-001', ts: 2000 },
        { id: 'new-tx-002', sessionId: 'session-merge' },
      ];

      const result = mergeTxIds(registry, newItems, undefined);

      expect(result.txIds).toHaveLength(3);
      expect(result.txIds[0].id).toBe('existing-tx');
      expect(result.txIds[1].id).toBe('new-tx-001');
      expect(result.txIds[1].ts).toBe(2000);
      expect(result.txIds[2].id).toBe('new-tx-002');
      expect(result.txIds[2].sessionId).toBe('session-merge');
      expect(typeof result.txIds[2].ts).toBe('number'); // 应该自动设置时间戳
    });

    it('应该忽略重复的事务ID', () => {
      const registry: TxIdRegistryData = {
        version: 1,
        txIds: [{ id: 'duplicate-tx', ts: 1000 }],
      };

      const newItems = [
        { id: 'duplicate-tx', ts: 2000 }, // 重复ID，应该被忽略
        { id: 'unique-tx', ts: 3000 },
      ];

      const result = mergeTxIds(registry, newItems, undefined);

      expect(result.txIds).toHaveLength(2);
      expect(result.txIds[0].id).toBe('duplicate-tx');
      expect(result.txIds[0].ts).toBe(1000); // 保持原时间戳
      expect(result.txIds[1].id).toBe('unique-tx');
    });

    it('应该忽略空的或无效的ID', () => {
      const registry: TxIdRegistryData = {
        version: 1,
        txIds: [],
      };

      const newItems = [
        { id: '', ts: 1000 }, // 空ID
        { id: 'valid-tx', ts: 2000 },
        { id: null as any, ts: 3000 }, // null ID
      ];

      const result = mergeTxIds(registry, newItems, undefined);

      expect(result.txIds).toHaveLength(1);
      expect(result.txIds[0].id).toBe('valid-tx');
    });

    it('应该根据max参数截断旧记录', () => {
      const registry: TxIdRegistryData = {
        version: 1,
        txIds: [
          { id: 'old-tx-1', ts: 1000 },
          { id: 'old-tx-2', ts: 2000 },
          { id: 'old-tx-3', ts: 3000 },
        ],
      };

      const newItems = [
        { id: 'new-tx-1', ts: 4000 },
        { id: 'new-tx-2', ts: 5000 },
      ];

      const result = mergeTxIds(registry, newItems, 3);

      expect(result.txIds).toHaveLength(3);
      expect(result.max).toBe(3);

      // 应该保留最新的3个事务（按时间戳排序）
      const ids = result.txIds.map((tx) => tx.id).sort();
      expect(ids).toEqual(['new-tx-1', 'new-tx-2', 'old-tx-3']);
    });

    it('max为0时不应该截断', () => {
      const registry: TxIdRegistryData = {
        version: 1,
        txIds: [
          { id: 'tx-1', ts: 1000 },
          { id: 'tx-2', ts: 2000 },
        ],
      };

      const newItems = [{ id: 'tx-3', ts: 3000 }];

      const result = mergeTxIds(registry, newItems, 0);

      expect(result.txIds).toHaveLength(3);
      expect(result.max).toBeUndefined();
    });

    it('max为负数时不应该截断', () => {
      const registry: TxIdRegistryData = {
        version: 1,
        txIds: [
          { id: 'tx-1', ts: 1000 },
          { id: 'tx-2', ts: 2000 },
        ],
      };

      const newItems = [{ id: 'tx-3', ts: 3000 }];

      const result = mergeTxIds(registry, newItems, -5);

      expect(result.txIds).toHaveLength(3);
      expect(result.max).toBeUndefined();
    });
  });

  describe('综合场景测试', () => {
    it('应该支持完整的读写循环', async () => {
      // 写入初始数据
      const initialData: TxIdRegistryData = {
        version: 1,
        txIds: [{ id: 'initial-tx', ts: 1000 }],
        max: 5,
      };

      await writeTxIdRegistry(testDir, initialData);

      // 读取数据
      let registry = await readTxIdRegistry(testDir);
      expect(registry.txIds).toHaveLength(1);

      // 合并新数据
      const newItems = [
        { id: 'cycle-tx-1', ts: 2000 },
        { id: 'cycle-tx-2', ts: 3000 },
      ];
      registry = mergeTxIds(registry, newItems, 5);

      // 再次写入
      await writeTxIdRegistry(testDir, registry);

      // 最终读取验证
      const finalRegistry = await readTxIdRegistry(testDir);
      expect(finalRegistry.txIds).toHaveLength(3);
      expect(finalRegistry.max).toBe(5);

      const idSet = toSet(finalRegistry);
      expect(idSet.has('initial-tx')).toBe(true);
      expect(idSet.has('cycle-tx-1')).toBe(true);
      expect(idSet.has('cycle-tx-2')).toBe(true);
    });

    it('应该处理大量事务ID的性能场景', () => {
      const registry: TxIdRegistryData = {
        version: 1,
        txIds: [],
      };

      // 生成1000个事务ID
      const newItems = Array.from({ length: 1000 }, (_, i) => ({
        id: `perf-tx-${i.toString().padStart(4, '0')}`,
        ts: 1000 + i,
        sessionId: `session-${i % 10}`,
      }));

      const start = Date.now();
      const result = mergeTxIds(registry, newItems, 500);
      const duration = Date.now() - start;

      expect(result.txIds).toHaveLength(500);
      expect(result.max).toBe(500);
      expect(duration).toBeLessThan(100); // 应该在100ms内完成

      // 验证保留的是最新的500个
      const latestIds = result.txIds.map((tx) => tx.id).sort();
      expect(latestIds[0]).toBe('perf-tx-0500');
      expect(latestIds[latestIds.length - 1]).toBe('perf-tx-0999');
    });
  });
});

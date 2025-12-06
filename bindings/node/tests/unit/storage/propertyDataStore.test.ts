import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PropertyDataStore } from '../../../src/core/storage/propertyDataStore.js';

describe('PropertyDataStore - 属性数据分页存储', () => {
  let testDir: string;
  let store: PropertyDataStore;

  beforeEach(async () => {
    testDir = await mkdtemp(join(tmpdir(), 'property-data-test-'));
    // 使用足够大的缓存大小以支持大数据集测试
    store = new PropertyDataStore(testDir, 1024, 3000);
    await store.initialize();
  });

  afterEach(async () => {
    try {
      await rm(testDir, { recursive: true, force: true });
    } catch {}
  });

  describe('基础读写功能', () => {
    it('should set and get node properties from cache', async () => {
      // 设置属性
      store.setNodeProperties(1, { name: 'Alice', age: 25 });

      // 从缓存读取
      const properties = await store.getNodeProperties(1);
      expect(properties).toEqual({ name: 'Alice', age: 25 });
    });

    it('should return undefined for non-existent node', async () => {
      const properties = await store.getNodeProperties(999);
      expect(properties).toBeUndefined();
    });

    it('should handle empty properties', async () => {
      store.setNodeProperties(1, {});
      const properties = await store.getNodeProperties(1);
      expect(properties).toEqual({});
    });
  });

  describe('持久化和加载', () => {
    it('should persist and reload node properties', async () => {
      // 写入数据
      store.setNodeProperties(1, { name: 'Alice', age: 25 });
      store.setNodeProperties(2, { name: 'Bob', age: 30 });
      store.setNodeProperties(3, { name: 'Charlie', age: 35 });

      // 持久化
      await store.flush();

      // 清空缓存
      store.clearCache();

      // 从磁盘重新加载
      const alice = await store.getNodeProperties(1);
      const bob = await store.getNodeProperties(2);
      const charlie = await store.getNodeProperties(3);

      expect(alice).toEqual({ name: 'Alice', age: 25 });
      expect(bob).toEqual({ name: 'Bob', age: 30 });
      expect(charlie).toEqual({ name: 'Charlie', age: 35 });
    });

    it('should handle large dataset with multiple pages', async () => {
      // 插入2000个节点（跨越2个page，pageSize=1024）
      for (let i = 0; i < 2000; i++) {
        store.setNodeProperties(i, {
          name: `Node_${i}`,
          value: i * 10,
        });
      }

      // 持久化
      await store.flush();

      // 清空缓存
      store.clearCache();

      // 验证随机访问
      const node0 = await store.getNodeProperties(0);
      const node1000 = await store.getNodeProperties(1000);
      const node1999 = await store.getNodeProperties(1999);

      expect(node0).toEqual({ name: 'Node_0', value: 0 });
      expect(node1000).toEqual({ name: 'Node_1000', value: 10000 });
      expect(node1999).toEqual({ name: 'Node_1999', value: 19990 });
    });
  });

  describe('缓存机制', () => {
    it('should use cache for repeated queries', async () => {
      store.setNodeProperties(1, { name: 'Alice' });
      await store.flush();
      store.clearCache();

      // 第一次查询（从磁盘）
      const first = await store.getNodeProperties(1);
      expect(first).toEqual({ name: 'Alice' });

      // 检查缓存统计
      const stats = store.getCacheStats();
      expect(stats.nodeCacheSize).toBe(1);

      // 第二次查询（从缓存，性能更好）
      const second = await store.getNodeProperties(1);
      expect(second).toEqual({ name: 'Alice' });
    });
  });

  describe('边属性支持', () => {
    it('should set and get edge properties from cache', async () => {
      const edgeKey = '0:1:2';
      store.setEdgeProperties(edgeKey, { weight: 5, type: 'friend' });

      const properties = await store.getEdgeProperties(edgeKey);
      expect(properties).toEqual({ weight: 5, type: 'friend' });
    });
  });
});

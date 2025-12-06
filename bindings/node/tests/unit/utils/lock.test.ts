import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { acquireLock, type LockHandle } from '@/utils/lock';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { unlink, readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';

describe('文件锁工具测试', () => {
  let testBasePath: string;
  let lockPath: string;

  beforeEach(async () => {
    // 创建临时锁文件路径
    testBasePath = join(tmpdir(), `test-lock-${Date.now()}-${Math.random().toString(36).slice(2)}`);
    lockPath = `${testBasePath}.lock`;
  });

  afterEach(async () => {
    // 清理测试锁文件
    try {
      if (existsSync(lockPath)) {
        await unlink(lockPath);
      }
    } catch {
      // ignore cleanup errors
    }
  });

  describe('基础锁获取和释放', () => {
    it('应该能够成功获取锁', async () => {
      const lock = await acquireLock(testBasePath);
      expect(lock).toBeDefined();
      expect(typeof lock.release).toBe('function');

      // 锁文件应该存在
      expect(existsSync(lockPath)).toBe(true);

      await lock.release();
    });

    it('应该能够释放锁', async () => {
      const lock = await acquireLock(testBasePath);
      expect(existsSync(lockPath)).toBe(true);

      await lock.release();

      // 锁文件应该被删除
      expect(existsSync(lockPath)).toBe(false);
    });

    it('锁文件应该包含正确的元数据', async () => {
      const lock = await acquireLock(testBasePath);

      const lockContent = await readFile(lockPath, 'utf8');
      const metadata = JSON.parse(lockContent);

      expect(metadata.pid).toBe(process.pid);
      expect(typeof metadata.startedAt).toBe('number');
      expect(metadata.startedAt).toBeGreaterThan(Date.now() - 5000); // 5秒内创建

      await lock.release();
    });
  });

  describe('锁冲突检测', () => {
    it('相同路径的第二个锁获取应该失败', async () => {
      const lock1 = await acquireLock(testBasePath);

      await expect(acquireLock(testBasePath)).rejects.toThrow('数据库正被占用');

      await lock1.release();
    });

    it('释放锁后应该允许重新获取', async () => {
      const lock1 = await acquireLock(testBasePath);
      await lock1.release();

      // 第二个锁获取应该成功
      const lock2 = await acquireLock(testBasePath);
      expect(lock2).toBeDefined();

      await lock2.release();
    });

    it('不同路径的锁应该不冲突', async () => {
      const basePath2 = join(
        tmpdir(),
        `test-lock-2-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      );

      const lock1 = await acquireLock(testBasePath);
      const lock2 = await acquireLock(basePath2);

      expect(lock1).toBeDefined();
      expect(lock2).toBeDefined();

      await lock1.release();
      await lock2.release();

      // 清理第二个锁文件
      try {
        await unlink(`${basePath2}.lock`);
      } catch {}
    });
  });

  describe('错误处理', () => {
    it('多次释放同一个锁应该安全', async () => {
      const lock = await acquireLock(testBasePath);

      await lock.release();
      await expect(lock.release()).resolves.not.toThrow();
      await expect(lock.release()).resolves.not.toThrow();
    });

    it('获取锁失败时应该包含错误信息', async () => {
      const lock1 = await acquireLock(testBasePath);

      try {
        await acquireLock(testBasePath);
        expect.fail('应该抛出错误');
      } catch (error) {
        expect(error).toBeInstanceOf(Error);
        expect((error as Error).message).toMatch(/数据库正被占用/);
        expect((error as Error).message).toMatch(/已重试 3 次/);
        expect((error as Error).message).toMatch(/提示：请检查是否有其他进程/);
      }

      await lock1.release();
    });
  });

  describe('边界条件', () => {
    it('应该支持长路径名', async () => {
      const longSegment = 'a'.repeat(50);
      const longBasePath = join(tmpdir(), longSegment);

      const lock = await acquireLock(longBasePath);
      expect(existsSync(`${longBasePath}.lock`)).toBe(true);

      await lock.release();
      expect(existsSync(`${longBasePath}.lock`)).toBe(false);
    });

    it('应该支持包含特殊字符的路径', async () => {
      const specialBasePath = join(tmpdir(), 'test-lock@special#path');

      const lock = await acquireLock(specialBasePath);
      expect(existsSync(`${specialBasePath}.lock`)).toBe(true);

      await lock.release();
      expect(existsSync(`${specialBasePath}.lock`)).toBe(false);
    });

    it('锁文件内容应该是有效的JSON', async () => {
      const lock = await acquireLock(testBasePath);

      const content = await readFile(lockPath, 'utf8');
      expect(() => JSON.parse(content)).not.toThrow();

      const parsed = JSON.parse(content);
      expect(typeof parsed).toBe('object');
      expect(typeof parsed.pid).toBe('number');
      expect(typeof parsed.startedAt).toBe('number');

      await lock.release();
    });
  });

  describe('并发安全性', () => {
    it('并发获取锁只有一个应该成功', async () => {
      const promises = [
        acquireLock(testBasePath).catch(() => null),
        acquireLock(testBasePath).catch(() => null),
        acquireLock(testBasePath).catch(() => null),
      ];

      const results = await Promise.all(promises);
      const successful = results.filter((result) => result !== null);

      expect(successful).toHaveLength(1);
      expect(successful[0]).toBeDefined();

      // 释放成功的锁
      if (successful[0]) {
        await successful[0].release();
      }
    });

    it('快速获取释放循环应该稳定', async () => {
      for (let i = 0; i < 10; i++) {
        const lock = await acquireLock(testBasePath);
        await lock.release();
      }

      // 最后检查锁文件已被清理
      expect(existsSync(lockPath)).toBe(false);
    });
  });

  describe('锁句柄行为', () => {
    it('锁句柄应该有release方法', async () => {
      const lock = await acquireLock(testBasePath);

      expect(typeof lock.release).toBe('function');
      expect(lock.release.constructor.name).toBe('AsyncFunction');

      await lock.release();
    });

    it('release方法应该返回Promise', async () => {
      const lock = await acquireLock(testBasePath);

      const releasePromise = lock.release();
      expect(releasePromise).toBeInstanceOf(Promise);

      await releasePromise;
    });
  });
});

import { describe, expect, it, beforeEach } from 'vitest';
import { setCrashPoint, triggerCrash } from '@/utils/fault';

describe('崩溃注入工具测试', () => {
  beforeEach(() => {
    // 每个测试前重置崩溃点
    setCrashPoint(null);
  });

  describe('基础功能', () => {
    it('应该能够设置崩溃点', () => {
      expect(() => setCrashPoint('test-point')).not.toThrow();
    });

    it('应该能够清除崩溃点', () => {
      setCrashPoint('test-point');
      expect(() => setCrashPoint(null)).not.toThrow();
    });

    it('没有设置崩溃点时触发崩溃不应该抛错', () => {
      expect(() => triggerCrash('any-point')).not.toThrow();
    });

    it('不匹配的崩溃点不应该触发崩溃', () => {
      setCrashPoint('point-a');
      expect(() => triggerCrash('point-b')).not.toThrow();
    });
  });

  describe('崩溃触发机制', () => {
    it('匹配的崩溃点应该触发崩溃', () => {
      setCrashPoint('crash-here');

      expect(() => triggerCrash('crash-here')).toThrow('InjectedCrash:crash-here');
    });

    it('崩溃触发后应该自动清除崩溃点', () => {
      setCrashPoint('one-time-crash');

      // 第一次触发应该崩溃
      expect(() => triggerCrash('one-time-crash')).toThrow('InjectedCrash:one-time-crash');

      // 第二次触发不应该崩溃（已被清除）
      expect(() => triggerCrash('one-time-crash')).not.toThrow();
    });

    it('应该生成正确的错误消息格式', () => {
      setCrashPoint('format-test');

      let error: Error | null = null;
      try {
        triggerCrash('format-test');
      } catch (e) {
        error = e as Error;
      }

      expect(error).not.toBeNull();
      expect(error!.message).toBe('InjectedCrash:format-test');
      expect(error!).toBeInstanceOf(Error);
    });
  });

  describe('多点崩溃管理', () => {
    it('应该支持切换不同的崩溃点', () => {
      // 设置第一个崩溃点
      setCrashPoint('point-1');
      expect(() => triggerCrash('point-1')).toThrow('InjectedCrash:point-1');

      // 切换到第二个崩溃点
      setCrashPoint('point-2');
      expect(() => triggerCrash('point-2')).toThrow('InjectedCrash:point-2');

      // 第一个崩溃点不再有效
      expect(() => triggerCrash('point-1')).not.toThrow();
    });

    it('覆盖崩溃点应该立即生效', () => {
      setCrashPoint('original');
      setCrashPoint('override');

      // 原始崩溃点无效
      expect(() => triggerCrash('original')).not.toThrow();

      // 覆盖的崩溃点有效
      expect(() => triggerCrash('override')).toThrow('InjectedCrash:override');
    });
  });

  describe('边界条件', () => {
    it('应该支持空字符串崩溃点', () => {
      setCrashPoint('');
      // 空字符串在JavaScript中是falsy，所以不会触发崩溃
      expect(() => triggerCrash('')).not.toThrow();

      // 但是非空字符串可以正常工作
      setCrashPoint('test');
      expect(() => triggerCrash('test')).toThrow('InjectedCrash:test');
    });

    it('应该支持包含特殊字符的崩溃点', () => {
      const specialPoint = 'test:point/with-special@chars#123';
      setCrashPoint(specialPoint);
      expect(() => triggerCrash(specialPoint)).toThrow(`InjectedCrash:${specialPoint}`);
    });

    it('应该支持长崩溃点名称', () => {
      const longPoint = 'a'.repeat(100);
      setCrashPoint(longPoint);
      expect(() => triggerCrash(longPoint)).toThrow(`InjectedCrash:${longPoint}`);
    });

    it('多次设置null应该安全', () => {
      setCrashPoint(null);
      setCrashPoint(null);
      setCrashPoint(null);

      expect(() => triggerCrash('any')).not.toThrow();
    });

    it('多次触发同一个不存在的崩溃点应该安全', () => {
      expect(() => triggerCrash('nonexistent')).not.toThrow();
      expect(() => triggerCrash('nonexistent')).not.toThrow();
      expect(() => triggerCrash('nonexistent')).not.toThrow();
    });
  });

  describe('实际应用场景模拟', () => {
    it('模拟数据库写入时的崩溃注入', () => {
      const simulateDbWrite = (data: string) => {
        triggerCrash('db-write-failure');
        return `written: ${data}`;
      };

      // 设置在数据库写入时崩溃
      setCrashPoint('db-write-failure');

      expect(() => simulateDbWrite('test-data')).toThrow('InjectedCrash:db-write-failure');
    });

    it('模拟索引构建时的崩溃注入', () => {
      const buildIndex = (items: number[]) => {
        items.forEach((_, index) => {
          if (index === 5) {
            triggerCrash('index-build-halfway');
          }
        });
        return 'index-complete';
      };

      setCrashPoint('index-build-halfway');

      const items = Array.from({ length: 10 }, (_, i) => i);
      expect(() => buildIndex(items)).toThrow('InjectedCrash:index-build-halfway');
    });

    it('模拟事务提交时的崩溃注入', () => {
      const commitTransaction = (txId: string) => {
        triggerCrash(`commit-${txId}`);
        return 'committed';
      };

      setCrashPoint('commit-tx-123');

      expect(() => commitTransaction('tx-123')).toThrow('InjectedCrash:commit-tx-123');

      // 不同事务ID不会触发
      expect(commitTransaction('tx-456')).toBe('committed');
    });
  });
});

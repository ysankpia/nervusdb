import { describe, expect, it, beforeEach } from 'vitest';
import { LsmLiteStaging, type StagingStrategy, type StagingMode } from '@/core/storage/staging';

describe('暂存层测试', () => {
  describe('LSM-Lite 暂存实现', () => {
    let staging: LsmLiteStaging<string>;

    beforeEach(() => {
      staging = new LsmLiteStaging<string>();
    });

    describe('基础操作', () => {
      it('应该创建空的暂存层', () => {
        expect(staging.size()).toBe(0);
      });

      it('应该能够添加单个记录', () => {
        staging.add('record-1');

        expect(staging.size()).toBe(1);
      });

      it('应该能够添加多个记录', () => {
        staging.add('record-1');
        staging.add('record-2');
        staging.add('record-3');

        expect(staging.size()).toBe(3);
      });

      it('应该能够取出所有记录', () => {
        staging.add('record-1');
        staging.add('record-2');
        staging.add('record-3');

        const records = staging.drain();

        expect(records).toEqual(['record-1', 'record-2', 'record-3']);
        expect(records).toHaveLength(3);
      });

      it('drain后应该清空暂存层', () => {
        staging.add('record-1');
        staging.add('record-2');

        expect(staging.size()).toBe(2);

        const records = staging.drain();
        expect(records).toHaveLength(2);
        expect(staging.size()).toBe(0);
      });
    });

    describe('类型支持', () => {
      it('应该支持字符串类型', () => {
        const strStaging = new LsmLiteStaging<string>();

        strStaging.add('hello');
        strStaging.add('world');

        const records = strStaging.drain();
        expect(records).toEqual(['hello', 'world']);
      });

      it('应该支持数字类型', () => {
        const numStaging = new LsmLiteStaging<number>();

        numStaging.add(1);
        numStaging.add(2);
        numStaging.add(3);

        const records = numStaging.drain();
        expect(records).toEqual([1, 2, 3]);
        expect(numStaging.size()).toBe(0);
      });

      it('应该支持对象类型', () => {
        interface TestRecord {
          id: string;
          value: number;
        }

        const objStaging = new LsmLiteStaging<TestRecord>();

        objStaging.add({ id: 'obj-1', value: 100 });
        objStaging.add({ id: 'obj-2', value: 200 });

        expect(objStaging.size()).toBe(2);

        const records = objStaging.drain();
        expect(records).toEqual([
          { id: 'obj-1', value: 100 },
          { id: 'obj-2', value: 200 },
        ]);
      });

      it('应该支持复杂嵌套对象', () => {
        interface ComplexRecord {
          metadata: {
            id: string;
            timestamp: number;
          };
          data: Array<{ key: string; value: any }>;
        }

        const complexStaging = new LsmLiteStaging<ComplexRecord>();

        const record: ComplexRecord = {
          metadata: { id: 'complex-1', timestamp: Date.now() },
          data: [
            { key: 'name', value: 'test' },
            { key: 'count', value: 42 },
          ],
        };

        complexStaging.add(record);
        expect(complexStaging.size()).toBe(1);

        const records = complexStaging.drain();
        expect(records[0].metadata.id).toBe('complex-1');
        expect(records[0].data).toHaveLength(2);
      });
    });

    describe('边界条件', () => {
      it('空暂存层drain应该返回空数组', () => {
        const records = staging.drain();

        expect(records).toEqual([]);
        expect(records).toHaveLength(0);
        expect(staging.size()).toBe(0);
      });

      it('多次drain应该都返回空数组', () => {
        staging.add('test');
        staging.drain(); // 第一次drain

        const secondDrain = staging.drain();
        const thirdDrain = staging.drain();

        expect(secondDrain).toEqual([]);
        expect(thirdDrain).toEqual([]);
        expect(staging.size()).toBe(0);
      });

      it('应该处理null和undefined值', () => {
        const nullableStaging = new LsmLiteStaging<string | null | undefined>();

        nullableStaging.add(null);
        nullableStaging.add(undefined);
        nullableStaging.add('valid');

        expect(nullableStaging.size()).toBe(3);

        const records = nullableStaging.drain();
        expect(records).toEqual([null, undefined, 'valid']);
      });
    });

    describe('性能和内存管理', () => {
      it('应该支持大量记录的添加', () => {
        const largeStaging = new LsmLiteStaging<number>();
        const count = 10000;

        // 添加大量记录
        for (let i = 0; i < count; i++) {
          largeStaging.add(i);
        }

        expect(largeStaging.size()).toBe(count);

        // 验证drain性能
        const start = Date.now();
        const records = largeStaging.drain();
        const duration = Date.now() - start;

        expect(records).toHaveLength(count);
        expect(records[0]).toBe(0);
        expect(records[count - 1]).toBe(count - 1);
        expect(duration).toBeLessThan(100); // 应该在100ms内完成
        expect(largeStaging.size()).toBe(0);
      });

      it('应该正确处理重复的drain操作', () => {
        for (let i = 0; i < 5; i++) {
          staging.add(`record-${i}`);
        }

        // 第一次drain获取所有记录
        const firstDrain = staging.drain();
        expect(firstDrain).toHaveLength(5);

        // 后续drain应该返回空数组
        for (let i = 0; i < 10; i++) {
          const emptyDrain = staging.drain();
          expect(emptyDrain).toEqual([]);
          expect(staging.size()).toBe(0);
        }
      });
    });

    describe('内部状态一致性', () => {
      it('size()应该始终反映正确的记录数量', () => {
        expect(staging.size()).toBe(0);

        // 逐步添加记录，验证size
        for (let i = 1; i <= 10; i++) {
          staging.add(`record-${i}`);
          expect(staging.size()).toBe(i);
        }

        // drain后size应该为0
        staging.drain();
        expect(staging.size()).toBe(0);

        // 再次添加记录
        staging.add('new-record');
        expect(staging.size()).toBe(1);
      });

      it('add和drain操作应该保持FIFO顺序', () => {
        const testData = ['first', 'second', 'third', 'fourth', 'fifth'];

        testData.forEach((item) => staging.add(item));

        const drained = staging.drain();
        expect(drained).toEqual(testData);

        // 验证顺序完全一致
        for (let i = 0; i < testData.length; i++) {
          expect(drained[i]).toBe(testData[i]);
        }
      });
    });

    describe('暂存策略接口兼容性', () => {
      it('应该实现StagingStrategy接口', () => {
        const strategy: StagingStrategy<string> = new LsmLiteStaging<string>();

        strategy.add('interface-test');
        expect(strategy.size()).toBe(1);
      });

      it('应该支持作为泛型策略使用', () => {
        function testStrategy<T>(strategy: StagingStrategy<T>, testItem: T): number {
          strategy.add(testItem);
          return strategy.size();
        }

        const size = testStrategy(staging, 'generic-test');
        expect(size).toBe(1);
      });
    });

    describe('类型定义验证', () => {
      it('StagingMode类型应该包含正确的值', () => {
        const defaultMode: StagingMode = 'default';
        const lsmMode: StagingMode = 'lsm-lite';

        expect(defaultMode).toBe('default');
        expect(lsmMode).toBe('lsm-lite');

        // 类型检查（编译时验证）
        const modes: StagingMode[] = ['default', 'lsm-lite'];
        expect(modes).toHaveLength(2);
      });
    });
  });
});

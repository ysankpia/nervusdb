import { describe, expect, it, beforeEach } from 'vitest';
import { MinHeap } from '@/utils/minHeap';

interface TestItem {
  id: number;
  priority: number;
}

describe('MinHeap 最小堆实现', () => {
  let heap: MinHeap<TestItem>;

  beforeEach(() => {
    heap = new MinHeap<TestItem>((a, b) => a.priority - b.priority);
  });

  describe('基础操作', () => {
    it('应该创建空堆', () => {
      expect(heap.size()).toBe(0);
      expect(heap.isEmpty()).toBe(true);
    });

    it('应该正确插入单个元素', () => {
      const item = { id: 1, priority: 10 };
      heap.push(item);

      expect(heap.size()).toBe(1);
      expect(heap.isEmpty()).toBe(false);
      expect(heap.peek()).toEqual(item);
    });

    it('应该正确插入多个元素', () => {
      const items = [
        { id: 1, priority: 30 },
        { id: 2, priority: 10 },
        { id: 3, priority: 20 },
      ];

      items.forEach((item) => heap.push(item));

      expect(heap.size()).toBe(3);
      expect(heap.peek()).toEqual({ id: 2, priority: 10 }); // 最小值
    });

    it('应该正确弹出最小元素', () => {
      const items = [
        { id: 1, priority: 30 },
        { id: 2, priority: 10 },
        { id: 3, priority: 20 },
      ];

      items.forEach((item) => heap.push(item));

      const min = heap.pop();
      expect(min).toEqual({ id: 2, priority: 10 });
      expect(heap.size()).toBe(2);
      expect(heap.peek()).toEqual({ id: 3, priority: 20 });
    });
  });

  describe('堆排序性质', () => {
    it('应该维护最小堆性质', () => {
      const items = [
        { id: 1, priority: 50 },
        { id: 2, priority: 30 },
        { id: 3, priority: 70 },
        { id: 4, priority: 10 },
        { id: 5, priority: 40 },
        { id: 6, priority: 60 },
      ];

      // 随机顺序插入
      items.forEach((item) => heap.push(item));

      // 应该按优先级顺序弹出
      const results: TestItem[] = [];
      while (!heap.isEmpty()) {
        results.push(heap.pop());
      }

      const expectedOrder = [
        { id: 4, priority: 10 },
        { id: 2, priority: 30 },
        { id: 5, priority: 40 },
        { id: 1, priority: 50 },
        { id: 6, priority: 60 },
        { id: 3, priority: 70 },
      ];

      expect(results).toEqual(expectedOrder);
    });

    it('应该正确处理重复优先级', () => {
      const items = [
        { id: 1, priority: 20 },
        { id: 2, priority: 10 },
        { id: 3, priority: 20 },
        { id: 4, priority: 10 },
      ];

      items.forEach((item) => heap.push(item));

      // 弹出所有元素
      const results: TestItem[] = [];
      while (!heap.isEmpty()) {
        results.push(heap.pop());
      }

      // 应该先弹出优先级为10的元素，然后是20的元素
      expect(results[0].priority).toBe(10);
      expect(results[1].priority).toBe(10);
      expect(results[2].priority).toBe(20);
      expect(results[3].priority).toBe(20);

      // 验证所有元素都被弹出
      expect(results).toHaveLength(4);
    });
  });

  describe('边界条件', () => {
    it('空堆 peek 应该返回 undefined', () => {
      expect(heap.peek()).toBeUndefined();
    });

    it('空堆 pop 应该返回 undefined', () => {
      expect(heap.pop()).toBeUndefined();
    });

    it('应该正确处理单个元素的堆', () => {
      const item = { id: 1, priority: 10 };
      heap.push(item);

      expect(heap.peek()).toEqual(item);
      expect(heap.size()).toBe(1);

      const popped = heap.pop();
      expect(popped).toEqual(item);
      expect(heap.isEmpty()).toBe(true);
      expect(heap.size()).toBe(0);
    });

    it('应该正确处理大量元素', () => {
      const items: TestItem[] = [];
      for (let i = 0; i < 1000; i++) {
        items.push({ id: i, priority: Math.random() * 1000 });
      }

      // 插入所有元素
      items.forEach((item) => heap.push(item));
      expect(heap.size()).toBe(1000);

      // 弹出所有元素并验证排序
      const results: TestItem[] = [];
      let lastPriority = -Infinity;

      while (!heap.isEmpty()) {
        const item = heap.pop()!;
        expect(item.priority).toBeGreaterThanOrEqual(lastPriority);
        lastPriority = item.priority;
        results.push(item);
      }

      expect(results).toHaveLength(1000);
    });
  });

  describe('自定义比较函数', () => {
    it('应该支持最大堆（反向比较）', () => {
      const maxHeap = new MinHeap<TestItem>((a, b) => b.priority - a.priority);

      const items = [
        { id: 1, priority: 10 },
        { id: 2, priority: 30 },
        { id: 3, priority: 20 },
      ];

      items.forEach((item) => maxHeap.push(item));

      expect(maxHeap.peek()).toEqual({ id: 2, priority: 30 }); // 最大值

      const results: TestItem[] = [];
      while (!maxHeap.isEmpty()) {
        results.push(maxHeap.pop());
      }

      // 应该按降序排列
      expect(results).toEqual([
        { id: 2, priority: 30 },
        { id: 3, priority: 20 },
        { id: 1, priority: 10 },
      ]);
    });

    it('应该支持字符串比较', () => {
      const stringHeap = new MinHeap<string>((a, b) => a.localeCompare(b));

      const items = ['banana', 'apple', 'cherry', 'date'];
      items.forEach((item) => stringHeap.push(item));

      const results: string[] = [];
      while (!stringHeap.isEmpty()) {
        results.push(stringHeap.pop());
      }

      expect(results).toEqual(['apple', 'banana', 'cherry', 'date']);
    });

    it('应该支持复合比较条件', () => {
      interface ComplexItem {
        priority: number;
        timestamp: number;
        id: number;
      }

      // 先按优先级，再按时间戳，最后按 ID
      const complexHeap = new MinHeap<ComplexItem>((a, b) => {
        if (a.priority !== b.priority) return a.priority - b.priority;
        if (a.timestamp !== b.timestamp) return a.timestamp - b.timestamp;
        return a.id - b.id;
      });

      const items: ComplexItem[] = [
        { priority: 1, timestamp: 100, id: 2 },
        { priority: 1, timestamp: 100, id: 1 },
        { priority: 1, timestamp: 200, id: 3 },
        { priority: 2, timestamp: 50, id: 4 },
      ];

      items.forEach((item) => complexHeap.push(item));

      const results: ComplexItem[] = [];
      while (!complexHeap.isEmpty()) {
        results.push(complexHeap.pop());
      }

      expect(results).toEqual([
        { priority: 1, timestamp: 100, id: 1 },
        { priority: 1, timestamp: 100, id: 2 },
        { priority: 1, timestamp: 200, id: 3 },
        { priority: 2, timestamp: 50, id: 4 },
      ]);
    });
  });

  describe('性能和稳定性', () => {
    it('应该在插入过程中维护堆性质', () => {
      const items = [100, 50, 150, 25, 75, 125, 175];

      items.forEach((priority) => {
        heap.push({ id: priority, priority });
        // 每次插入后验证堆顶是最小值
        expect(heap.peek()?.priority).toBe(
          Math.min(...items.slice(0, items.indexOf(priority) + 1)),
        );
      });
    });

    it('应该在弹出过程中维护堆性质', () => {
      const items = [100, 50, 150, 25, 75, 125, 175];
      items.forEach((priority) => heap.push({ id: priority, priority }));

      let lastPopped = -Infinity;
      while (!heap.isEmpty()) {
        const current = heap.pop()!;
        expect(current.priority).toBeGreaterThanOrEqual(lastPopped);
        lastPopped = current.priority;
      }
    });
  });

  describe('内部状态', () => {
    it('size() 应该始终返回正确的元素数量', () => {
      expect(heap.size()).toBe(0);

      // 插入元素
      for (let i = 0; i < 10; i++) {
        heap.push({ id: i, priority: i });
        expect(heap.size()).toBe(i + 1);
      }

      // 弹出元素
      for (let i = 10; i > 0; i--) {
        expect(heap.size()).toBe(i);
        heap.pop();
      }

      expect(heap.size()).toBe(0);
    });

    it('isEmpty() 应该与 size() 一致', () => {
      expect(heap.isEmpty()).toBe(heap.size() === 0);

      heap.push({ id: 1, priority: 1 });
      expect(heap.isEmpty()).toBe(heap.size() === 0);

      heap.pop();
      expect(heap.isEmpty()).toBe(heap.size() === 0);
    });
  });
});

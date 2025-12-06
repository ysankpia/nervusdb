import { describe, it, expect } from 'vitest';
import { MinHeap } from '@/utils/minHeap';

describe('MinHeap · 最小堆基础能力', () => {
  it('插入后 size/peek 正常，pop 返回最小值', () => {
    const heap = new MinHeap<number>((a, b) => a - b);
    expect(heap.isEmpty()).toBe(true);
    heap.push(5);
    heap.push(3);
    heap.push(7);
    expect(heap.size()).toBe(3);
    expect(heap.peek()).toBe(3);
    expect(heap.pop()).toBe(3);
    expect(heap.peek()).toBe(5);
    expect(heap.size()).toBe(2);
  });

  it('连续 pop 应按从小到大有序返回，空堆返回 undefined', () => {
    const heap = new MinHeap<number>((a, b) => a - b);
    [10, 1, 6, 4, 2].forEach((n) => heap.push(n));
    const out: number[] = [];
    while (!heap.isEmpty()) out.push(heap.pop());
    expect(out).toEqual([1, 2, 4, 6, 10]);
    expect(heap.pop()).toBeUndefined();
    expect(heap.peek()).toBeUndefined();
  });
});

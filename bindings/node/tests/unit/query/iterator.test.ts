import { describe, it, expect } from 'vitest';
import { flattenBatches } from '@/extensions/query/iterator.ts';

describe('流式工具 · flattenBatches', () => {
  it('应将批量异步迭代扁平化为逐条输出', async () => {
    async function* gen() {
      yield [{ subjectId: 1 } as any, { subjectId: 2 } as any];
      yield [{ subjectId: 3 } as any];
    }
    const out: any[] = [];
    for await (const r of flattenBatches(gen())) out.push(r.subjectId);
    expect(out).toEqual([1, 2, 3]);
  });
});

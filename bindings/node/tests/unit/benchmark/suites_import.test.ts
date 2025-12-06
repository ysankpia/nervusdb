import { describe, it, expect } from 'vitest';
import { allBenchmarkSuites } from '@/benchmark/suites.ts';

describe('Benchmark Suites · 导入即构建（不执行）', () => {
  it('allBenchmarkSuites 存在且为数组', () => {
    expect(Array.isArray(allBenchmarkSuites)).toBe(true);
    // 只验证定义，不执行真实基准用例
    expect(allBenchmarkSuites.length).toBeGreaterThan(0);
  });
});

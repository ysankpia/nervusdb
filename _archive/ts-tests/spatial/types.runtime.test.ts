/**
 * 空间类型运行时守卫测试
 *
 * 覆盖 src/spatial/types.ts 中的 isBoundingBox/assertBoundingBox，
 * 避免类型文件在覆盖率中计为 0%。
 */

import { describe, it, expect } from 'vitest';
import { assertBoundingBox, isBoundingBox } from '@/extensions/spatial/types';

describe('空间类型运行时守卫', () => {
  it('isBoundingBox: 非数组或长度不匹配时返回 false', () => {
    expect(isBoundingBox(null)).toBe(false);
    expect(isBoundingBox(undefined)).toBe(false);
    expect(isBoundingBox(123)).toBe(false);
    expect(isBoundingBox('bbox')).toBe(false);
    expect(isBoundingBox([1, 2, 3])).toBe(false);
    expect(isBoundingBox([1, 2, 3, '4'])).toBe(false);
    expect(isBoundingBox([1, Number.POSITIVE_INFINITY, 3, 4])).toBe(false);
  });

  it('isBoundingBox: 正确的四元数值数组返回 true', () => {
    expect(isBoundingBox([0, 1, 2, 3])).toBe(true);
    expect(isBoundingBox([-180, -90, 180, 90])).toBe(true);
  });

  it('assertBoundingBox: 非法输入抛出 TypeError，合法输入不抛', () => {
    expect(() => assertBoundingBox([1, 2, 3])).toThrowError(TypeError);
    expect(() => assertBoundingBox([0, 0, 1, 1])).not.toThrow();
  });
});

/**
 * 类型增强模块运行时守卫测试
 */

import { describe, it, expect } from 'vitest';
import { assertTypedPropertyFilter, isTypedPropertyFilter } from '@/types/enhanced';

describe('TypedPropertyFilter 运行时守卫', () => {
  it('isTypedPropertyFilter: 非法输入返回 false', () => {
    expect(isTypedPropertyFilter(null)).toBe(false);
    expect(isTypedPropertyFilter(undefined)).toBe(false);
    expect(isTypedPropertyFilter(123)).toBe(false);
    expect(isTypedPropertyFilter({})).toBe(false);
    expect(isTypedPropertyFilter({ propertyName: 1 })).toBe(false);
    expect(isTypedPropertyFilter({ propertyName: 'name', range: { includeMin: 'yes' } })).toBe(
      false,
    );
  });

  it('isTypedPropertyFilter: 合法输入返回 true', () => {
    expect(isTypedPropertyFilter({ propertyName: 'name' })).toBe(true);
    expect(
      isTypedPropertyFilter({
        propertyName: 'score',
        range: { min: 1, max: 10, includeMin: true, includeMax: false },
      }),
    ).toBe(true);
  });

  it('assertTypedPropertyFilter: 非合法输入抛出 TypeError，合法输入不抛', () => {
    expect(() => assertTypedPropertyFilter({ propertyName: 1 })).toThrowError(TypeError);
    expect(() => assertTypedPropertyFilter({ propertyName: 'age' })).not.toThrow();
  });
});

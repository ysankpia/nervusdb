/**
 * NervusDB 打开选项运行时守卫测试
 */

import { describe, it, expect } from 'vitest';
import { assertNervusDBOpenOptions, isNervusDBOpenOptions } from '@/types/openOptions';

describe('NervusDB 打开选项运行时守卫', () => {
  it('isNervusDBOpenOptions: 非对象输入返回 false', () => {
    expect(isNervusDBOpenOptions(null)).toBe(false);
    expect(isNervusDBOpenOptions(undefined)).toBe(false);
    expect(isNervusDBOpenOptions(123)).toBe(false);
    expect(isNervusDBOpenOptions('options')).toBe(false);
  });

  it('isNervusDBOpenOptions: 检查字段约束', () => {
    expect(isNervusDBOpenOptions({ enableLock: 'yes' })).toBe(false);
    expect(isNervusDBOpenOptions({ registerReader: 1 })).toBe(false);
    expect(isNervusDBOpenOptions({ experimental: null })).toBe(false);
    expect(isNervusDBOpenOptions({ experimental: { cypher: 'yes' } })).toBe(false);
  });

  it('isNervusDBOpenOptions: 合法输入返回 true', () => {
    expect(isNervusDBOpenOptions({})).toBe(true);
    expect(
      isNervusDBOpenOptions({
        enableLock: true,
        registerReader: true,
        experimental: { cypher: true },
      }),
    ).toBe(true);
  });

  it('assertNervusDBOpenOptions: 非法输入抛出 TypeError，合法输入不抛', () => {
    expect(() => assertNervusDBOpenOptions({ experimental: null })).toThrowError(TypeError);
    expect(() => assertNervusDBOpenOptions({ enableLock: true })).not.toThrow();
  });
});

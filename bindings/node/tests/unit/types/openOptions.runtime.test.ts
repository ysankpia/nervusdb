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
    expect(isNervusDBOpenOptions({ pageSize: 0 })).toBe(false);
    expect(isNervusDBOpenOptions({ pageSize: 1, compression: { codec: 'invalid' } })).toBe(false);
    expect(isNervusDBOpenOptions({ stagingMode: 'unknown' })).toBe(false);
    expect(isNervusDBOpenOptions({ enablePersistentTxDedupe: true, maxRememberTxIds: 50 })).toBe(
      false,
    );
  });

  it('isNervusDBOpenOptions: 合法输入返回 true', () => {
    expect(isNervusDBOpenOptions({})).toBe(true);
    expect(
      isNervusDBOpenOptions({
        indexDirectory: '/tmp/index',
        pageSize: 2000,
        rebuildIndexes: false,
        compression: { codec: 'brotli', level: 5 },
        enableLock: true,
        registerReader: true,
        stagingMode: 'default',
        enablePersistentTxDedupe: false,
        maxRememberTxIds: 5000,
      }),
    ).toBe(true);
  });

  it('assertNervusDBOpenOptions: 非法输入抛出 TypeError，合法输入不抛', () => {
    expect(() => assertNervusDBOpenOptions({ pageSize: 0 })).toThrowError(TypeError);
    expect(() => assertNervusDBOpenOptions({ pageSize: 100 })).not.toThrow();
  });
});

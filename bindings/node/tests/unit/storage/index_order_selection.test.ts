import { describe, it, expect } from 'vitest';
import { getBestIndexKey } from '@/core/storage/tripleIndexes';

describe('索引选择策略（六序）', () => {
  it('s+p 命中 SPO', () => {
    expect(getBestIndexKey({ subjectId: 1, predicateId: 2 })).toBe('SPO');
  });
  it('s+o 命中 SOP', () => {
    expect(getBestIndexKey({ subjectId: 1, objectId: 3 })).toBe('SOP');
  });
  it('p+o 命中 POS', () => {
    expect(getBestIndexKey({ predicateId: 2, objectId: 3 })).toBe('POS');
  });
  it('仅 s 命中 SPO', () => {
    expect(getBestIndexKey({ subjectId: 1 })).toBe('SPO');
  });
  it('仅 p 命中 POS', () => {
    expect(getBestIndexKey({ predicateId: 2 })).toBe('POS');
  });
  it('仅 o 命中 OSP', () => {
    expect(getBestIndexKey({ objectId: 3 })).toBe('OSP');
  });
});

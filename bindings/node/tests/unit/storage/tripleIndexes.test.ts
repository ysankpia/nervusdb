import { describe, expect, it } from 'vitest';

import { TripleIndexes } from '@/core/storage/tripleIndexes';

const triples = [
  { subjectId: 2, predicateId: 1, objectId: 3 },
  { subjectId: 2, predicateId: 1, objectId: 4 },
  { subjectId: 1, predicateId: 2, objectId: 3 },
  { subjectId: 3, predicateId: 1, objectId: 2 },
];

describe('TripleIndexes 分桶索引', () => {
  it('基于主键分桶并按次级键排序', () => {
    const indexes = new TripleIndexes();
    triples.forEach((triple) => indexes.add(triple));

    const spo = indexes.get('SPO');
    expect(spo).toHaveLength(4);
    expect(spo[0].subjectId).toBe(1);
    expect(spo[1].subjectId).toBe(2);
    expect(spo[1].objectId).toBe(3);
    expect(spo[2].objectId).toBe(4);
  });

  it('查询时优先命中对应主键桶', () => {
    const indexes = new TripleIndexes();
    triples.forEach((triple) => indexes.add(triple));

    const results = indexes.query({ subjectId: 2, predicateId: 1 });
    expect(results).toHaveLength(2);
    expect(results.every((item) => item.subjectId === 2)).toBe(true);
  });

  it('序列化与反序列化恢复索引结构', () => {
    const indexes = new TripleIndexes();
    triples.forEach((triple) => indexes.add(triple));

    const buffer = indexes.serialize();
    const restored = TripleIndexes.deserialize(buffer);
    const results = restored.query({ predicateId: 1, objectId: 2 });

    expect(results).toHaveLength(1);
    expect(results[0]).toEqual({ subjectId: 3, predicateId: 1, objectId: 2 });
  });
});

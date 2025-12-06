import type { EncodedTriple, IndexOrder } from '../tripleIndexes.js';

export function primaryKey(order: IndexOrder): keyof EncodedTriple {
  return order === 'SPO' ? 'subjectId' : order === 'POS' ? 'predicateId' : 'objectId';
}

export function primarySelector(order: IndexOrder): (t: EncodedTriple) => number {
  if (order === 'SPO') return (t) => t.subjectId;
  if (order === 'POS') return (t) => t.predicateId;
  return (t) => t.objectId;
}

export function matchCriteria(t: EncodedTriple, criteria: Partial<EncodedTriple>): boolean {
  if (criteria.subjectId !== undefined && t.subjectId !== criteria.subjectId) return false;
  if (criteria.predicateId !== undefined && t.predicateId !== criteria.predicateId) return false;
  if (criteria.objectId !== undefined && t.objectId !== criteria.objectId) return false;
  return true;
}

export function encodeTripleKey({ subjectId, predicateId, objectId }: EncodedTriple): string {
  return `${subjectId}:${predicateId}:${objectId}`;
}

export function decodeTripleKey(key: string): {
  subjectId: number;
  predicateId: number;
  objectId: number;
} {
  const [s, p, o] = key.split(':').map((x) => Number(x));
  return { subjectId: s, predicateId: p, objectId: o };
}

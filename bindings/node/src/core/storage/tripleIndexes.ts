/**
 * Triple Index Types (v2.0 - Minimal)
 *
 * This file contains only type definitions for index orders.
 * Actual indexing is handled by the Rust core.
 */

export type IndexOrder = 'SPO' | 'SOP' | 'PSO' | 'POS' | 'OSP' | 'OPS';

export interface EncodedTriple {
  subjectId: number;
  predicateId: number;
  objectId: number;
}

/**
 * Select the best index order based on which triple components are known.
 * This is a simple heuristic for query optimization.
 */
export function getBestIndexKey(criteria: Partial<EncodedTriple>): IndexOrder {
  const { subjectId, predicateId, objectId } = criteria;

  // If all three are specified, any index works (default to SPO)
  if (subjectId !== undefined && predicateId !== undefined && objectId !== undefined) {
    return 'SPO';
  }

  // Two components specified
  if (subjectId !== undefined && predicateId !== undefined) return 'SPO';
  if (subjectId !== undefined && objectId !== undefined) return 'SOP';
  if (predicateId !== undefined && objectId !== undefined) return 'POS';

  // One component specified
  if (subjectId !== undefined) return 'SPO';
  if (predicateId !== undefined) return 'PSO';
  if (objectId !== undefined) return 'OSP';

  // No components specified (full scan)
  return 'SPO';
}

import { describe, expect, it, beforeEach, vi } from 'vitest';

const mocks = vi.hoisted(() => {
  const cypherSupport = {
    cypher: vi.fn(),
    cypherRead: vi.fn(),
    validateCypher: vi.fn(),
    clearOptimizationCache: vi.fn(),
    getOptimizerStats: vi.fn(),
    warmUpOptimizer: vi.fn(),
  };
  return {
    cypherSupport,
    createCypherSupport: vi.fn(() => cypherSupport),
    warnExperimental: vi.fn(),
    variableBuilderAll: vi.fn(),
    variableBuilderCtor: vi.fn().mockImplementation(() => ({
      all: () => mocks.variableBuilderAll(),
    })),
  };
});

vi.mock('../../../src/extensions/query/cypher.js', () => ({
  createCypherSupport: mocks.createCypherSupport,
}));

vi.mock('../../../src/utils/experimental.js', () => ({
  warnExperimental: (...args: unknown[]) => mocks.warnExperimental(...args),
}));

vi.mock('../../../src/extensions/query/path/variable.js', () => ({
  VariablePathBuilder: mocks.variableBuilderCtor,
}));

import { CypherPlugin } from '../../../src/plugins/cypher.js';

describe('CypherPlugin', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.cypherSupport.cypher.mockResolvedValue({ rows: [] });
    mocks.cypherSupport.cypherRead.mockResolvedValue({ rows: [] });
    mocks.cypherSupport.validateCypher.mockReturnValue({ valid: true, errors: [] });
    mocks.cypherSupport.getOptimizerStats.mockReturnValue({ hit: 0 });
    mocks.cypherSupport.warmUpOptimizer.mockResolvedValue(undefined);
  });

  it('lazily initialises cypher support and delegates queries', async () => {
    const plugin = new CypherPlugin();
    const fakeDb = {
      find: vi.fn(() => ({ all: () => [] })),
    } as any;
    const fakeStore = {} as any;
    plugin.initialize(fakeDb, fakeStore);

    await plugin.cypherQuery('MATCH (n) RETURN n', { foo: 'bar' });
    await plugin.cypherRead('MATCH (n) RETURN n');
    const validation = plugin.validateCypher('RETURN 1');
    plugin.clearCypherOptimizationCache();
    const stats = plugin.getCypherOptimizerStats();
    await plugin.warmUpCypherOptimizer();

    expect(mocks.createCypherSupport).toHaveBeenCalledTimes(1);
    expect(mocks.cypherSupport.cypher).toHaveBeenCalledWith(
      'MATCH (n) RETURN n',
      { foo: 'bar' },
      {},
    );
    expect(mocks.cypherSupport.cypherRead).toHaveBeenCalled();
    expect(validation.valid).toBe(true);
    expect(stats).toEqual({ hit: 0 });
  });

  it('evaluates simplified MATCH pattern without variable length', () => {
    const plugin = new CypherPlugin();
    const fakeDb = {
      find: vi.fn(() => ({
        all: () => [
          { subject: 'Alice', object: 'Bob' },
          { subject: 'Charlie', object: 'Dylan' },
        ],
      })),
    } as any;
    plugin.initialize(fakeDb, {} as any);

    const result = plugin.cypherSimple('MATCH (a)-[:KNOWS]->(b) RETURN a,b');
    expect(result).toEqual([
      { a: 'Alice', b: 'Bob' },
      { a: 'Charlie', b: 'Dylan' },
    ]);
  });

  it('evaluates variable length paths via VariablePathBuilder', () => {
    const plugin = new CypherPlugin();
    const fakeDb = {
      find: vi.fn(() => ({
        all: () => [
          { subjectId: 1, predicateId: 10, objectId: 2 },
          { subjectId: 3, predicateId: 10, objectId: 4 },
        ],
      })),
    } as any;
    const names: Record<number, string> = { 1: 'Alice', 2: 'Bob', 3: 'Charlie', 4: 'Dylan' };
    const fakeStore = {
      getNodeIdByValue: vi.fn(() => 10),
      getNodeValueById: vi.fn((id: number) => names[id] ?? null),
    } as any;
    plugin.initialize(fakeDb, fakeStore);

    mocks.variableBuilderAll.mockReturnValue([
      { startId: 1, endId: 2 },
      { startId: 3, endId: 4 },
    ]);

    const result = plugin.cypherSimple('MATCH (a)-[:KNOWS*1..3]->(b) RETURN a,b');
    expect(fakeStore.getNodeIdByValue).toHaveBeenCalledWith('KNOWS');
    expect(mocks.variableBuilderCtor).toHaveBeenCalledWith(
      fakeStore,
      new Set([1, 3]),
      10,
      expect.objectContaining({ min: 1, max: 3 }),
    );
    expect(result).toHaveLength(2);
  });
});

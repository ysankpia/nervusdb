import { describe, expect, it, beforeEach, vi } from 'vitest';

const mocks = vi.hoisted(() => {
  return {
    readPagedManifest: vi.fn(),
    readHotness: vi.fn(),
    compactDatabase: vi.fn(),
    garbageCollectPages: vi.fn(),
    getActiveReaders: vi.fn(),
    readFile: vi.fn(),
  };
});

vi.mock('../../../src/core/storage/pagedIndex.js', () => ({
  readPagedManifest: mocks.readPagedManifest,
}));

vi.mock('../../../src/core/storage/hotness.js', () => ({
  readHotness: mocks.readHotness,
}));

vi.mock('../../../src/maintenance/compaction.js', () => ({
  compactDatabase: mocks.compactDatabase,
}));

vi.mock('../../../src/maintenance/gc.js', () => ({
  garbageCollectPages: mocks.garbageCollectPages,
}));

vi.mock('../../../src/core/storage/readerRegistry.js', () => ({
  getActiveReaders: mocks.getActiveReaders,
}));

vi.mock('node:fs', () => ({
  promises: {
    readFile: mocks.readFile,
    writeFile: vi.fn(),
    mkdir: vi.fn(),
  },
}));

import { autoCompact } from '../../../src/maintenance/autoCompact.js';

describe('autoCompact analysis', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('skips compaction when active readers exist', async () => {
    mocks.readPagedManifest.mockResolvedValue({
      lookups: [],
      pageSize: 2,
      tombstones: [],
      compression: { codec: 'none' },
    });
    mocks.getActiveReaders.mockResolvedValue([{}]);
    const decision = await autoCompact('/tmp/mockdb', { respectReaders: true });
    expect(decision.skipped).toBe(true);
    expect(decision.reason).toBe('active_readers');
    expect(decision.selectedOrders).toEqual([]);
    expect(mocks.compactDatabase).not.toHaveBeenCalled();
  });

  it('selects hot primaries, includes LSM segments and runs compaction', async () => {
    const manifest = {
      lookups: [
        {
          order: 'SPO',
          pages: [{ primaryValue: 1 }, { primaryValue: 1 }, { primaryValue: 2 }],
        },
      ],
      pageSize: 2,
      tombstones: [],
      compression: { codec: 'none' },
    };
    mocks.readPagedManifest.mockResolvedValue(manifest);
    mocks.getActiveReaders.mockResolvedValue([]);
    mocks.readHotness.mockResolvedValue({
      updatedAt: Date.now(),
      counts: { SPO: { '1': 5 } },
    });
    mocks.readFile.mockResolvedValueOnce(
      Buffer.from(JSON.stringify({ segments: [{ count: 5 }, { count: 5 }] })),
    );
    mocks.compactDatabase.mockResolvedValue({
      pagesBefore: 3,
      pagesAfter: 1,
      primariesMerged: 1,
      ordersRewritten: ['SPO'],
      removedByTombstones: 0,
    });

    const decision = await autoCompact('/tmp/mockdb', {
      dryRun: false,
      mode: 'incremental',
      hotThreshold: 1,
      scoreWeights: { hot: 1, pages: 1, tomb: 0.5 },
      includeLsmSegmentsAuto: true,
      autoGC: true,
    });

    expect(decision.selectedOrders).toEqual(['SPO']);
    expect(mocks.compactDatabase).toHaveBeenCalledTimes(1);
    const [, compactOpts] = mocks.compactDatabase.mock.calls[0];
    expect(compactOpts?.includeLsmSegments).toBe(true);
    expect(mocks.garbageCollectPages).toHaveBeenCalledWith('/tmp/mockdb', { dryRun: false });
  });
});

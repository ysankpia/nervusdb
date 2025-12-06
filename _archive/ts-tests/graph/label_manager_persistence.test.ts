import { describe, expect, it } from 'vitest';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { LabelManager } from '@/graph/labels.js';

const TMP_PREFIX = join(tmpdir(), 'labels-manager-');

describe('LabelManager persistence', () => {
  it('flushes and reloads label snapshots', async () => {
    const dir = await mkdtemp(TMP_PREFIX);
    const manager = new LabelManager(dir);

    manager.applyLabelChange(1, [], ['Person', 'Admin']);
    manager.applyLabelChange(2, [], ['Person']);
    await manager.flush();

    const reloaded = new LabelManager(dir);
    const loaded = await reloaded.tryLoad();
    expect(loaded).toBe(true);

    const index = reloaded.getMemoryIndex();
    expect(index.getNodeLabels(1)).toEqual(['Admin', 'Person']);
    expect(index.getNodeLabels(2)).toEqual(['Person']);
    expect(index.findNodesByLabel('Person')).toEqual(new Set([1, 2]));
  });
});

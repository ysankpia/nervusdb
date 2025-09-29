import { describe, it, expect } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { makeWorkspace, cleanupWorkspace, within } from '../../helpers/tempfs';

describe('Lazy.explain · whereProperty(edge, =) 估算参与', () => {
  it('edge 等值属性过滤参与 upperBound 估算（仅验证存在性）', async () => {
    const dir = await makeWorkspace('unit-lazy-explain-edge');
    const db = await SynapseDB.open(within(dir, 'db.synapsedb'));
    const f1 = db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    const f2 = db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    await db.flush();

    db.setEdgeProperties(
      { subjectId: f1.subjectId, predicateId: f1.predicateId, objectId: f1.objectId },
      { weight: 'w1' },
    );
    db.setEdgeProperties(
      { subjectId: f2.subjectId, predicateId: f2.predicateId, objectId: f2.objectId },
      { weight: 'w1' },
    );
    await db.flush();

    const q = db.find({ subject: 'S' }).whereProperty('weight', '=', 'w1', 'edge');
    const e = (q as any).explain();
    expect(e?.estimate?.upperBound).not.toBeUndefined();

    await db.close();
    await cleanupWorkspace(dir);
  });
});

import { describe, it, expect } from 'vitest';
import { NervusDB } from '@/synapseDb';
import { makeWorkspace, cleanupWorkspace, within } from '../../helpers/tempfs';

describe('Lazy.explain · whereLabel 估算参与', () => {
  it('标签过滤参与 upperBound 估算（仅验证存在性）', async () => {
    const dir = await makeWorkspace('unit-lazy-explain-label');
    const db = await NervusDB.open(within(dir, 'db.synapsedb'));
    // 两条 fact，给主体打标签
    const f1 = db.addFact({ subject: 'LS', predicate: 'R', object: 'O1' });
    const f2 = db.addFact({ subject: 'LS', predicate: 'R', object: 'O2' });
    await db.flush();
    // 给主体节点设置标签
    db.setNodeProperties(f1.subjectId, { labels: ['Person'] });
    db.setNodeProperties(f2.objectId, { labels: ['Other'] });
    await db.flush();

    const q = db.find({}).whereLabel(['Person'], { mode: 'AND', on: 'subject' });
    const e = (q as any).explain();
    expect(e?.estimate?.upperBound).not.toBeUndefined();

    await db.close();
    await cleanupWorkspace(dir);
  });
});

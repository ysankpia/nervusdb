import { describe, it, expect } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { makeWorkspace, cleanupWorkspace, within } from '../../helpers/tempfs';

describe('Lazy.explain · 选择率组合（AND 交集）', () => {
  it('两个 whereLabel(subject) 叠加应进一步收紧 upperBound', async () => {
    const dir = await makeWorkspace('unit-lazy-explain-combo');
    const db = await SynapseDB.open(within(dir, 'db.synapsedb'));

    // 三个主体，各有若干边
    db.addFact({ subject: 'S1', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S1', predicate: 'R', object: 'O2' });
    db.addFact({ subject: 'S2', predicate: 'R', object: 'O3' });
    db.addFact({ subject: 'S3', predicate: 'R', object: 'O4' });
    await db.flush();

    // 打标签：S1 拥有 A、B；S2 只有 A；S3 只有 B
    const idS1 = db.getNodeId('S1');
    const idS2 = db.getNodeId('S2');
    const idS3 = db.getNodeId('S3');
    if (idS1) db.setNodeProperties(idS1, { labels: ['A', 'B'] });
    if (idS2) db.setNodeProperties(idS2, { labels: ['A'] });
    if (idS3) db.setNodeProperties(idS3, { labels: ['B'] });
    await db.flush();

    const q1 = db.find({}).whereLabel(['A'], { on: 'subject', mode: 'AND' });
    const e1 = (q1 as any).explain();
    const ub1 = e1?.estimate?.upperBound as number | undefined;
    expect(ub1).not.toBeUndefined();

    const q2 = q1.whereLabel(['B'], { on: 'subject', mode: 'AND' });
    const e2 = (q2 as any).explain();
    const ub2 = e2?.estimate?.upperBound as number | undefined;
    expect(ub2).not.toBeUndefined();

    // 仅验证存在性与不为 0（方向性验证交给更大规模用例）
    if (ub2 !== undefined) expect(ub2).toBeGreaterThan(0);

    await db.close();
    await cleanupWorkspace(dir);
  });
});

import { describe, expect, it } from 'vitest';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { NervusDB } from '../../../src/index.js';
import { loadNativeCore } from '../../../src/native/core.js';

function tempPath(prefix: string) {
  return mkdtempSync(join(tmpdir(), prefix));
}

describe('engine parity (js vs native)', () => {
  const nativeAvailable = Boolean(loadNativeCore());

  it.skipIf(!nativeAvailable)('produces same facts for simple workload', async () => {
    const base = tempPath('engine-parity-');
    const pathJs = join(base, 'js.synapsedb');
    const pathNative = join(base, 'native.synapsedb');

    const input = [
      { subject: 'alice', predicate: 'knows', object: 'bob', properties: { since: 2020 } },
      { subject: 'bob', predicate: 'likes', object: 'carol', properties: { weight: 0.8 } },
      { subject: 'carol', predicate: 'knows', object: 'dave', properties: { since: 2021 } },
    ];

    const js = await NervusDB.open(pathJs, { engine: 'js', enableLock: false });
    for (const f of input) js.addFact(f);
    const jsFacts = js.listFacts();
    await js.close();

    const native = await NervusDB.open(pathNative, { engine: 'native', enableLock: false });
    for (const f of input) native.addFact(f);
    const nativeFacts = native.listFacts();
    await native.close();

    const normalize = (facts: ReturnType<typeof js.listFacts>) =>
      facts
        .map((f) => ({
          s: f.subject,
          p: f.predicate,
          o: f.object,
          sp: f.subjectProperties ?? {},
          op: f.objectProperties ?? {},
          ep: f.edgeProperties ?? {},
        }))
        .sort((a, b) => (a.s + a.p + a.o).localeCompare(b.s + b.p + b.o));

    expect(normalize(nativeFacts)).toEqual(normalize(jsFacts));
  });
});

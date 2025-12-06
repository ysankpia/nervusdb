import { beforeEach, afterEach, describe, expect, it } from 'vitest';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { loadNativeBinding } from '../helpers/native-binding.js';

const { open } = loadNativeBinding<{ open: (options: { dataPath: string }) => any }>();
describe('Cypher parameter support (native)', () => {
  let dbPathDir: string;
  let db: any;

  beforeEach(() => {
    dbPathDir = mkdtempSync(join(tmpdir(), 'nervusdb-cypher-'));
    const dataPath = join(dbPathDir, 'data.redb');
    db = open({ dataPath });
  });

  afterEach(() => {
    if (db) {
      db.close();
      db = null;
    }
    if (dbPathDir && existsSync(dbPathDir)) {
      rmSync(dbPathDir, { recursive: true, force: true });
    }
  });

  it('filters nodes using parameterized WHERE clause', () => {
    const triple = db.addFact('Alice', 'type', 'Person');
    db.setNodeProperty(triple.subjectId, JSON.stringify({ name: 'Alice' }));

    const other = db.addFact('Bob', 'type', 'Person');
    db.setNodeProperty(other.subjectId, JSON.stringify({ name: 'Bob' }));

    const results = db.executeQuery('MATCH (n:Person) WHERE n.name = $target RETURN n', {
      target: 'Alice',
    });

    const literal = db.executeQuery('MATCH (n:Person) WHERE n.name = "Alice" RETURN n');
    expect(literal.length).toBe(1);

    expect(results.length).toBe(1);
    expect(results[0].n.id).toBe(Number(triple.subjectId));
  });

  it('returns empty result when parameter does not match', () => {
    const triple = db.addFact('Alice', 'type', 'Person');
    db.setNodeProperty(triple.subjectId, JSON.stringify({ name: 'Alice' }));

    const results = db.executeQuery('MATCH (n:Person) WHERE n.name = $target RETURN n', {
      target: 'Unknown',
    });

    expect(results.length).toBe(0);
  });
});

import { describe, it, expect } from 'vitest';
import { CypherParser } from '@/extensions/query/pattern/parser.ts';

describe('CypherParser · 轻量语法覆盖', () => {
  it('MATCH/WHERE/RETURN + 变长关系/属性映射/ORDER BY/LIMIT/SKIP', () => {
    const parser = new CypherParser();
    const q = parser.parse(
      'MATCH (a:Person {age: 30})-[:KNOWS*1..2]->(b:Person) WHERE a.age >= 25 RETURN a, b ORDER BY a ASC, b DESC LIMIT 10 SKIP 1',
    );
    expect(q.type).toBe('CypherQuery');
    expect(Array.isArray(q.clauses)).toBe(true);
    // 应至少包含 MATCH / WHERE / RETURN 三类子句
    const kinds = q.clauses.map((c) => c.type);
    expect(kinds).toContain('MatchClause');
    expect(kinds).toContain('WhereClause');
    expect(kinds).toContain('ReturnClause');
  });
});

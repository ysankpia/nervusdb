import { describe, it, expect } from 'vitest';
import { CypherParser } from '@/extensions/query/pattern/parser.ts';

describe('CypherParser · 多子句分支覆盖', () => {
  const p = new CypherParser();

  it('CREATE/RETURN 与 MERGE/ON CREATE SET', () => {
    const q1 = p.parse('CREATE (n:Person {name: "Alice"}) RETURN n');
    expect(q1.clauses.some((c) => c.type === 'CreateClause')).toBe(true);
    expect(q1.clauses.some((c) => c.type === 'ReturnClause')).toBe(true);

    const q2 = p.parse('MERGE (n:Person {name: "Bob"}) ON CREATE SET n.age = 30 RETURN n');
    expect(q2.clauses.some((c) => c.type === 'MergeClause')).toBe(true);
  });

  it('DELETE/DETACH DELETE/REMOVE 属性与标签', () => {
    const q1 = p.parse('DELETE n');
    expect(q1.clauses.some((c) => c.type === 'DeleteClause')).toBe(true);

    const q2 = p.parse('DETACH DELETE n');
    expect(q2.clauses.some((c) => c.type === 'DeleteClause')).toBe(true);

    const q3 = p.parse('REMOVE n:Old:Legacy, m.prop');
    expect(q3.clauses.some((c) => c.type === 'RemoveClause')).toBe(true);
  });

  it('WITH/UNWIND/UNION', () => {
    const q1 = p.parse('WITH 1 RETURN 1');
    expect(q1.clauses.some((c) => c.type === 'WithClause')).toBe(true);

    const q2 = p.parse('UNWIND [1,2,3] AS x RETURN x');
    expect(q2.clauses.some((c) => c.type === 'UnwindClause')).toBe(true);

    const q3 = p.parse('MATCH (a) RETURN a UNION ALL RETURN 1');
    expect(q3.clauses.some((c) => c.type === 'MatchClause')).toBe(true);
    // UnionClause 是末尾另一个查询，当前 parse() 仅返回第一部分，但内部 parseUnion 覆盖到
  });
});

import { describe, it, expect } from 'vitest';
import { createNode, type Literal } from '@/extensions/query/pattern/ast.ts';

describe('Cypher 模式 AST · createNode', () => {
  it('createNode 应正确创建带位置信息的节点', () => {
    const lit = createNode<Literal>('Literal', { value: 1, raw: '1' } as any, {
      start: { line: 1, column: 1, offset: 0 },
      end: { line: 1, column: 2, offset: 1 },
    });
    expect(lit.type).toBe('Literal');
    expect(lit.raw).toBe('1');
    expect(lit.location?.start.line).toBe(1);
  });
});

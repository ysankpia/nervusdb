import { describe, it, expect } from 'vitest';
import { PatternBuilder } from '@/extensions/query/pattern/match.ts';

type Triple = { subjectId: number; predicateId: number; objectId: number };

// 伪造最小持久层：仅满足 PatternBuilder 所需接口
function makeStore() {
  // 节点值
  const nodeValue = (id: number) => `n${id}`;
  // 谓词字典
  const PRED: Record<string, number> = { TYPE: 100, OTHER: 101 };
  const predVal: Record<number, string> = { 100: 'TYPE', 101: 'OTHER' };

  // 三元组集合
  const triples: Triple[] = [
    { subjectId: 1, predicateId: PRED.TYPE, objectId: 2 },
    { subjectId: 2, predicateId: PRED.TYPE, objectId: 3 },
    { subjectId: 3, predicateId: PRED.OTHER, objectId: 4 },
    { subjectId: 3, predicateId: PRED.TYPE, objectId: 4 }, // 用于范围用例
  ];

  // 标签索引
  const labels: Record<string, Set<number>> = {
    A: new Set([1, 2]),
    B: new Set([2, 3]),
  };

  // 属性索引（等值与范围）
  const propsEq: Record<string, Map<unknown, Set<number>>> = {
    age: new Map<unknown, Set<number>>([
      [30, new Set([2])],
      [40, new Set([3])],
    ]),
  };
  const propsRange: Record<
    string,
    (min?: unknown, max?: unknown, incMin?: boolean, incMax?: boolean) => Set<number>
  > = {
    age: (min, max, incMin, incMax) => {
      const toNum = (v: unknown) => (typeof v === 'number' ? v : Number(v));
      const minN = min === undefined ? -Infinity : toNum(min);
      const maxN = max === undefined ? Infinity : toNum(max);
      const s = new Set<number>();
      const entries: Array<[number, number]> = [
        [2, 30],
        [3, 40],
      ];
      for (const [id, val] of entries) {
        const okMin = incMin ? val >= minN : val > minN;
        const okMax = incMax ? val <= maxN : val < maxN;
        if (okMin && okMax) s.add(id);
      }
      return s;
    },
  } as any;

  return {
    getLabelIndex() {
      return {
        findNodesByLabels(ls: string[], _opts: any) {
          // AND 模式
          if (ls.length === 0) return new Set<number>();
          let acc = new Set(labels[ls[0]] ?? []);
          for (let i = 1; i < ls.length; i++) {
            const cur = labels[ls[i]] ?? new Set<number>();
            acc = new Set([...acc].filter((x) => cur.has(x)));
          }
          return acc;
        },
      };
    },
    getPropertyIndex() {
      return {
        queryNodesByProperty(name: string, value: unknown) {
          const m = propsEq[name];
          if (!m) return new Set<number>();
          return new Set(m.get(value) ?? []);
        },
        queryNodesByRange(
          name: string,
          min?: unknown,
          max?: unknown,
          incMin?: boolean,
          incMax?: boolean,
        ) {
          const f = propsRange[name];
          if (!f) return new Set<number>();
          return f(min, max, incMin, incMax);
        },
      };
    },
    // 简化：query 返回数组，resolveRecords 直接返回输入
    query(criteria: Partial<Triple>) {
      const keys = Object.keys(criteria) as (keyof Triple)[];
      return triples.filter((t) => keys.every((k) => (criteria as any)[k] === (t as any)[k]));
    },
    resolveRecords(records: Triple[]) {
      return records;
    },
    getNodeIdByValue(v: string) {
      return PRED[v];
    },
    getNodeValueById(id: number) {
      return predVal[id] ?? nodeValue(id);
    },
  } as any;
}

describe('PatternBuilder · 基础路径', () => {
  it('标签起始 + 固定一跳 -> 返回指定别名', async () => {
    const store = makeStore();
    const pb = new PatternBuilder(store)
      .node('a', ['A'])
      .edge('->', 'TYPE')
      .node('b', ['B'])
      .return(['a', 'b']);
    const out = await pb.execute();
    // 预期 (1)-TYPE->(2) 命中，(2) 属于 B
    expect(out.length).toBeGreaterThan(0);
    const row = out[0] as any;
    expect(row.a).toBe('n1');
    expect(row.b).toBe('n2');
  });

  it('属性过滤（>=）作为起始 + 固定一跳', async () => {
    const store = makeStore();
    const pb = new PatternBuilder(store)
      .node('a')
      .whereNodeProperty('a', 'age', '>=', 35)
      .edge('->', 'TYPE')
      .node('b')
      .return(['a', 'b']);
    const out = await pb.execute();
    // 变体：3(age40) -TYPE-> 4
    const row = out.find((r) => r.a === 'n3');
    expect(row).toBeDefined();
    expect((row as any).b).toBe('n4');
  });

  it('反向 <- 一跳与默认返回（所有别名）', async () => {
    const store = makeStore();
    const pb = new PatternBuilder(store).node('x', ['B']).edge('<-', 'TYPE').node('y', ['A']);
    const out = await pb.execute();
    // (1)-TYPE->(2)，反向匹配得到 (x=2,y=1)
    const row = out.find((r) => r.x === 'n2' && r.y === 'n1');
    expect(row).toBeDefined();
  });

  it('属性等值过滤（=）与多过滤组合', async () => {
    const store = makeStore();
    const pb = new PatternBuilder(store)
      .node('a')
      .whereNodeProperty('a', 'age', '=', 30)
      .whereNodeProperty('a', 'age', '>=', 20)
      .edge('->', 'TYPE')
      .node('b', ['B']);
    const out = await pb.execute();
    // a 应为节点2（age=30），b 可为 3（2->3）
    const row = out.find((r) => r.a === 'n2');
    expect(row).toBeDefined();
  });

  it('无标签且无属性起始 → all() 启动 + 无谓词单跳', async () => {
    const store = makeStore();
    const pb = new PatternBuilder(store)
      .node('a') // 无标签/属性，触发 all 查询启动
      .edge('->') // 未指定关系类型
      .node('b', ['B']);
    const out = await pb.execute();
    // 仍应能命中 (1)->(2) 或 (2)->(3)
    expect(out.length).toBeGreaterThan(0);
  });
});

import { describe, it, expect } from 'vitest';
import { CypherQueryExecutor } from '@/extensions/query/pattern/executor.ts';
import type { IndexScanPlan, FilterPlan, LimitPlan } from '@/extensions/query/pattern/planner.ts';

describe('CypherQueryExecutor · IndexScan/Filter/Limit 基础路径', () => {
  it('label 索引扫描 + 属性过滤 + limit', async () => {
    // 伪造最小 store
    const store: any = {
      getLabelIndex() {
        return {
          findNodesByLabels(_labels: string[], _opts: any) {
            return new Set<number>([1, 2]);
          },
        };
      },
      getPropertyIndex() {
        return {
          queryNodesByProperty() {
            return new Set<number>();
          },
          queryNodesByRange() {
            return new Set<number>();
          },
        };
      },
      resolveRecords() {
        return [];
      },
      query() {
        return [];
      },
      getNodeProperties(id: number) {
        return id === 1 ? { age: 25 } : { age: 35 };
      },
      getNodeValueById(id: number) {
        return `node${id}`;
      },
      getNodeIdByValue() {
        return undefined;
      },
    };

    const scan: IndexScanPlan = {
      type: 'IndexScan',
      indexType: 'label',
      labels: ['Person'],
      variable: 'n',
      cost: 1,
      cardinality: 2,
      properties: {},
    };

    const filter: FilterPlan = {
      type: 'Filter',
      child: scan,
      condition: {
        type: 'BinaryExpression',
        operator: '>',
        left: {
          type: 'PropertyAccess',
          object: { type: 'Variable', name: 'n' },
          property: 'age',
        } as any,
        right: { type: 'Literal', value: 30, raw: '30' } as any,
      } as any,
      selectivity: 0.5,
      cost: 1,
      cardinality: 1,
      properties: {},
    };

    const limit: LimitPlan = {
      type: 'Limit',
      child: filter,
      limit: 1,
      cost: 1,
      cardinality: 1,
      properties: {},
    };

    const exec = new CypherQueryExecutor(store);
    const out = await exec.execute(limit);
    expect(out.length).toBe(1);
    expect(out[0]['n']).toBe('node2');
  });

  it('AND/OR 条件与笛卡尔积', async () => {
    const store: any = {
      getLabelIndex() {
        return {
          findNodesByLabels(labels: string[]) {
            if (labels[0] === 'A') return new Set<number>([2]);
            if (labels[0] === 'B') return new Set<number>([3]);
            return new Set<number>();
          },
        };
      },
      getPropertyIndex() {
        return {
          queryNodesByProperty() {
            return new Set<number>();
          },
          queryNodesByRange() {
            return new Set<number>();
          },
        };
      },
      resolveRecords() {
        return [];
      },
      query() {
        return [];
      },
      getNodeProperties(id: number) {
        return id === 2 ? { age: 40, score: 90 } : { age: 20, score: 50 };
      },
      getNodeValueById(id: number) {
        return `node${id}`;
      },
      getNodeIdByValue() {
        return undefined;
      },
    };

    const leftScan: IndexScanPlan = {
      type: 'IndexScan',
      indexType: 'label',
      labels: ['A'],
      variable: 'x',
      cost: 1,
      cardinality: 1,
      properties: {},
    };
    const rightScan: IndexScanPlan = {
      type: 'IndexScan',
      indexType: 'label',
      labels: ['B'],
      variable: 'y',
      cost: 1,
      cardinality: 1,
      properties: {},
    };

    const cart: any = {
      type: 'CartesianProduct',
      cost: 1,
      cardinality: 1,
      properties: { left: leftScan, right: rightScan },
    };

    const filter: FilterPlan = {
      type: 'Filter',
      child: cart,
      condition: {
        type: 'BinaryExpression',
        operator: 'AND',
        left: {
          type: 'BinaryExpression',
          operator: '>=',
          left: {
            type: 'PropertyAccess',
            object: { type: 'Variable', name: 'x' },
            property: 'age',
          } as any,
          right: { type: 'Literal', value: 30, raw: '30' } as any,
        } as any,
        right: {
          type: 'BinaryExpression',
          operator: '>',
          left: {
            type: 'PropertyAccess',
            object: { type: 'Variable', name: 'x' },
            property: 'score',
          } as any,
          right: { type: 'Literal', value: 80, raw: '80' } as any,
        } as any,
      } as any,
      selectivity: 0.5,
      cost: 1,
      cardinality: 1,
      properties: {},
    };

    const exec = new CypherQueryExecutor(store);
    const out = await exec.execute(filter);
    expect(out.length).toBe(1);
    // 变量 x 应绑定到 node2（满足 AND），y 绑定 node3（笛卡尔积）
    expect(out[0]['x']).toBe('node2');
    expect(out[0]['y']).toBe('node3');
  });

  it('IndexScan property/range 与 Project/Limit', async () => {
    const store: any = {
      getLabelIndex() {
        return {
          findNodesByLabels() {
            return new Set<number>();
          },
        };
      },
      getPropertyIndex() {
        return {
          queryNodesByProperty() {
            return new Set<number>([5]);
          },
          queryNodesByRange() {
            return new Set<number>([6, 7]);
          },
        };
      },
      resolveRecords() {
        return [];
      },
      query() {
        return [];
      },
      getNodeProperties() {
        return {};
      },
      getNodeValueById(id: number) {
        return `v${id}`;
      },
      getNodeIdByValue() {
        return undefined;
      },
    };

    const propScan: IndexScanPlan = {
      type: 'IndexScan',
      indexType: 'property',
      propertyName: 'age',
      propertyValue: 18,
      propertyOperator: '=',
      variable: 'p',
      cost: 1,
      cardinality: 1,
      properties: {},
    };
    const rangeScan: IndexScanPlan = {
      type: 'IndexScan',
      indexType: 'property',
      propertyName: 'score',
      propertyValue: 60,
      propertyOperator: '>=',
      variable: 'q',
      cost: 1,
      cardinality: 2,
      properties: {},
    };

    const proj: any = {
      type: 'Project',
      child: rangeScan,
      columns: ['q'],
      cost: 1,
      cardinality: 2,
      properties: {},
    } as import('@/extensions/query/pattern/planner.ts').ProjectPlan;
    const limit: LimitPlan = {
      type: 'Limit',
      child: proj,
      limit: 1,
      cost: 1,
      cardinality: 1,
      properties: {},
    };

    const exec = new CypherQueryExecutor(store);
    const r1 = await exec.execute(propScan);
    expect(r1.length).toBe(1);
    const r2 = await exec.execute(limit);
    expect(r2.length).toBe(1);
  });

  it('IndexScan full 扫描', async () => {
    const store: any = {
      getLabelIndex() {
        return {
          findNodesByLabels() {
            return new Set<number>();
          },
        };
      },
      getPropertyIndex() {
        return {
          queryNodesByProperty() {
            return new Set<number>();
          },
          queryNodesByRange() {
            return new Set<number>();
          },
        };
      },
      resolveRecords(records: any[]) {
        return records as any;
      },
      query() {
        return [
          { subjectId: 11, predicateId: 0, objectId: 0 },
          { subjectId: 12, predicateId: 0, objectId: 0 },
        ];
      },
      getNodeProperties() {
        return {};
      },
      getNodeValueById(id: number) {
        return `n${id}`;
      },
      getNodeIdByValue() {
        return undefined;
      },
    };

    const scan: IndexScanPlan = {
      type: 'IndexScan',
      indexType: 'full',
      variable: 'z',
      cost: 1,
      cardinality: 2,
      properties: {},
    } as any;
    const exec = new CypherQueryExecutor(store);
    const out = await exec.execute(scan);
    expect(out.length).toBe(2);
  });
});

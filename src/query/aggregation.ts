import { PersistentStore, FactRecord } from '../storage/persistentStore.js';
import { FactCriteria } from './queryBuilder.js';
import { buildFindContext } from './queryBuilder.js';

export interface AggregateResult {
  [key: string]: unknown;
}

type AggOp =
  | { kind: 'COUNT'; alias: string }
  | { kind: 'SUM'; field: string; alias: string }
  | { kind: 'AVG'; field: string; alias: string };

/**
 * 简易聚合管道：支持 match → groupBy → count/sum/avg → execute
 * 设计为内存聚合，输入规模过大时建议配合流式查询拆批聚合
 */
export class AggregationPipeline {
  private records: FactRecord[] = [];
  private groupFields: string[] = [];
  private ops: AggOp[] = [];
  private sortField?: string;
  private sortDir: 'ASC' | 'DESC' = 'ASC';
  private limitCount?: number;
  private streamCriteria?: FactCriteria;
  private streamBatchSize: number = 1000;

  constructor(private readonly store: PersistentStore) {}

  /**
   * 设定输入集合（来自 QueryBuilder）
   */
  from(records: FactRecord[]): this {
    this.records = records;
    return this;
  }

  /**
   * 使用三元组条件匹配输入集合
   */
  match(criteria: FactCriteria): this {
    const ctx = buildFindContext(this.store, criteria, 'both');
    this.records = ctx.facts;
    return this;
  }

  /**
   * 流式匹配：不预先加载全部记录，适合大数据集
   */
  matchStream(criteria: FactCriteria, options?: { batchSize?: number }): this {
    this.streamCriteria = criteria;
    if (options?.batchSize && options.batchSize > 0) this.streamBatchSize = options.batchSize;
    return this;
  }

  groupBy(fields: string[]): this {
    this.groupFields = fields;
    return this;
  }

  count(alias = 'count'): this {
    this.ops.push({ kind: 'COUNT', alias });
    return this;
  }

  sum(field: string, alias: string): this {
    this.ops.push({ kind: 'SUM', field, alias });
    return this;
  }

  avg(field: string, alias: string): this {
    this.ops.push({ kind: 'AVG', field, alias });
    return this;
  }

  min(field: string, alias: string): this {
    // 以 SUM/AVG 扩展思路实现极值
    this.ops.push({ kind: 'SUM', field: `__MIN__:${field}`, alias });
    return this;
  }

  max(field: string, alias: string): this {
    this.ops.push({ kind: 'SUM', field: `__MAX__:${field}`, alias });
    return this;
  }

  orderBy(field: string, direction: 'ASC' | 'DESC'): this {
    this.sortField = field;
    this.sortDir = direction;
    return this;
  }

  limit(n: number): this {
    this.limitCount = Math.max(0, n | 0);
    return this;
  }

  execute(): AggregateResult[] {
    const groups = new Map<string, { keys: Record<string, unknown>; rows: FactRecord[] }>();
    const gf = this.groupFields.length > 0 ? this.groupFields : ['_all'];

    for (const r of this.records) {
      const keyObj: Record<string, unknown> = {};
      for (const f of gf) {
        if (f === '_all') keyObj['_all'] = 'all';
        else keyObj[f] = getField(r, f);
      }
      const keyStr = JSON.stringify(keyObj);
      const g = groups.get(keyStr) ?? { keys: keyObj, rows: [] };
      g.rows.push(r);
      groups.set(keyStr, g);
    }

    const results: AggregateResult[] = [];
    for (const { keys, rows } of groups.values()) {
      const out: AggregateResult = { ...keys };
      for (const op of this.ops) {
        if (op.kind === 'COUNT') {
          out[op.alias] = rows.length;
        } else if (op.kind === 'SUM') {
          // MIN/MAX 伪操作符：用特殊前缀区分
          if (op.field.startsWith('__MIN__:')) {
            const real = op.field.replace('__MIN__:', '');
            let mv: number | undefined;
            for (const r of rows) {
              const v = Number(getField(r, real));
              if (!Number.isNaN(v)) mv = mv === undefined ? v : Math.min(mv, v);
            }
            out[op.alias] = mv ?? null;
            continue;
          }
          if (op.field.startsWith('__MAX__:')) {
            const real = op.field.replace('__MAX__:', '');
            let mv: number | undefined;
            for (const r of rows) {
              const v = Number(getField(r, real));
              if (!Number.isNaN(v)) mv = mv === undefined ? v : Math.max(mv, v);
            }
            out[op.alias] = mv ?? null;
            continue;
          }
          let s = 0;
          for (const r of rows) {
            const v = Number(getField(r, op.field));
            if (!Number.isNaN(v)) s += v;
          }
          out[op.alias] = s;
        } else if (op.kind === 'AVG') {
          let s = 0;
          let c = 0;
          for (const r of rows) {
            const v = Number(getField(r, op.field));
            if (!Number.isNaN(v)) {
              s += v;
              c += 1;
            }
          }
          out[op.alias] = c > 0 ? s / c : 0;
        }
      }
      results.push(out);
    }
    // 排序与限制
    if (this.sortField) {
      const f = this.sortField;
      const dir = this.sortDir === 'ASC' ? 1 : -1;
      results.sort((a, b) => {
        const av = a[f] as number | string;
        const bv = b[f] as number | string;
        if (av === bv) return 0;
        // 数值优先比较
        const an = Number(av);
        const bn = Number(bv);
        if (!Number.isNaN(an) && !Number.isNaN(bn)) return (an - bn) * dir;
        return String(av).localeCompare(String(bv)) * dir;
      });
    }
    if (this.limitCount !== undefined) {
      return results.slice(0, this.limitCount);
    }
    return results;
  }

  /**
   * 流式执行：按批次聚合，避免全量载入内存 - 优化版本
   * 使用增量聚合算法，不存储完整记录，大幅减少内存使用
   */
  async executeStreaming(): Promise<AggregateResult[]> {
    if (!this.streamCriteria) {
      // 未设置流式条件时退化为常规执行
      return this.execute();
    }

    const gf = this.groupFields.length > 0 ? this.groupFields : ['_all'];

    // 优化: 使用增量聚合状态，避免存储完整记录数组
    interface GroupState {
      keys: Record<string, unknown>;
      count: number;
      sums: Map<string, number>;
      mins: Map<string, number>;
      maxs: Map<string, number>;
      avgSums: Map<string, number>;
      avgCounts: Map<string, number>;
    }

    const groups = new Map<string, GroupState>();
    let processedCount = 0;

    const addRow = (r: FactRecord) => {
      const keyObj: Record<string, unknown> = {};
      for (const f of gf) {
        if (f === '_all') keyObj['_all'] = 'all';
        else keyObj[f] = getField(r, f);
      }
      const keyStr = JSON.stringify(keyObj);

      let state = groups.get(keyStr);
      if (!state) {
        state = {
          keys: keyObj,
          count: 0,
          sums: new Map(),
          mins: new Map(),
          maxs: new Map(),
          avgSums: new Map(),
          avgCounts: new Map(),
        };
        groups.set(keyStr, state);
      }

      state.count++;

      // 预计算聚合值，避免后续遍历完整记录
      for (const op of this.ops) {
        if (op.kind === 'SUM') {
          if (op.field.startsWith('__MIN__:')) {
            const real = op.field.replace('__MIN__:', '');
            const v = Number(getField(r, real));
            if (!Number.isNaN(v)) {
              const current = state.mins.get(real);
              state.mins.set(real, current === undefined ? v : Math.min(current, v));
            }
          } else if (op.field.startsWith('__MAX__:')) {
            const real = op.field.replace('__MAX__:', '');
            const v = Number(getField(r, real));
            if (!Number.isNaN(v)) {
              const current = state.maxs.get(real);
              state.maxs.set(real, current === undefined ? v : Math.max(current, v));
            }
          } else {
            const v = Number(getField(r, op.field));
            if (!Number.isNaN(v)) {
              const current = state.sums.get(op.field) || 0;
              state.sums.set(op.field, current + v);
            }
          }
        } else if (op.kind === 'AVG') {
          const v = Number(getField(r, op.field));
          if (!Number.isNaN(v)) {
            const currentSum = state.avgSums.get(op.field) || 0;
            const currentCount = state.avgCounts.get(op.field) || 0;
            state.avgSums.set(op.field, currentSum + v);
            state.avgCounts.set(op.field, currentCount + 1);
          }
        }
      }
    };

    // 批量处理数据 - 转换 FactCriteria 为 EncodedTriple 格式
    const encodedCriteria: Partial<{ subjectId: number; predicateId: number; objectId: number }> =
      {};
    if (this.streamCriteria.subject !== undefined) {
      const subjectId = this.store.getNodeIdByValue(this.streamCriteria.subject);
      if (subjectId !== undefined) encodedCriteria.subjectId = subjectId;
    }
    if (this.streamCriteria.predicate !== undefined) {
      const predicateId = this.store.getNodeIdByValue(this.streamCriteria.predicate);
      if (predicateId !== undefined) encodedCriteria.predicateId = predicateId;
    }
    if (this.streamCriteria.object !== undefined) {
      const objectId = this.store.getNodeIdByValue(this.streamCriteria.object);
      if (objectId !== undefined) encodedCriteria.objectId = objectId;
    }

    for await (const batch of this.store.streamFactRecords(encodedCriteria, this.streamBatchSize)) {
      for (const r of batch) {
        addRow(r);
        processedCount++;

        // 可选：定期释放内存压力（针对极大数据集）
        if (processedCount % 10000 === 0) {
          // 在JavaScript中GC通常是自动的，这里只是一个示例占位符
          // 实际生产环境中可以考虑分批处理策略
        }
      }
    }

    // 构建最终结果（优化版本，避免重复计算）
    const results: AggregateResult[] = [];
    for (const state of groups.values()) {
      const out: AggregateResult = { ...state.keys };

      for (const op of this.ops) {
        if (op.kind === 'COUNT') {
          out[op.alias] = state.count;
        } else if (op.kind === 'SUM') {
          if (op.field.startsWith('__MIN__:')) {
            const real = op.field.replace('__MIN__:', '');
            out[op.alias] = state.mins.get(real) ?? null;
          } else if (op.field.startsWith('__MAX__:')) {
            const real = op.field.replace('__MAX__:', '');
            out[op.alias] = state.maxs.get(real) ?? null;
          } else {
            out[op.alias] = state.sums.get(op.field) || 0;
          }
        } else if (op.kind === 'AVG') {
          const sum = state.avgSums.get(op.field) || 0;
          const count = state.avgCounts.get(op.field) || 0;
          out[op.alias] = count > 0 ? sum / count : 0;
        }
      }
      results.push(out);
    }

    // 优化排序：对于有限制的情况，使用部分排序
    if (this.sortField) {
      const f = this.sortField;
      const dir = this.sortDir === 'ASC' ? 1 : -1;

      if (this.limitCount !== undefined && this.limitCount < results.length) {
        // 使用部分排序优化，只保留前N个结果
        return this.partialSort(results, f, dir, this.limitCount);
      } else {
        // 全排序
        results.sort((a, b) => {
          const av = a[f] as number | string;
          const bv = b[f] as number | string;
          if (av === bv) return 0;
          const an = Number(av);
          const bn = Number(bv);
          if (!Number.isNaN(an) && !Number.isNaN(bn)) return (an - bn) * dir;
          return String(av).localeCompare(String(bv)) * dir;
        });
      }
    }

    if (this.limitCount !== undefined) {
      return results.slice(0, this.limitCount);
    }
    return results;
  }

  /**
   * 部分排序优化：使用堆排序获取前K个元素，适用于limit较小的场景
   */
  private partialSort(
    results: AggregateResult[],
    field: string,
    dir: number,
    k: number,
  ): AggregateResult[] {
    if (results.length <= k) {
      // 如果结果数量不超过k，直接全排序
      results.sort((a, b) => {
        const av = a[field] as number | string;
        const bv = b[field] as number | string;
        if (av === bv) return 0;
        const an = Number(av);
        const bn = Number(bv);
        if (!Number.isNaN(an) && !Number.isNaN(bn)) return (an - bn) * dir;
        return String(av).localeCompare(String(bv)) * dir;
      });
      return results;
    }

    // 使用快速选择算法的简化版本：构建最小/最大堆
    const heap: AggregateResult[] = [];
    const compare = (a: AggregateResult, b: AggregateResult): number => {
      const av = a[field] as number | string;
      const bv = b[field] as number | string;
      if (av === bv) return 0;
      const an = Number(av);
      const bn = Number(bv);
      if (!Number.isNaN(an) && !Number.isNaN(bn)) return (an - bn) * dir * -1; // 反向，维持最小堆
      return String(av).localeCompare(String(bv)) * dir * -1;
    };

    // 构建堆（前k个元素）
    for (let i = 0; i < Math.min(k, results.length); i++) {
      heap.push(results[i]);
    }
    heap.sort(compare);

    // 处理剩余元素
    for (let i = k; i < results.length; i++) {
      if (compare(results[i], heap[0]) < 0) {
        heap[0] = results[i];
        // 重新堆化（向下）
        let idx = 0;
        while (true) {
          let minIdx = idx;
          const left = 2 * idx + 1;
          const right = 2 * idx + 2;

          if (left < heap.length && compare(heap[left], heap[minIdx]) < 0) {
            minIdx = left;
          }
          if (right < heap.length && compare(heap[right], heap[minIdx]) < 0) {
            minIdx = right;
          }

          if (minIdx === idx) break;

          [heap[idx], heap[minIdx]] = [heap[minIdx], heap[idx]];
          idx = minIdx;
        }
      }
    }

    // 对堆进行最终排序并返回
    heap.sort((a, b) => -compare(a, b)); // 恢复正确顺序
    return heap;
  }
}

function getField(r: FactRecord, path: string): unknown {
  // 支持 subject/predicate/object 以及 *.properties 下的简单路径
  if (path === 'subject') return r.subject;
  if (path === 'predicate') return r.predicate;
  if (path === 'object') return r.object;

  const segs = path.split('.');
  let cur: any = r;
  for (const s of segs) {
    if (cur == null) return undefined;
    cur = cur[s as keyof typeof cur];
  }
  return cur;
}

import { PersistentStore } from '../../storage/persistentStore.js';
import { QueryBuilder, buildFindContextFromLabel } from '../queryBuilder.js';

type Direction = '->' | '<-' | '-';

interface NodeSpec {
  alias?: string;
  labels?: string[];
  props?: Record<string, unknown>;
}

interface EdgeSpec {
  direction: Direction;
  type?: string;
  alias?: string;
  variable?: { min: number; max: number; uniqueness?: 'NODE' | 'EDGE' | 'NONE' };
}

export interface PatternResult {
  [key: string]: unknown;
}

export class PatternBuilder {
  private nodes: NodeSpec[] = [];
  private edges: EdgeSpec[] = [];
  private returns: string[] | null = null;
  private propFilters: Map<
    string,
    Array<{ name: string; op: '=' | '>' | '<' | '>=' | '<='; value: unknown }>
  > = new Map();

  constructor(private readonly store: PersistentStore) {}

  node(alias?: string, labels?: string[], props?: Record<string, unknown>): this {
    this.nodes.push({ alias, labels, props });
    return this;
  }

  edge(direction: Direction, type?: string, alias?: string): this {
    this.edges.push({ direction, type, alias });
    return this;
  }

  // 为最近一次 edge 设置变长范围（[*min..max]）
  variable(min: number, max: number, uniqueness: 'NODE' | 'EDGE' | 'NONE' = 'NODE'): this {
    if (this.edges.length === 0) throw new Error('variable() 必须在 edge() 之后调用');
    const last = this.edges[this.edges.length - 1];
    last.variable = { min: Math.max(1, min), max: Math.max(min, max), uniqueness };
    return this;
  }

  whereNodeProperty(
    alias: string,
    name: string,
    op: '=' | '>' | '<' | '>=' | '<=',
    value: unknown,
  ): this {
    const key = alias;
    const arr = this.propFilters.get(key) ?? [];
    arr.push({ name, op, value });
    this.propFilters.set(key, arr);
    return this;
  }

  return(items: string[]): this {
    this.returns = items;
    return this;
  }

  async execute(): Promise<PatternResult[]> {
    if (this.nodes.length === 0) return [];

    type State = { nodeId: number; bindings: Map<string, number> };
    const labelIndex = this.store.getLabelIndex();
    const propIndex = this.store.getPropertyIndex();

    const satisfiesNode = (nodeId: number, spec: NodeSpec): boolean => {
      if (spec.labels && spec.labels.length > 0) {
        if (!labelIndex.findNodesByLabels(spec.labels, { mode: 'AND' }).has(nodeId)) return false;
      }
      // 属性过滤（等值/范围）
      const alias = spec.alias;
      if (alias && this.propFilters.has(alias)) {
        for (const f of this.propFilters.get(alias)!) {
          if (f.op === '=') {
            if (!propIndex.queryNodesByProperty(f.name, f.value).has(nodeId)) return false;
          } else {
            const inRange = propIndex
              .queryNodesByRange(
                f.name,
                f.op === '>' || f.op === '>=' ? f.value : undefined,
                f.op === '<' || f.op === '<=' ? f.value : undefined,
                f.op === '>=' || f.op === '<=',
                f.op === '<=' || f.op === '>=',
              )
              .has(nodeId);
            if (!inRange) return false;
          }
        }
      }
      return true;
    };

    // 初始状态：根据第一个节点的标签/属性进行候选收敛；若无约束，则从全库的主语集启动
    let startCandidates: Set<number>;
    const first = this.nodes[0];
    if (first.labels && first.labels.length > 0) {
      startCandidates = labelIndex.findNodesByLabels(first.labels, { mode: 'AND' });
    } else if (first.alias && this.propFilters.has(first.alias)) {
      // 找到第一个属性过滤对应的候选集合（等值的联合，范围的并集，简化处理）
      const idSet = new Set<number>();
      for (const f of this.propFilters.get(first.alias)!) {
        if (f.op === '=') {
          propIndex.queryNodesByProperty(f.name, f.value).forEach((id) => idSet.add(id));
        } else {
          propIndex
            .queryNodesByRange(
              f.name,
              f.op === '>' || f.op === '>=' ? f.value : undefined,
              f.op === '<' || f.op === '<=' ? f.value : undefined,
              f.op === '>=' || f.op === '<=',
              f.op === '<=' || f.op === '>=',
            )
            .forEach((id) => idSet.add(id));
        }
      }
      startCandidates = idSet;
    } else {
      // 从全库主语集合粗略启动
      const all = this.store.resolveRecords(this.store.query({}), { includeProperties: false });
      startCandidates = new Set(all.map((r) => r.subjectId));
    }

    let states: State[] = [];
    for (const nid of startCandidates) {
      if (!satisfiesNode(nid, first)) continue;
      const bindings = new Map<string, number>();
      if (first.alias) bindings.set(first.alias, nid);
      states.push({ nodeId: nid, bindings });
    }

    // 逐段扩展
    for (let i = 0; i < this.edges.length; i++) {
      const e = this.edges[i];
      const nextNodeSpec = this.nodes[i + 1];
      const dir = e.direction === '<-' ? 'reverse' : 'forward';
      const pid = e.type ? this.store.getNodeIdByValue(e.type) : undefined;
      const nextStates: State[] = [];

      for (const st of states) {
        if (e.variable) {
          const predId = pid ?? 0;
          if (!pid) continue; // 变长必须指定关系类型
          const { VariablePathBuilder } = await import('../path/variable.js');
          const vbuilder = new (VariablePathBuilder as any)(
            this.store,
            new Set<number>([st.nodeId]),
            predId,
            {
              min: e.variable.min,
              max: e.variable.max,
              uniqueness: e.variable.uniqueness,
              direction: dir === 'forward' ? 'forward' : 'reverse',
            },
          );
          const paths = vbuilder.all();
          for (const p of paths) {
            const neighbor = p.endId;
            if (!satisfiesNode(neighbor, nextNodeSpec)) continue;
            const b = new Map(st.bindings);
            const curAlias = this.nodes[i].alias;
            const nextAlias = nextNodeSpec.alias;
            if (curAlias && b.has(curAlias) && b.get(curAlias) !== st.nodeId) continue;
            if (nextAlias && b.has(nextAlias) && b.get(nextAlias) !== neighbor) continue;
            if (curAlias && !b.has(curAlias)) b.set(curAlias, st.nodeId);
            if (nextAlias && !b.has(nextAlias)) b.set(nextAlias, neighbor);
            nextStates.push({ nodeId: neighbor, bindings: b });
          }
        } else {
          // 固定一跳
          const criteria = dir === 'forward' ? { subjectId: st.nodeId } : { objectId: st.nodeId };
          const enc =
            pid === undefined
              ? this.store.query(criteria)
              : this.store.query({ ...criteria, predicateId: pid });
          const recs = this.store.resolveRecords(enc, { includeProperties: false });
          for (const r of recs) {
            const neighbor = dir === 'forward' ? r.objectId : r.subjectId;
            if (!satisfiesNode(neighbor, nextNodeSpec)) continue;
            const b = new Map(st.bindings);
            const curAlias = this.nodes[i].alias;
            const nextAlias = nextNodeSpec.alias;
            if (curAlias && b.has(curAlias) && b.get(curAlias) !== st.nodeId) continue;
            if (nextAlias && b.has(nextAlias) && b.get(nextAlias) !== neighbor) continue;
            if (curAlias && !b.has(curAlias)) b.set(curAlias, st.nodeId);
            if (nextAlias && !b.has(nextAlias)) b.set(nextAlias, neighbor);
            nextStates.push({ nodeId: neighbor, bindings: b });
          }
        }
      }
      states = nextStates;
      if (states.length === 0) break;
    }

    // 投影返回
    const out: PatternResult[] = [];
    for (const st of states) {
      const row: PatternResult = {};
      if (this.returns && this.returns.length > 0) {
        for (const key of this.returns) {
          const id = st.bindings.get(key);
          row[key] = id !== undefined ? (this.store.getNodeValueById(id) ?? null) : null;
        }
      } else {
        // 默认返回所有别名
        for (const [alias, id] of st.bindings.entries()) {
          row[alias] = this.store.getNodeValueById(id) ?? null;
        }
      }
      out.push(row);
    }
    return out;
  }
}

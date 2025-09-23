import { FactInput, FactRecord } from '../storage/persistentStore.js';
import { PersistentStore } from '../storage/persistentStore.js';

export type FactCriteria = Partial<FactInput>;

export type FrontierOrientation = 'subject' | 'object' | 'both';

export interface PropertyFilter {
  propertyName: string;
  value?: unknown;
  range?: {
    min?: unknown;
    max?: unknown;
    includeMin?: boolean;
    includeMax?: boolean;
  };
}

interface QueryContext {
  facts: FactRecord[];
  frontier: Set<number>;
  orientation: FrontierOrientation;
}

const EMPTY_CONTEXT: QueryContext = {
  facts: [],
  frontier: new Set<number>(),
  orientation: 'object',
};

export class QueryBuilder {
  private readonly facts: FactRecord[];
  private readonly frontier: Set<number>;
  private readonly orientation: FrontierOrientation;
  private readonly pinnedEpoch?: number;

  constructor(
    private readonly store: PersistentStore,
    context: QueryContext,
    pinnedEpoch?: number,
  ) {
    this.facts = context.facts;
    this.frontier = context.frontier;
    this.orientation = context.orientation;
    this.pinnedEpoch = pinnedEpoch;
  }

  // 为测试与直觉友好：提供数组化能力（只读视图）
  get length(): number {
    this.pin();
    try {
      return this.facts.length;
    } finally {
      this.unpin();
    }
  }

  slice(start?: number, end?: number): FactRecord[] {
    this.pin();
    try {
      return this.facts.slice(start, end);
    } finally {
      this.unpin();
    }
  }

  // 迭代期间保持快照固定
  *[Symbol.iterator](): IterableIterator<FactRecord> {
    this.pin();
    try {
      for (const fact of this.facts) {
        yield fact;
      }
    } finally {
      this.unpin();
    }
  }

  toArray(): FactRecord[] {
    return this.all();
  }

  all(): FactRecord[] {
    this.pin();
    try {
      return [...this.facts];
    } finally {
      this.unpin();
    }
  }

  where(predicate: (record: FactRecord) => boolean): QueryBuilder {
    this.pin();
    const nextFacts = this.facts.filter((f) => {
      try {
        return Boolean(predicate(f));
      } catch {
        return false;
      }
    });
    this.unpin();
    const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
    return new QueryBuilder(
      this.store,
      {
        facts: nextFacts,
        frontier: nextFrontier,
        orientation: this.orientation,
      },
      this.pinnedEpoch,
    );
  }

  limit(n: number): QueryBuilder {
    if (n < 0 || Number.isNaN(n)) {
      return this;
    }
    this.pin();
    const nextFacts = this.facts.slice(0, n);
    this.unpin();
    const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
    return new QueryBuilder(
      this.store,
      {
        facts: nextFacts,
        frontier: nextFrontier,
        orientation: this.orientation,
      },
      this.pinnedEpoch,
    );
  }

  /**
   * 根据节点属性过滤当前前沿
   * @param filter 属性过滤条件
   */
  whereNodeProperty(filter: PropertyFilter): QueryBuilder {
    this.pin();
    try {
      const propertyIndex = this.store.getPropertyIndex();
      let matchingNodeIds: Set<number>;

      if (filter.value !== undefined) {
        // 等值查询
        matchingNodeIds = propertyIndex.queryNodesByProperty(filter.propertyName, filter.value);
      } else if (filter.range) {
        // 范围查询
        matchingNodeIds = propertyIndex.queryNodesByRange(
          filter.propertyName,
          filter.range.min,
          filter.range.max,
          filter.range.includeMin,
          filter.range.includeMax,
        );
      } else {
        // 如果没有指定值或范围，返回所有具有该属性的节点
        const allPropertyNames = propertyIndex.getNodePropertyNames();
        if (!allPropertyNames.includes(filter.propertyName)) {
          return new QueryBuilder(this.store, EMPTY_CONTEXT, this.pinnedEpoch);
        }
        // 获取所有具有该属性的节点（通过查询所有可能的值）
        matchingNodeIds = new Set<number>();
        // 注意：这是一个简化实现，在实际应用中可能需要更高效的方式
      }

      // 根据当前方向过滤事实
      const nextFacts = this.facts.filter((fact) => {
        if (this.orientation === 'subject') {
          return matchingNodeIds.has(fact.subjectId);
        } else if (this.orientation === 'object') {
          return matchingNodeIds.has(fact.objectId);
        } else {
          // orientation === 'both'
          return matchingNodeIds.has(fact.subjectId) || matchingNodeIds.has(fact.objectId);
        }
      });

      const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
      return new QueryBuilder(
        this.store,
        {
          facts: nextFacts,
          frontier: nextFrontier,
          orientation: this.orientation,
        },
        this.pinnedEpoch,
      );
    } finally {
      this.unpin();
    }
  }

  /**
   * 根据边属性过滤当前事实
   * @param filter 属性过滤条件
   */
  whereEdgeProperty(filter: PropertyFilter): QueryBuilder {
    this.pin();
    try {
      const propertyIndex = this.store.getPropertyIndex();
      let matchingEdgeKeys: Set<string>;

      if (filter.value !== undefined) {
        // 等值查询
        matchingEdgeKeys = propertyIndex.queryEdgesByProperty(filter.propertyName, filter.value);
      } else {
        // 如果没有指定值，返回所有具有该属性的边
        const allPropertyNames = propertyIndex.getEdgePropertyNames();
        if (!allPropertyNames.includes(filter.propertyName)) {
          return new QueryBuilder(this.store, EMPTY_CONTEXT, this.pinnedEpoch);
        }
        // 获取所有具有该属性的边
        matchingEdgeKeys = new Set<string>();
        // 注意：这是一个简化实现
      }

      // 过滤当前事实
      const nextFacts = this.facts.filter((fact) => {
        const edgeKey = encodeTripleKey(fact);
        return matchingEdgeKeys.has(edgeKey);
      });

      const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
      return new QueryBuilder(
        this.store,
        {
          facts: nextFacts,
          frontier: nextFrontier,
          orientation: this.orientation,
        },
        this.pinnedEpoch,
      );
    } finally {
      this.unpin();
    }
  }

  /**
   * 基于属性条件进行联想查询
   * @param predicate 关系谓词
   * @param nodePropertyFilter 可选的目标节点属性过滤条件
   */
  followWithNodeProperty(predicate: string, nodePropertyFilter?: PropertyFilter): QueryBuilder {
    return this.traverseWithProperty(predicate, 'forward', nodePropertyFilter);
  }

  /**
   * 基于属性条件进行反向联想查询
   * @param predicate 关系谓词
   * @param nodePropertyFilter 可选的目标节点属性过滤条件
   */
  followReverseWithNodeProperty(
    predicate: string,
    nodePropertyFilter?: PropertyFilter,
  ): QueryBuilder {
    return this.traverseWithProperty(predicate, 'reverse', nodePropertyFilter);
  }

  /**
   * 带属性过滤的联想查询实现
   */
  private traverseWithProperty(
    predicate: string,
    direction: 'forward' | 'reverse',
    nodePropertyFilter?: PropertyFilter,
  ): QueryBuilder {
    if (this.frontier.size === 0) {
      return new QueryBuilder(this.store, EMPTY_CONTEXT);
    }

    this.pin();
    try {
      const predicateId = this.store.getNodeIdByValue(predicate);
      if (predicateId === undefined) {
        return new QueryBuilder(this.store, EMPTY_CONTEXT);
      }

      // 如果有节点属性过滤条件，先获取匹配的节点ID
      let targetNodeIds: Set<number> | undefined;
      if (nodePropertyFilter) {
        const propertyIndex = this.store.getPropertyIndex();
        if (nodePropertyFilter.value !== undefined) {
          targetNodeIds = propertyIndex.queryNodesByProperty(
            nodePropertyFilter.propertyName,
            nodePropertyFilter.value,
          );
        } else if (nodePropertyFilter.range) {
          targetNodeIds = propertyIndex.queryNodesByRange(
            nodePropertyFilter.propertyName,
            nodePropertyFilter.range.min,
            nodePropertyFilter.range.max,
            nodePropertyFilter.range.includeMin,
            nodePropertyFilter.range.includeMax,
          );
        }

        // 如果没有匹配的节点，直接返回空结果
        if (targetNodeIds && targetNodeIds.size === 0) {
          return new QueryBuilder(this.store, EMPTY_CONTEXT);
        }
      }

      const triples = new Map<string, FactRecord>();

      for (const nodeId of this.frontier.values()) {
        const criteria =
          direction === 'forward'
            ? { subjectId: nodeId, predicateId }
            : { predicateId, objectId: nodeId };

        const matches = this.store.query(criteria);
        const records = this.store.resolveRecords(matches);

        records.forEach((record) => {
          // 如果有目标节点过滤条件，检查目标节点是否匹配
          if (targetNodeIds) {
            const targetNodeId = direction === 'forward' ? record.objectId : record.subjectId;
            if (!targetNodeIds.has(targetNodeId)) {
              return; // 跳过不匹配的记录
            }
          }

          triples.set(encodeTripleKey(record), record);
        });
      }

      const nextFacts = [...triples.values()];
      const nextFrontier = new Set<number>();

      nextFacts.forEach((fact) => {
        if (direction === 'forward') {
          nextFrontier.add(fact.objectId);
        } else {
          nextFrontier.add(fact.subjectId);
        }
      });

      return new QueryBuilder(
        this.store,
        {
          facts: nextFacts,
          frontier: nextFrontier,
          orientation: direction === 'forward' ? 'object' : 'subject',
        },
        this.pinnedEpoch,
      );
    } finally {
      this.unpin();
    }
  }

  anchor(orientation: FrontierOrientation): QueryBuilder {
    this.pin();
    const nextFrontier = buildInitialFrontier(this.facts, orientation);
    this.unpin();
    return new QueryBuilder(
      this.store,
      {
        facts: [...this.facts],
        frontier: nextFrontier,
        orientation,
      },
      this.pinnedEpoch,
    );
  }

  follow(predicate: string): QueryBuilder {
    return this.traverse(predicate, 'forward');
  }

  followReverse(predicate: string): QueryBuilder {
    return this.traverse(predicate, 'reverse');
  }

  private traverse(predicate: string, direction: 'forward' | 'reverse'): QueryBuilder {
    if (this.frontier.size === 0) {
      return new QueryBuilder(this.store, EMPTY_CONTEXT);
    }

    this.pin();
    try {
      const predicateId = this.store.getNodeIdByValue(predicate);
      if (predicateId === undefined) {
        return new QueryBuilder(this.store, EMPTY_CONTEXT);
      }

      const triples = new Map<string, FactRecord>();

      for (const nodeId of this.frontier.values()) {
        const criteria =
          direction === 'forward'
            ? { subjectId: nodeId, predicateId }
            : { predicateId, objectId: nodeId };

        const matches = this.store.query(criteria);
        const records = this.store.resolveRecords(matches);
        records.forEach((record) => {
          triples.set(encodeTripleKey(record), record);
        });
      }

      const nextFacts = [...triples.values()];
      const nextFrontier = new Set<number>();

      nextFacts.forEach((fact) => {
        if (direction === 'forward') {
          nextFrontier.add(fact.objectId);
        } else {
          nextFrontier.add(fact.subjectId);
        }
      });

      return new QueryBuilder(
        this.store,
        {
          facts: nextFacts,
          frontier: nextFrontier,
          orientation: direction === 'forward' ? 'object' : 'subject',
        },
        this.pinnedEpoch,
      );
    } finally {
      this.unpin();
    }
  }

  static fromFindResult(
    store: PersistentStore,
    context: QueryContext,
    pinnedEpoch?: number,
  ): QueryBuilder {
    return new QueryBuilder(store, context, pinnedEpoch);
  }

  static empty(store: PersistentStore): QueryBuilder {
    return new QueryBuilder(store, EMPTY_CONTEXT);
  }

  private pin(): void {
    if (this.pinnedEpoch !== undefined) {
      try {
        // 只做内存级别的epoch固定，避免与withSnapshot的reader注册冲突
        (this.store as unknown as { pinnedEpochStack: number[] }).pinnedEpochStack?.push(
          this.pinnedEpoch,
        );
      } catch {
        /* ignore */
      }
    }
  }

  private unpin(): void {
    if (this.pinnedEpoch !== undefined) {
      try {
        // 只做内存级别的epoch释放，避免与withSnapshot的reader注册冲突
        (this.store as unknown as { pinnedEpochStack: number[] }).pinnedEpochStack?.pop();
      } catch {
        /* ignore */
      }
    }
  }
}

export function buildFindContext(
  store: PersistentStore,
  criteria: FactCriteria,
  anchor: FrontierOrientation,
): QueryContext {
  const query = convertCriteriaToIds(store, criteria);
  if (query === null) {
    return EMPTY_CONTEXT;
  }

  const matches = store.query(query);
  if (matches.length === 0) {
    return EMPTY_CONTEXT;
  }

  const facts = store.resolveRecords(matches);
  const frontier = buildInitialFrontier(facts, anchor);

  return {
    facts,
    frontier,
    orientation: anchor,
  };
}

/**
 * 基于属性条件构建查询上下文
 * @param store 数据存储实例
 * @param propertyFilter 属性过滤条件
 * @param anchor 前沿方向
 * @param target 查询目标（节点或边）
 */
export function buildFindContextFromProperty(
  store: PersistentStore,
  propertyFilter: PropertyFilter,
  anchor: FrontierOrientation,
  target: 'node' | 'edge' = 'node',
): QueryContext {
  const propertyIndex = store.getPropertyIndex();

  if (target === 'node') {
    let matchingNodeIds: Set<number>;

    if (propertyFilter.value !== undefined) {
      // 等值查询
      matchingNodeIds = propertyIndex.queryNodesByProperty(
        propertyFilter.propertyName,
        propertyFilter.value,
      );
    } else if (propertyFilter.range) {
      // 范围查询
      matchingNodeIds = propertyIndex.queryNodesByRange(
        propertyFilter.propertyName,
        propertyFilter.range.min,
        propertyFilter.range.max,
        propertyFilter.range.includeMin,
        propertyFilter.range.includeMax,
      );
    } else {
      // 返回所有具有该属性的节点
      const allPropertyNames = propertyIndex.getNodePropertyNames();
      if (!allPropertyNames.includes(propertyFilter.propertyName)) {
        return EMPTY_CONTEXT;
      }
      matchingNodeIds = new Set<number>();
      // 注意：这需要更完整的实现来获取所有具有该属性的节点
    }

    if (matchingNodeIds.size === 0) {
      return EMPTY_CONTEXT;
    }

    // 查找包含这些节点的所有三元组
    const allFacts: FactRecord[] = [];
    for (const nodeId of matchingNodeIds) {
      // 作为主语的三元组
      const subjectTriples = store.query({ subjectId: nodeId });
      allFacts.push(...store.resolveRecords(subjectTriples));

      // 作为宾语的三元组
      const objectTriples = store.query({ objectId: nodeId });
      allFacts.push(...store.resolveRecords(objectTriples));
    }

    // 去重
    const uniqueFacts = new Map<string, FactRecord>();
    allFacts.forEach((fact) => {
      uniqueFacts.set(encodeTripleKey(fact), fact);
    });

    const facts = [...uniqueFacts.values()];
    const frontier = buildInitialFrontier(facts, anchor);

    return {
      facts,
      frontier,
      orientation: anchor,
    };
  } else {
    // target === 'edge'
    let matchingEdgeKeys: Set<string>;

    if (propertyFilter.value !== undefined) {
      matchingEdgeKeys = propertyIndex.queryEdgesByProperty(
        propertyFilter.propertyName,
        propertyFilter.value,
      );
    } else {
      const allPropertyNames = propertyIndex.getEdgePropertyNames();
      if (!allPropertyNames.includes(propertyFilter.propertyName)) {
        return EMPTY_CONTEXT;
      }
      matchingEdgeKeys = new Set<string>();
      // 注意：这需要更完整的实现
    }

    if (matchingEdgeKeys.size === 0) {
      return EMPTY_CONTEXT;
    }

    // 根据边键获取对应的三元组
    const facts: FactRecord[] = [];
    for (const edgeKey of matchingEdgeKeys) {
      const [subjectId, predicateId, objectId] = edgeKey.split(':').map(Number);
      const matches = store.query({ subjectId, predicateId, objectId });
      facts.push(...store.resolveRecords(matches));
    }

    const frontier = buildInitialFrontier(facts, anchor);

    return {
      facts,
      frontier,
      orientation: anchor,
    };
  }
}

type IdCriteria = Partial<Record<'subjectId' | 'predicateId' | 'objectId', number>>;

function convertCriteriaToIds(store: PersistentStore, criteria: FactCriteria): IdCriteria | null {
  const result: IdCriteria = {};

  if (criteria.subject !== undefined) {
    const id = store.getNodeIdByValue(criteria.subject);
    if (id === undefined) {
      return null;
    }
    result.subjectId = id;
  }

  if (criteria.predicate !== undefined) {
    const id = store.getNodeIdByValue(criteria.predicate);
    if (id === undefined) {
      return null;
    }
    result.predicateId = id;
  }

  if (criteria.object !== undefined) {
    const id = store.getNodeIdByValue(criteria.object);
    if (id === undefined) {
      return null;
    }
    result.objectId = id;
  }

  return result;
}

function buildInitialFrontier(facts: FactRecord[], anchor: FrontierOrientation): Set<number> {
  const nodes = new Set<number>();
  facts.forEach((fact) => {
    if (anchor === 'subject') {
      nodes.add(fact.subjectId);
      return;
    }
    if (anchor === 'object') {
      nodes.add(fact.objectId);
      return;
    }
    nodes.add(fact.subjectId);
    nodes.add(fact.objectId);
  });
  return nodes;
}

function rebuildFrontier(facts: FactRecord[], orientation: FrontierOrientation): Set<number> {
  if (facts.length === 0) return new Set<number>();
  if (orientation === 'subject') return new Set<number>(facts.map((f) => f.subjectId));
  if (orientation === 'object') return new Set<number>(facts.map((f) => f.objectId));
  const set = new Set<number>();
  facts.forEach((f) => {
    set.add(f.subjectId);
    set.add(f.objectId);
  });
  return set;
}

function encodeTripleKey(fact: FactRecord): string {
  return `${fact.subjectId}:${fact.predicateId}:${fact.objectId}`;
}

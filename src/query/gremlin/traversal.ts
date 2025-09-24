/**
 * Gremlin 图遍历实现
 *
 * 实现 GraphTraversal 链式 API，将 Gremlin 步骤转换为 SynapseDB 查询
 * 支持流式执行和延迟求值
 */

import type { PersistentStore } from '../../storage/persistentStore.js';
import type {
  Vertex,
  Edge,
  Direction,
  Predicate,
  ElementId,
  PropertyKey,
  PropertyValue,
  TraversalConfig,
} from './types.js';
import { Scope, Order, GremlinError, P } from './types.js';
import type { GremlinStep } from './step.js';

// 遍历状态
interface TraversalState {
  steps: GremlinStep[];
  currentStep: number;
  labels: Map<string, number>; // 标签到步骤索引的映射
  sideEffects: Map<string, unknown>;
  barriers: Set<number>; // 屏障步骤位置
}

// 遍历结果
export interface TraversalResult<T = Vertex | Edge> {
  value: T;
  path?: (Vertex | Edge)[];
  bulk: number;
}

/**
 * GraphTraversal 主类
 */
export class GraphTraversal<S = Vertex | Edge, E = Vertex | Edge> {
  private state: TraversalState;
  private readonly store: PersistentStore;
  private config: TraversalConfig;
  private compiled = false;
  private results?: Promise<TraversalResult<E>[]>;

  constructor(
    store: PersistentStore,
    initialSteps: GremlinStep[] = [],
    config: TraversalConfig = {},
  ) {
    this.store = store;
    this.config = config;
    this.state = {
      steps: [...initialSteps],
      currentStep: 0,
      labels: new Map(),
      sideEffects: new Map(config.sideEffects || []),
      barriers: new Set(),
    };
  }

  // ============ 起始步骤 ============

  /**
   * V() - 获取所有或指定顶点
   */
  V(...ids: ElementId[]): GraphTraversal<Vertex, Vertex> {
    return this.addStep({
      type: 'V',
      id: this.generateStepId(),
      ids: ids.length > 0 ? ids : undefined,
    }) as GraphTraversal<Vertex, Vertex>;
  }

  /**
   * E() - 获取所有或指定边
   */
  E(...ids: ElementId[]): GraphTraversal<Edge, Edge> {
    return this.addStep({
      type: 'E',
      id: this.generateStepId(),
      ids: ids.length > 0 ? ids : undefined,
    }) as GraphTraversal<Edge, Edge>;
  }

  // ============ 遍历步骤 ============

  /**
   * out() - 沿出边遍历到相邻顶点
   */
  out(...edgeLabels: string[]): GraphTraversal<S, Vertex> {
    return this.addStep({
      type: 'out',
      id: this.generateStepId(),
      edgeLabels: edgeLabels.length > 0 ? edgeLabels : undefined,
    }) as GraphTraversal<S, Vertex>;
  }

  /**
   * in() - 沿入边遍历到相邻顶点
   */
  in(...edgeLabels: string[]): GraphTraversal<S, Vertex> {
    return this.addStep({
      type: 'in',
      id: this.generateStepId(),
      edgeLabels: edgeLabels.length > 0 ? edgeLabels : undefined,
    }) as GraphTraversal<S, Vertex>;
  }

  /**
   * both() - 沿双向边遍历到相邻顶点
   */
  both(...edgeLabels: string[]): GraphTraversal<S, Vertex> {
    return this.addStep({
      type: 'both',
      id: this.generateStepId(),
      edgeLabels: edgeLabels.length > 0 ? edgeLabels : undefined,
    }) as GraphTraversal<S, Vertex>;
  }

  /**
   * outE() - 获取出边
   */
  outE(...edgeLabels: string[]): GraphTraversal<S, Edge> {
    return this.addStep({
      type: 'outE',
      id: this.generateStepId(),
      edgeLabels: edgeLabels.length > 0 ? edgeLabels : undefined,
    }) as GraphTraversal<S, Edge>;
  }

  /**
   * inE() - 获取入边
   */
  inE(...edgeLabels: string[]): GraphTraversal<S, Edge> {
    return this.addStep({
      type: 'inE',
      id: this.generateStepId(),
      edgeLabels: edgeLabels.length > 0 ? edgeLabels : undefined,
    }) as GraphTraversal<S, Edge>;
  }

  /**
   * bothE() - 获取双向边
   */
  bothE(...edgeLabels: string[]): GraphTraversal<S, Edge> {
    return this.addStep({
      type: 'bothE',
      id: this.generateStepId(),
      edgeLabels: edgeLabels.length > 0 ? edgeLabels : undefined,
    }) as GraphTraversal<S, Edge>;
  }

  /**
   * inV() - 从边到入顶点
   */
  inV(): GraphTraversal<S, Vertex> {
    return this.addStep({
      type: 'inV',
      id: this.generateStepId(),
    }) as GraphTraversal<S, Vertex>;
  }

  /**
   * outV() - 从边到出顶点
   */
  outV(): GraphTraversal<S, Vertex> {
    return this.addStep({
      type: 'outV',
      id: this.generateStepId(),
    }) as GraphTraversal<S, Vertex>;
  }

  /**
   * bothV() - 从边到两端顶点
   */
  bothV(): GraphTraversal<S, Vertex> {
    return this.addStep({
      type: 'bothV',
      id: this.generateStepId(),
    }) as GraphTraversal<S, Vertex>;
  }

  // ============ 过滤步骤 ============

  /**
   * has() - 属性过滤
   */
  has(key: string): GraphTraversal<S, E>;
  has(key: string, value: PropertyValue): GraphTraversal<S, E>;
  has(key: string, predicate: Predicate): GraphTraversal<S, E>;
  has(label: string, key: string, value: PropertyValue): GraphTraversal<S, E>;
  has(label: string, key: string, predicate: Predicate): GraphTraversal<S, E>;
  has(...args: unknown[]): GraphTraversal<S, E> {
    const step: any = {
      type: 'has',
      id: this.generateStepId(),
    };

    if (args.length === 1) {
      step.key = args[0] as string;
    } else if (args.length === 2) {
      step.key = args[0] as string;
      if (this.isPredicate(args[1])) {
        step.predicate = args[1] as Predicate;
      } else {
        step.value = args[1];
      }
    } else if (args.length === 3) {
      step.label = args[0] as string;
      step.key = args[1] as string;
      if (this.isPredicate(args[2])) {
        step.predicate = args[2] as Predicate;
      } else {
        step.value = args[2];
      }
    }

    return this.addStep(step);
  }

  /**
   * hasLabel() - 标签过滤
   */
  hasLabel(...labels: string[]): GraphTraversal<S, E> {
    return this.addStep({
      type: 'hasLabel',
      id: this.generateStepId(),
      labels,
    });
  }

  /**
   * hasId() - ID过滤
   */
  hasId(...ids: ElementId[]): GraphTraversal<S, E> {
    return this.addStep({
      type: 'hasId',
      id: this.generateStepId(),
      ids,
    });
  }

  /**
   * is() - 值比较
   */
  is(predicate: Predicate): GraphTraversal<S, E>;
  is(value: PropertyValue): GraphTraversal<S, E>;
  is(arg: Predicate | PropertyValue): GraphTraversal<S, E> {
    const predicate = this.isPredicate(arg) ? arg : { operator: P.eq, value: arg };
    return this.addStep({
      type: 'is',
      id: this.generateStepId(),
      predicate,
    });
  }

  /**
   * where() - 复杂过滤
   */
  where(predicate: Predicate): GraphTraversal<S, E>;
  where(traversal: GraphTraversal<any, any>): GraphTraversal<S, E>;
  where(arg: Predicate | GraphTraversal<any, any>): GraphTraversal<S, E> {
    if (this.isPredicate(arg)) {
      return this.addStep({
        type: 'where',
        id: this.generateStepId(),
        predicate: arg,
      });
    } else {
      return this.addStep({
        type: 'where',
        id: this.generateStepId(),
        traversal: (arg as GraphTraversal<any, any>).getSteps(),
      });
    }
  }

  // ============ 范围限制步骤 ============

  /**
   * limit() - 限制数量
   */
  limit(limit: number): GraphTraversal<S, E> {
    return this.addStep({
      type: 'limit',
      id: this.generateStepId(),
      limit,
      scope: Scope.global,
    });
  }

  /**
   * range() - 范围选择
   */
  range(low: number, high: number): GraphTraversal<S, E> {
    return this.addStep({
      type: 'range',
      id: this.generateStepId(),
      low,
      high,
      scope: Scope.global,
    });
  }

  /**
   * skip() - 跳过
   */
  skip(skip: number): GraphTraversal<S, E> {
    return this.addStep({
      type: 'skip',
      id: this.generateStepId(),
      skip,
      scope: Scope.global,
    });
  }

  // ============ 排序步骤 ============

  /**
   * order() - 排序
   */
  order(): GraphTraversal<S, E> {
    return this.addStep({
      type: 'order',
      id: this.generateStepId(),
      scope: Scope.global,
    });
  }

  // ============ 去重步骤 ============

  /**
   * dedup() - 去重
   */
  dedup(...dedupLabels: string[]): GraphTraversal<S, E> {
    return this.addStep({
      type: 'dedup',
      id: this.generateStepId(),
      scope: Scope.global,
      dedupLabels: dedupLabels.length > 0 ? dedupLabels : undefined,
    });
  }

  // ============ 标记和选择步骤 ============

  /**
   * as() - 标记步骤
   */
  as(stepLabel: string): GraphTraversal<S, E> {
    const stepIndex = this.state.steps.length;
    this.state.labels.set(stepLabel, stepIndex);

    return this.addStep({
      type: 'as',
      id: this.generateStepId(),
      stepLabel,
    });
  }

  /**
   * select() - 选择标记的步骤结果
   */
  select<T = unknown>(...selectKeys: string[]): GraphTraversal<S, T> {
    return this.addStep({
      type: 'select',
      id: this.generateStepId(),
      selectKeys,
    }) as any;
  }

  // ============ 投影步骤 ============

  /**
   * values() - 获取属性值
   */
  values<T = PropertyValue>(...propertyKeys: string[]): GraphTraversal<S, T> {
    return this.addStep({
      type: 'values',
      id: this.generateStepId(),
      propertyKeys: propertyKeys.length > 0 ? propertyKeys : undefined,
    }) as any;
  }

  /**
   * valueMap() - 获取值映射
   */
  valueMap<T = Record<string, PropertyValue>>(...propertyKeys: string[]): GraphTraversal<S, T> {
    return this.addStep({
      type: 'valueMap',
      id: this.generateStepId(),
      includeTokens: false,
      propertyKeys: propertyKeys.length > 0 ? propertyKeys : undefined,
    }) as any;
  }

  /**
   * elementMap() - 获取元素映射
   */
  elementMap<T = Record<string, PropertyValue>>(...propertyKeys: string[]): GraphTraversal<S, T> {
    return this.addStep({
      type: 'elementMap',
      id: this.generateStepId(),
      propertyKeys: propertyKeys.length > 0 ? propertyKeys : undefined,
    }) as any;
  }

  // ============ 聚合步骤 ============

  /**
   * count() - 计数
   */
  count(): GraphTraversal<S, number> {
    return this.addStep({
      type: 'count',
      id: this.generateStepId(),
      scope: Scope.global,
    }) as any;
  }

  /**
   * fold() - 折叠为列表
   */
  fold<T = E[]>(): GraphTraversal<S, T> {
    return this.addStep({
      type: 'fold',
      id: this.generateStepId(),
    }) as any;
  }

  // ============ 终端步骤 ============

  /**
   * toList() - 转为数组
   */
  async toList(): Promise<E[]> {
    const results = await this.execute();
    return results.map((result) => result.value);
  }

  /**
   * iterate() - 仅执行，不返回结果
   */
  async iterate(): Promise<void> {
    await this.execute();
  }

  /**
   * next() - 获取下一个结果
   */
  async next(): Promise<E> {
    const results = await this.execute();
    if (results.length === 0) {
      throw new GremlinError('No more elements');
    }
    return results[0].value;
  }

  /**
   * tryNext() - 尝试获取下一个结果
   */
  async tryNext(): Promise<E | undefined> {
    try {
      return await this.next();
    } catch {
      return undefined;
    }
  }

  /**
   * hasNext() - 是否有下一个结果
   */
  async hasNext(): Promise<boolean> {
    const results = await this.execute();
    return results.length > 0;
  }

  // ============ 内部方法 ============

  /**
   * 添加步骤
   */
  private addStep<T extends GremlinStep>(step: T): GraphTraversal<S, E> {
    const newTraversal = this.clone();
    newTraversal.state.steps.push(step);
    newTraversal.compiled = false;
    newTraversal.results = undefined;
    return newTraversal;
  }

  /**
   * 克隆遍历
   */
  private clone(): GraphTraversal<S, E> {
    const newTraversal = new GraphTraversal<S, E>(this.store, this.state.steps, this.config);
    newTraversal.state = {
      steps: [...this.state.steps],
      currentStep: this.state.currentStep,
      labels: new Map(this.state.labels),
      sideEffects: new Map(this.state.sideEffects),
      barriers: new Set(this.state.barriers),
    };
    return newTraversal;
  }

  /**
   * 生成步骤ID
   */
  private generateStepId(): string {
    return `step_${this.state.steps.length}`;
  }

  /**
   * 判断是否为谓词
   */
  private isPredicate(value: unknown): value is Predicate {
    return value !== null && typeof value === 'object' && 'operator' in value!;
  }

  /**
   * 获取步骤列表
   */
  getSteps(): GremlinStep[] {
    return [...this.state.steps];
  }

  /**
   * 执行遍历
   */
  private async execute(): Promise<TraversalResult<E>[]> {
    if (this.results && this.compiled) {
      return this.results;
    }

    // 延迟导入执行器以避免循环依赖
    const { GremlinExecutor } = await import('./executor.js');
    const executor = new GremlinExecutor(this.store);

    this.results = executor.execute(this.state.steps);
    this.compiled = true;

    return this.results;
  }
}

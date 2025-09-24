/**
 * Gremlin 类型定义
 *
 * 定义 Gremlin 遍历中使用的核心数据类型和接口
 * 兼容 Apache TinkerPop 规范
 */

// 图元素基础接口
export interface Element {
  id: string | number;
  label: string;
  properties: Record<string, unknown>;
}

// 顶点（节点）
export interface Vertex extends Element {
  type: 'vertex';
}

// 边（关系）
export interface Edge extends Element {
  type: 'edge';
  inVertex: string | number; // 入顶点ID
  outVertex: string | number; // 出顶点ID
}

// 路径
export interface Path {
  objects: (Vertex | Edge)[];
  labels: string[][];
}

// 遍历器
export interface Traverser<T = Vertex | Edge> {
  get(): T;
  path(): Path;
  loops(): number;
  bulk(): number;
}

// 比较运算符
export enum P {
  eq = 'eq',
  neq = 'neq',
  lt = 'lt',
  lte = 'lte',
  gt = 'gt',
  gte = 'gte',
  inside = 'inside',
  outside = 'outside',
  between = 'between',
  within = 'within',
  without = 'without',
  startingWith = 'startingWith',
  endingWith = 'endingWith',
  containing = 'containing',
  notStartingWith = 'notStartingWith',
  notEndingWith = 'notEndingWith',
  notContaining = 'notContaining',
}

// 谓词
export interface Predicate {
  operator: P;
  value: unknown;
  other?: unknown; // 用于between等需要两个值的操作
}

// 作用域
export enum Scope {
  local = 'local',
  global = 'global',
}

// 列名
export enum Column {
  keys = 'keys',
  values = 'values',
}

// 排序
export enum Order {
  asc = 'asc',
  desc = 'desc',
  shuffle = 'shuffle',
}

// 遍历策略
export interface TraversalStrategy {
  name: string;
  configuration: Record<string, unknown>;
}

// 遍历副作用
export interface SideEffect<T = unknown> {
  key: string;
  value: T;
}

// 方向
export enum Direction {
  OUT = 'OUT',
  IN = 'IN',
  BOTH = 'BOTH',
}

// 基数
export enum Cardinality {
  single = 'single',
  list = 'list',
  set = 'set',
}

// 遍历指标
export interface TraversalMetrics {
  stepId: string;
  name: string;
  duration: number;
  counts: Map<string, number>;
  annotations: Map<string, unknown>;
  nested?: TraversalMetrics[];
}

// 遍历解释
export interface TraversalExplanation {
  original: string[];
  intermediate: string[][];
  final: string[];
}

// 图遍历统计
export interface GraphTraversalStats {
  vertices: number;
  edges: number;
  traversals: number;
  avgTraversalTime: number;
}

// 批处理配置
export interface BatchConfig {
  batchSize: number;
  timeout: number;
  concurrent: boolean;
}

// 遍历配置
export interface TraversalConfig {
  batch?: BatchConfig;
  strategies?: TraversalStrategy[];
  sideEffects?: Map<string, unknown>;
  requirements?: Set<string>;
}

// 子图
export interface SubGraph {
  vertices: Set<Vertex>;
  edges: Set<Edge>;
}

// 错误类型
export class GremlinError extends Error {
  constructor(
    message: string,
    public readonly step?: string,
  ) {
    super(message);
    this.name = 'GremlinError';
  }
}

export class UnsupportedStepError extends GremlinError {
  constructor(stepName: string) {
    super(`不支持的 Gremlin 步骤: ${stepName}`, stepName);
    this.name = 'UnsupportedStepError';
  }
}

export class TraversalError extends GremlinError {
  constructor(
    message: string,
    public readonly traversal?: string,
  ) {
    super(message);
    this.name = 'TraversalError';
  }
}

// 工具类型
export type ElementId = string | number;
export type PropertyKey = string;
export type PropertyValue = unknown;
export type Label = string;

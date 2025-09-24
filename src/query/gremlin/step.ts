/**
 * Gremlin 遍历步骤定义
 *
 * 定义各种 Gremlin 遍历步骤的结构和行为
 * 支持链式调用和组合
 */

import type {
  Vertex,
  Edge,
  Direction,
  Predicate,
  Scope,
  Column,
  Order,
  Cardinality,
  ElementId,
  PropertyKey,
  PropertyValue,
  Label,
} from './types.js';

// 步骤基础接口
export interface Step {
  type: string;
  id: string;
  labels?: string[];
}

// 起始步骤
export interface StartStep extends Step {
  type: 'start';
  elements: ElementId[];
}

// V() 步骤 - 获取顶点
export interface VStep extends Step {
  type: 'V';
  ids?: ElementId[];
}

// E() 步骤 - 获取边
export interface EStep extends Step {
  type: 'E';
  ids?: ElementId[];
}

// out() 步骤 - 出边遍历
export interface OutStep extends Step {
  type: 'out';
  edgeLabels?: string[];
}

// in() 步骤 - 入边遍历
export interface InStep extends Step {
  type: 'in';
  edgeLabels?: string[];
}

// both() 步骤 - 双向边遍历
export interface BothStep extends Step {
  type: 'both';
  edgeLabels?: string[];
}

// outE() 步骤 - 出边
export interface OutEStep extends Step {
  type: 'outE';
  edgeLabels?: string[];
}

// inE() 步骤 - 入边
export interface InEStep extends Step {
  type: 'inE';
  edgeLabels?: string[];
}

// bothE() 步骤 - 双向边
export interface BothEStep extends Step {
  type: 'bothE';
  edgeLabels?: string[];
}

// inV() 步骤 - 边的入顶点
export interface InVStep extends Step {
  type: 'inV';
}

// outV() 步骤 - 边的出顶点
export interface OutVStep extends Step {
  type: 'outV';
}

// bothV() 步骤 - 边的两端顶点
export interface BothVStep extends Step {
  type: 'bothV';
}

// otherV() 步骤 - 边的另一端顶点
export interface OtherVStep extends Step {
  type: 'otherV';
}

// has() 步骤 - 属性过滤
export interface HasStep extends Step {
  type: 'has';
  key?: string;
  predicate?: Predicate;
  value?: PropertyValue;
  label?: string;
}

// hasLabel() 步骤 - 标签过滤
export interface HasLabelStep extends Step {
  type: 'hasLabel';
  labels: string[];
}

// hasId() 步骤 - ID过滤
export interface HasIdStep extends Step {
  type: 'hasId';
  ids: ElementId[];
}

// hasKey() 步骤 - 属性键过滤
export interface HasKeyStep extends Step {
  type: 'hasKey';
  keys: string[];
}

// hasValue() 步骤 - 属性值过滤
export interface HasValueStep extends Step {
  type: 'hasValue';
  values: PropertyValue[];
}

// hasNot() 步骤 - 没有属性
export interface HasNotStep extends Step {
  type: 'hasNot';
  key: string;
}

// is() 步骤 - 值比较
export interface IsStep extends Step {
  type: 'is';
  predicate: Predicate;
}

// where() 步骤 - 复杂过滤
export interface WhereStep extends Step {
  type: 'where';
  traversal?: Step[];
  predicate?: Predicate;
}

// filter() 步骤 - 自定义过滤
export interface FilterStep extends Step {
  type: 'filter';
  predicate: (element: Vertex | Edge) => boolean;
}

// and() 步骤 - 逻辑与
export interface AndStep extends Step {
  type: 'and';
  traversals: Step[][];
}

// or() 步骤 - 逻辑或
export interface OrStep extends Step {
  type: 'or';
  traversals: Step[][];
}

// not() 步骤 - 逻辑非
export interface NotStep extends Step {
  type: 'not';
  traversal: Step[];
}

// range() 步骤 - 范围限制
export interface RangeStep extends Step {
  type: 'range';
  low: number;
  high: number;
  scope?: Scope;
}

// limit() 步骤 - 数量限制
export interface LimitStep extends Step {
  type: 'limit';
  limit: number;
  scope?: Scope;
}

// skip() 步骤 - 跳过
export interface SkipStep extends Step {
  type: 'skip';
  skip: number;
  scope?: Scope;
}

// tail() 步骤 - 末尾元素
export interface TailStep extends Step {
  type: 'tail';
  limit: number;
  scope?: Scope;
}

// coin() 步骤 - 随机过滤
export interface CoinStep extends Step {
  type: 'coin';
  probability: number;
}

// sample() 步骤 - 采样
export interface SampleStep extends Step {
  type: 'sample';
  amountToSample: number;
  scope?: Scope;
}

// order() 步骤 - 排序
export interface OrderStep extends Step {
  type: 'order';
  scope?: Scope;
  comparators?: Array<{
    key?: string;
    order: Order;
  }>;
}

// orderBy() 步骤 - 按属性排序
export interface OrderByStep extends Step {
  type: 'orderBy';
  key: string;
  order: Order;
  scope?: Scope;
}

// shuffle() 步骤 - 随机排序
export interface ShuffleStep extends Step {
  type: 'shuffle';
  scope?: Scope;
}

// dedup() 步骤 - 去重
export interface DedupStep extends Step {
  type: 'dedup';
  scope?: Scope;
  dedupLabels?: string[];
}

// as() 步骤 - 标记
export interface AsStep extends Step {
  type: 'as';
  stepLabel: string;
}

// select() 步骤 - 选择
export interface SelectStep extends Step {
  type: 'select';
  selectKeys: string[];
  pop?: 'first' | 'last' | 'all';
  by?: Step[];
}

// project() 步骤 - 投影
export interface ProjectStep extends Step {
  type: 'project';
  projectKeys: string[];
  by?: Step[][];
}

// values() 步骤 - 属性值
export interface ValuesStep extends Step {
  type: 'values';
  propertyKeys?: string[];
}

// valueMap() 步骤 - 值映射
export interface ValueMapStep extends Step {
  type: 'valueMap';
  includeTokens: boolean;
  propertyKeys?: string[];
}

// propertyMap() 步骤 - 属性映射
export interface PropertyMapStep extends Step {
  type: 'propertyMap';
  propertyKeys?: string[];
}

// properties() 步骤 - 属性对象
export interface PropertiesStep extends Step {
  type: 'properties';
  propertyKeys?: string[];
}

// elementMap() 步骤 - 元素映射
export interface ElementMapStep extends Step {
  type: 'elementMap';
  propertyKeys?: string[];
}

// id() 步骤 - ID
export interface IdStep extends Step {
  type: 'id';
}

// label() 步骤 - 标签
export interface LabelStep extends Step {
  type: 'label';
}

// key() 步骤 - 键
export interface KeyStep extends Step {
  type: 'key';
}

// value() 步骤 - 值
export interface ValueStep extends Step {
  type: 'value';
}

// constant() 步骤 - 常量
export interface ConstantStep extends Step {
  type: 'constant';
  value: unknown;
}

// identity() 步骤 - 恒等
export interface IdentityStep extends Step {
  type: 'identity';
}

// count() 步骤 - 计数
export interface CountStep extends Step {
  type: 'count';
  scope?: Scope;
}

// sum() 步骤 - 求和
export interface SumStep extends Step {
  type: 'sum';
  scope?: Scope;
}

// min() 步骤 - 最小值
export interface MinStep extends Step {
  type: 'min';
  scope?: Scope;
}

// max() 步骤 - 最大值
export interface MaxStep extends Step {
  type: 'max';
  scope?: Scope;
}

// mean() 步骤 - 平均值
export interface MeanStep extends Step {
  type: 'mean';
  scope?: Scope;
}

// fold() 步骤 - 折叠
export interface FoldStep extends Step {
  type: 'fold';
}

// unfold() 步骤 - 展开
export interface UnfoldStep extends Step {
  type: 'unfold';
}

// group() 步骤 - 分组
export interface GroupStep extends Step {
  type: 'group';
  keyTraversal?: Step[];
  valueTraversal?: Step[];
}

// groupCount() 步骤 - 分组计数
export interface GroupCountStep extends Step {
  type: 'groupCount';
  keyTraversal?: Step[];
}

// tree() 步骤 - 树结构
export interface TreeStep extends Step {
  type: 'tree';
}

// path() 步骤 - 路径
export interface PathStep extends Step {
  type: 'path';
  by?: Step[];
}

// simplePath() 步骤 - 简单路径
export interface SimplePathStep extends Step {
  type: 'simplePath';
}

// cyclicPath() 步骤 - 环路径
export interface CyclicPathStep extends Step {
  type: 'cyclicPath';
}

// repeat() 步骤 - 重复
export interface RepeatStep extends Step {
  type: 'repeat';
  traversal: Step[];
  times?: number;
  until?: Step[];
  emit?: Step[];
}

// until() 步骤 - 直到
export interface UntilStep extends Step {
  type: 'until';
  predicate: Step[];
}

// emit() 步骤 - 发射
export interface EmitStep extends Step {
  type: 'emit';
  predicate?: Step[];
}

// loops() 步骤 - 循环次数
export interface LoopsStep extends Step {
  type: 'loops';
  loopLabel?: string;
}

// times() 步骤 - 次数
export interface TimesStep extends Step {
  type: 'times';
  maxLoops: number;
}

// union() 步骤 - 联合
export interface UnionStep extends Step {
  type: 'union';
  traversals: Step[][];
}

// coalesce() 步骤 - 合并
export interface CoalesceStep extends Step {
  type: 'coalesce';
  traversals: Step[][];
}

// choose() 步骤 - 选择分支
export interface ChooseStep extends Step {
  type: 'choose';
  predicate: Step[] | Predicate;
  trueTraversal: Step[];
  falseTraversal?: Step[];
}

// optional() 步骤 - 可选
export interface OptionalStep extends Step {
  type: 'optional';
  traversal: Step[];
}

// local() 步骤 - 本地作用域
export interface LocalStep extends Step {
  type: 'local';
  localTraversal: Step[];
}

// barrier() 步骤 - 屏障
export interface BarrierStep extends Step {
  type: 'barrier';
  maxBarrierSize?: number;
}

// 步骤联合类型
export type GremlinStep =
  | StartStep
  | VStep
  | EStep
  | OutStep
  | InStep
  | BothStep
  | OutEStep
  | InEStep
  | BothEStep
  | InVStep
  | OutVStep
  | BothVStep
  | OtherVStep
  | HasStep
  | HasLabelStep
  | HasIdStep
  | HasKeyStep
  | HasValueStep
  | HasNotStep
  | IsStep
  | WhereStep
  | FilterStep
  | AndStep
  | OrStep
  | NotStep
  | RangeStep
  | LimitStep
  | SkipStep
  | TailStep
  | CoinStep
  | SampleStep
  | OrderStep
  | OrderByStep
  | ShuffleStep
  | DedupStep
  | AsStep
  | SelectStep
  | ProjectStep
  | ValuesStep
  | ValueMapStep
  | PropertyMapStep
  | PropertiesStep
  | ElementMapStep
  | IdStep
  | LabelStep
  | KeyStep
  | ValueStep
  | ConstantStep
  | IdentityStep
  | CountStep
  | SumStep
  | MinStep
  | MaxStep
  | MeanStep
  | FoldStep
  | UnfoldStep
  | GroupStep
  | GroupCountStep
  | TreeStep
  | PathStep
  | SimplePathStep
  | CyclicPathStep
  | RepeatStep
  | UntilStep
  | EmitStep
  | LoopsStep
  | TimesStep
  | UnionStep
  | CoalesceStep
  | ChooseStep
  | OptionalStep
  | LocalStep
  | BarrierStep;

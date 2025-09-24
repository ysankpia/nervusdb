/**
 * GraphQL Schema 生成器类型定义
 *
 * 为 SynapseDB 提供动态 GraphQL Schema 生成和查询解析能力
 * 支持从知识图谱自动推断 GraphQL 类型和解析器
 */

// GraphQL 基础类型
export enum GraphQLScalarType {
  String = 'String',
  Int = 'Int',
  Float = 'Float',
  Boolean = 'Boolean',
  ID = 'ID',
  JSON = 'JSON',
}

// GraphQL 字段定义
export interface GraphQLField {
  name: string;
  type: string;
  isArray?: boolean;
  isNullable?: boolean;
  description?: string;
  args?: GraphQLArgument[];
  resolver?: GraphQLResolver;
}

// GraphQL 参数定义
export interface GraphQLArgument {
  name: string;
  type: string;
  isNullable?: boolean;
  defaultValue?: unknown;
  description?: string;
}

// GraphQL 类型定义
export interface GraphQLType {
  name: string;
  kind: 'OBJECT' | 'INTERFACE' | 'UNION' | 'ENUM' | 'INPUT_OBJECT' | 'SCALAR';
  fields?: GraphQLField[];
  description?: string;
  interfaces?: string[];
  possibleTypes?: string[];
  enumValues?: string[];
}

// GraphQL 解析器函数
export interface GraphQLResolver {
  (
    parent: unknown,
    args: Record<string, unknown>,
    context: GraphQLContext,
  ): Promise<unknown> | unknown;
}

// GraphQL 上下文
export interface GraphQLContext {
  store: any; // PersistentStore
  loaders?: Record<string, any>;
  user?: unknown;
  headers?: Record<string, string>;
}

// Schema 生成配置
export interface SchemaGenerationConfig {
  // 实体发现配置
  maxSampleSize?: number; // 用于类型推断的最大样本数量
  minEntityCount?: number; // 生成类型的最小实体数量

  // 类型映射配置
  typeMapping?: Record<string, string>; // 自定义类型映射
  rootTypes?: {
    Query?: string[];
    Mutation?: string[];
    Subscription?: string[];
  };

  // 字段生成配置
  includeReverseRelations?: boolean; // 是否包含反向关系
  maxDepth?: number; // 关系遍历的最大深度
  fieldNaming?: 'camelCase' | 'snake_case' | 'preserve'; // 字段命名规范

  // 过滤配置
  excludeTypes?: string[]; // 排除的类型
  includeTypes?: string[]; // 仅包含的类型
  excludePredicates?: string[]; // 排除的谓词

  // 解析器配置
  enableDataLoader?: boolean; // 是否启用 DataLoader 批量加载
  cacheResolvers?: boolean; // 是否缓存解析器结果
}

// 实体类型推断结果
export interface EntityTypeInfo {
  typeName: string;
  count: number;
  sampleIds: (string | number)[];
  properties: PropertyInfo[];
  relations: RelationInfo[];
}

// 属性信息
export interface PropertyInfo {
  predicate: string;
  fieldName: string;
  valueType: GraphQLScalarType;
  isRequired: boolean;
  isArray: boolean;
  uniqueCount: number;
  samples: unknown[];
}

// 关系信息
export interface RelationInfo {
  predicate: string;
  fieldName: string;
  targetType: string;
  isArray: boolean;
  count: number;
  isReverse?: boolean; // 是否为反向关系
}

// Schema 构建结果
export interface GeneratedSchema {
  typeDefs: string; // GraphQL SDL
  resolvers: Record<string, Record<string, GraphQLResolver>>; // 解析器映射
  types: GraphQLType[]; // 类型定义
  statistics: SchemaStatistics;
}

// Schema 统计信息
export interface SchemaStatistics {
  typeCount: number;
  fieldCount: number;
  relationCount: number;
  entitiesAnalyzed: number;
  generationTime: number;
  schemaComplexity: number;
}

// GraphQL 查询解析结果
export interface ParsedGraphQLQuery {
  operationType: 'query' | 'mutation' | 'subscription';
  operationName?: string;
  fields: ParsedField[];
  variables: Record<string, unknown>;
  fragments: Record<string, ParsedFragment>;
}

// 解析的字段
export interface ParsedField {
  name: string;
  alias?: string;
  args: Record<string, unknown>;
  fields: ParsedField[];
  type: string;
}

// 解析的片段
export interface ParsedFragment {
  name: string;
  typeCondition: string;
  fields: ParsedField[];
}

// GraphQL 执行结果
export interface GraphQLExecutionResult {
  data?: Record<string, unknown>;
  errors?: GraphQLError[];
  extensions?: Record<string, unknown>;
}

// GraphQL 错误
export interface GraphQLError {
  message: string;
  locations?: { line: number; column: number }[];
  path?: (string | number)[];
  extensions?: Record<string, unknown>;
}

// 数据加载器配置
export interface DataLoaderConfig {
  batchSize?: number;
  cache?: boolean;
  cacheKeyFn?: (key: unknown) => string;
  cacheMap?: Map<string, unknown>;
}

// 解析器生成选项
export interface ResolverGenerationOptions {
  enablePagination?: boolean;
  enableFiltering?: boolean;
  enableSorting?: boolean;
  enableAggregation?: boolean;
  maxQueryDepth?: number;
  maxQueryComplexity?: number;
}

// 分页参数
export interface PaginationArgs {
  first?: number;
  after?: string;
  last?: number;
  before?: string;
}

// 排序参数
export interface SortArgs {
  field: string;
  direction: 'ASC' | 'DESC';
}

// 过滤参数
export interface FilterArgs {
  field: string;
  operator:
    | 'EQ'
    | 'NEQ'
    | 'LT'
    | 'LTE'
    | 'GT'
    | 'GTE'
    | 'IN'
    | 'NOT_IN'
    | 'CONTAINS'
    | 'STARTS_WITH'
    | 'ENDS_WITH';
  value: unknown;
}

// 聚合参数
export interface AggregationArgs {
  field: string;
  function: 'COUNT' | 'SUM' | 'AVG' | 'MIN' | 'MAX';
}

// 连接类型（Relay 规范）
export interface Connection<T> {
  edges: Edge<T>[];
  pageInfo: PageInfo;
  totalCount?: number;
}

export interface Edge<T> {
  node: T;
  cursor: string;
}

export interface PageInfo {
  hasNextPage: boolean;
  hasPreviousPage: boolean;
  startCursor?: string;
  endCursor?: string;
}

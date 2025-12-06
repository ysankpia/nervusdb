/**
 * Cypher 模式匹配 AST 节点定义
 *
 * 设计目标：
 * - 支持基础 Cypher 模式语法：(a:Label {prop: value})-[:REL*1..3]->(b)
 * - 与现有 PatternBuilder 保持兼容
 * - 支持渐进式功能扩展
 */

// 基础 AST 节点接口
export interface ASTNode {
  type: string;
  location?: SourceLocation;
}

export interface SourceLocation {
  start: Position;
  end: Position;
}

export interface Position {
  line: number;
  column: number;
  offset: number;
}

// 查询根节点
export interface CypherQuery extends ASTNode {
  type: 'CypherQuery';
  clauses: Clause[];
}

// 子句类型
export type Clause =
  | MatchClause
  | CreateClause
  | ReturnClause
  | WhereClause
  | WithClause
  | SetClause
  | DeleteClause
  | MergeClause
  | RemoveClause
  | UnionClause
  | UnwindClause;

// MATCH 子句
export interface MatchClause extends ASTNode {
  type: 'MatchClause';
  optional: boolean;
  pattern: Pattern;
}

// CREATE 子句
export interface CreateClause extends ASTNode {
  type: 'CreateClause';
  pattern: Pattern;
}

// RETURN 子句
export interface ReturnClause extends ASTNode {
  type: 'ReturnClause';
  distinct: boolean;
  items: ReturnItem[];
  orderBy?: OrderByClause;
  limit?: number;
  skip?: number;
}

export interface ReturnItem extends ASTNode {
  type: 'ReturnItem';
  expression: Expression;
  alias?: string;
}

// WHERE 子句
export interface WhereClause extends ASTNode {
  type: 'WhereClause';
  expression: Expression;
}

// WITH 子句
export interface WithClause extends ASTNode {
  type: 'WithClause';
  distinct: boolean;
  items: ReturnItem[];
  where?: WhereClause;
  orderBy?: OrderByClause;
  limit?: number;
  skip?: number;
}

// ORDER BY 子句
export interface OrderByClause extends ASTNode {
  type: 'OrderByClause';
  items: OrderByItem[];
}

export interface OrderByItem extends ASTNode {
  type: 'OrderByItem';
  expression: Expression;
  direction: 'ASC' | 'DESC';
}

// 模式定义
export interface Pattern extends ASTNode {
  type: 'Pattern';
  elements: PathElement[];
}

// 路径元素（节点或关系）
export type PathElement = NodePattern | RelationshipPattern;

// 节点模式：(variable:Label1:Label2 {prop1: value1, prop2: value2})
export interface NodePattern extends ASTNode {
  type: 'NodePattern';
  variable?: string;
  labels: string[];
  properties?: PropertyMap;
}

// 关系模式：-[variable:REL_TYPE*1..3 {prop: value}]->
export interface RelationshipPattern extends ASTNode {
  type: 'RelationshipPattern';
  variable?: string;
  types: string[];
  direction: Direction;
  properties?: PropertyMap;
  variableLength?: VariableLength;
}

export type Direction = 'LEFT_TO_RIGHT' | 'RIGHT_TO_LEFT' | 'UNDIRECTED';

// 变长关系：*1..3
export interface VariableLength extends ASTNode {
  type: 'VariableLength';
  min?: number;
  max?: number;
  uniqueness?: 'NODE' | 'EDGE' | 'NONE';
}

// 属性映射：{key1: value1, key2: value2}
export interface PropertyMap extends ASTNode {
  type: 'PropertyMap';
  properties: PropertyPair[];
}

export interface PropertyPair extends ASTNode {
  type: 'PropertyPair';
  key: string;
  value: Expression;
}

// 表达式系统
export type Expression =
  | Literal
  | Variable
  | PropertyAccess
  | BinaryExpression
  | UnaryExpression
  | FunctionCall
  | ListExpression
  | MapExpression
  | SubqueryExpression
  | CaseExpression
  | ParameterExpression;

// 字面量
export interface Literal extends ASTNode {
  type: 'Literal';
  value: string | number | boolean | null;
  raw: string;
}

// 变量引用
export interface Variable extends ASTNode {
  type: 'Variable';
  name: string;
}

// 属性访问：variable.property
export interface PropertyAccess extends ASTNode {
  type: 'PropertyAccess';
  object: Expression;
  property: string;
}

// 二元表达式：a > b, a AND b
export interface BinaryExpression extends ASTNode {
  type: 'BinaryExpression';
  operator: BinaryOperator;
  left: Expression;
  right: Expression;
}

export type BinaryOperator =
  | '='
  | '<>'
  | '!='
  | '<'
  | '<='
  | '>'
  | '>='
  | 'AND'
  | 'OR'
  | 'XOR'
  | 'IN'
  | 'NOT IN'
  | 'STARTS WITH'
  | 'ENDS WITH'
  | 'CONTAINS'
  | '+'
  | '-'
  | '*'
  | '/'
  | '%'
  | '^';

// 一元表达式：NOT a, -a
export interface UnaryExpression extends ASTNode {
  type: 'UnaryExpression';
  operator: UnaryOperator;
  argument: Expression;
}

export type UnaryOperator = 'NOT' | '-' | '+';

// 函数调用：COUNT(n), exists(pattern)
export interface FunctionCall extends ASTNode {
  type: 'FunctionCall';
  name: string;
  arguments: Expression[];
}

// 列表：[1, 2, 3]
export interface ListExpression extends ASTNode {
  type: 'ListExpression';
  elements: Expression[];
}

// 映射：{key1: value1, key2: value2}
export interface MapExpression extends ASTNode {
  type: 'MapExpression';
  properties: PropertyPair[];
}

// 工具函数：创建 AST 节点的便利方法
export function createNode<T extends ASTNode>(
  type: T['type'],
  props: Omit<T, 'type'>,
  location?: SourceLocation,
): T {
  return {
    type,
    location,
    ...props,
  } as T;
}

// 验证 AST 结构的接口
export interface ASTValidator {
  validate(ast: ASTNode): ValidationResult;
}

export interface ValidationResult {
  valid: boolean;
  errors: ValidationError[];
}

export interface ValidationError {
  message: string;
  location?: SourceLocation;
  severity: 'error' | 'warning';
}

// 子查询表达式：EXISTS {...}, NOT EXISTS {...}
export interface SubqueryExpression extends ASTNode {
  type: 'SubqueryExpression';
  operator: SubqueryOperator;
  query: SubqueryPattern;
}

export type SubqueryOperator = 'EXISTS' | 'NOT EXISTS';

// 子查询模式（简化的查询语法）
export interface SubqueryPattern extends ASTNode {
  type: 'SubqueryPattern';
  pattern: Pattern;
  where?: WhereClause;
}

// SET 子句：SET n.name = 'value', m.age = 30
export interface SetClause extends ASTNode {
  type: 'SetClause';
  items: SetItem[];
}

export interface SetItem extends ASTNode {
  type: 'SetItem';
  property: PropertyAccess;
  value: Expression;
}

// DELETE 子句：DELETE n, r
export interface DeleteClause extends ASTNode {
  type: 'DeleteClause';
  detach: boolean; // DETACH DELETE
  expressions: Expression[];
}

// MERGE 子句：MERGE (n:Label {prop: value})
export interface MergeClause extends ASTNode {
  type: 'MergeClause';
  pattern: Pattern;
  onCreate?: OnClause;
  onMatch?: OnClause;
}

export interface OnClause extends ASTNode {
  type: 'OnClause';
  items: SetItem[];
}

// REMOVE 子句：REMOVE n.prop, m:Label
export interface RemoveClause extends ASTNode {
  type: 'RemoveClause';
  items: RemoveItem[];
}

export type RemoveItem = RemovePropertyItem | RemoveLabelItem;

export interface RemovePropertyItem extends ASTNode {
  type: 'RemovePropertyItem';
  property: PropertyAccess;
}

export interface RemoveLabelItem extends ASTNode {
  type: 'RemoveLabelItem';
  variable: string;
  labels: string[];
}

// UNION 子句：UNION / UNION ALL
export interface UnionClause extends ASTNode {
  type: 'UnionClause';
  all: boolean;
  query: CypherQuery;
}

// UNWIND 子句：UNWIND collection AS item
export interface UnwindClause extends ASTNode {
  type: 'UnwindClause';
  expression: Expression;
  alias: string;
}

// CASE 表达式：CASE WHEN condition THEN value ELSE default END
export interface CaseExpression extends ASTNode {
  type: 'CaseExpression';
  expression?: Expression; // CASE expression WHEN...
  whenClauses: WhenClause[];
  elseExpression?: Expression;
}

export interface WhenClause extends ASTNode {
  type: 'WhenClause';
  condition: Expression;
  result: Expression;
}

// 参数表达式：$param
export interface ParameterExpression extends ASTNode {
  type: 'ParameterExpression';
  name: string;
}

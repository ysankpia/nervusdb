/**
 * Cypher 模式语法分析器
 *
 * 使用递归下降算法解析标记流为 AST
 * 支持的语法模式：
 * - 节点：(variable:Label {property: value})
 * - 关系：-[variable:TYPE*1..3 {property: value}]->
 * - 完整模式：MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE a.age > 25 RETURN a, b
 */

import type {
  ASTNode,
  CypherQuery,
  Clause,
  MatchClause,
  CreateClause,
  ReturnClause,
  WhereClause,
  WithClause,
  SetClause,
  SetItem,
  DeleteClause,
  MergeClause,
  OnClause,
  RemoveClause,
  RemoveItem,
  UnwindClause,
  UnionClause,
  Pattern,
  PathElement,
  NodePattern,
  RelationshipPattern,
  Expression,
  Literal,
  Variable,
  PropertyAccess,
  BinaryExpression,
  UnaryExpression,
  PropertyMap,
  PropertyPair,
  VariableLength,
  Direction,
  ReturnItem,
  OrderByClause,
  OrderByItem,
  SourceLocation,
  SubqueryExpression,
  SubqueryOperator,
  SubqueryPattern,
} from './ast.js';

import type { Token, TokenType } from './lexer.js';
import { CypherLexer } from './lexer.js';

export class ParseError extends Error {
  constructor(
    message: string,
    public location?: SourceLocation,
  ) {
    super(message);
    this.name = 'ParseError';
  }
}

export class CypherParser {
  private tokens: Token[] = [];
  private current = 0;

  parse(input: string): CypherQuery {
    const lexer = new CypherLexer(input);
    this.tokens = lexer.tokenize();
    this.current = 0;

    return this.parseQuery();
  }

  parseTokens(tokens: Token[]): CypherQuery {
    this.tokens = tokens;
    this.current = 0;

    return this.parseQuery();
  }

  // 解析查询根节点
  private parseQuery(): CypherQuery {
    const clauses: Clause[] = [];

    while (!this.isAtEnd()) {
      if (this.check('EOF')) break;

      const clause = this.parseClause();
      if (clause) {
        clauses.push(clause);
      }
    }

    return {
      type: 'CypherQuery',
      clauses,
    };
  }

  // 解析子句
  private parseClause(): Clause | null {
    if (this.match('MATCH')) {
      return this.parseMatch();
    }
    if (this.match('CREATE')) {
      return this.parseCreate();
    }
    if (this.match('RETURN')) {
      return this.parseReturn();
    }
    if (this.match('WHERE')) {
      return this.parseWhere();
    }
    if (this.match('WITH')) {
      return this.parseWith();
    }
    if (this.match('SET')) {
      return this.parseSet();
    }
    if (this.match('DELETE')) {
      return this.parseDelete();
    }
    if (this.match('DETACH')) {
      return this.parseDetachDelete();
    }
    if (this.match('MERGE')) {
      return this.parseMerge();
    }
    if (this.match('REMOVE')) {
      return this.parseRemove();
    }
    if (this.match('UNWIND')) {
      return this.parseUnwind();
    }
    if (this.match('UNION')) {
      return this.parseUnion();
    }

    // 如果遇到无法识别的标记，抛出错误而不是跳过
    if (!this.isAtEnd()) {
      const token = this.peek();
      throw new ParseError(`不支持的子句关键字: ${token.value}`, token.location);
    }
    return null;
  }

  // 解析 MATCH 子句
  private parseMatch(): MatchClause {
    const optional = this.previous().value.toUpperCase() === 'OPTIONAL';
    const pattern = this.parsePattern();

    return {
      type: 'MatchClause',
      optional,
      pattern,
    };
  }

  // 解析 CREATE 子句
  private parseCreate(): CreateClause {
    const pattern = this.parsePattern();

    return {
      type: 'CreateClause',
      pattern,
    };
  }

  // 解析 RETURN 子句
  private parseReturn(): ReturnClause {
    const distinct = this.match('DISTINCT');
    const items: ReturnItem[] = [];

    // 解析返回项
    do {
      const expression = this.parseExpression();
      let alias: string | undefined;

      if (this.check('IDENTIFIER') && this.peek().value.toUpperCase() === 'AS') {
        this.advance(); // consume 'AS'
        alias = this.consume('IDENTIFIER', '期望别名').value;
      }

      items.push({
        type: 'ReturnItem',
        expression,
        alias,
      });
    } while (this.match('COMMA'));

    // 解析可选的 ORDER BY
    let orderBy: OrderByClause | undefined;
    if (this.check('ORDER')) {
      orderBy = this.parseOrderBy();
    }

    // 解析可选的 LIMIT
    let limit: number | undefined;
    if (this.match('LIMIT')) {
      const limitToken = this.consume('NUMBER', '期望数字');
      limit = parseInt(limitToken.value, 10);
    }

    // 解析可选的 SKIP
    let skip: number | undefined;
    if (this.match('SKIP')) {
      const skipToken = this.consume('NUMBER', '期望数字');
      skip = parseInt(skipToken.value, 10);
    }

    return {
      type: 'ReturnClause',
      distinct,
      items,
      orderBy,
      limit,
      skip,
    };
  }

  // 解析 WHERE 子句
  private parseWhere(): WhereClause {
    const expression = this.parseExpression();

    return {
      type: 'WhereClause',
      expression,
    };
  }

  // 解析 WITH 子句
  private parseWith(): WithClause {
    const distinct = this.match('DISTINCT');
    const items: ReturnItem[] = [];

    // 解析项目
    do {
      const expression = this.parseExpression();
      let alias: string | undefined;

      if (this.check('IDENTIFIER') && this.peek().value.toUpperCase() === 'AS') {
        this.advance(); // consume 'AS'
        alias = this.consume('IDENTIFIER', '期望别名').value;
      }

      items.push({
        type: 'ReturnItem',
        expression,
        alias,
      });
    } while (this.match('COMMA'));

    // 解析可选的 WHERE
    let where: WhereClause | undefined;
    if (this.match('WHERE')) {
      const expression = this.parseExpression();
      where = {
        type: 'WhereClause',
        expression,
      };
    }

    // 解析可选的 ORDER BY
    let orderBy: OrderByClause | undefined;
    if (this.check('ORDER')) {
      orderBy = this.parseOrderBy();
    }

    // 解析可选的 LIMIT 和 SKIP
    let limit: number | undefined;
    let skip: number | undefined;

    if (this.match('LIMIT')) {
      const limitToken = this.consume('NUMBER', '期望数字');
      limit = parseInt(limitToken.value, 10);
    }

    if (this.match('SKIP')) {
      const skipToken = this.consume('NUMBER', '期望数字');
      skip = parseInt(skipToken.value, 10);
    }

    return {
      type: 'WithClause',
      distinct,
      items,
      where,
      orderBy,
      limit,
      skip,
    };
  }

  // 解析 ORDER BY 子句
  private parseOrderBy(): OrderByClause {
    this.consume('ORDER', '期望 ORDER');
    this.consume('BY', '期望 BY');

    const items: OrderByItem[] = [];

    do {
      const expression = this.parseExpression();
      let direction: 'ASC' | 'DESC' = 'ASC';

      if (this.match('ASC', 'DESC')) {
        direction = this.previous().value.toUpperCase() as 'ASC' | 'DESC';
      }

      items.push({
        type: 'OrderByItem',
        expression,
        direction,
      });
    } while (this.match('COMMA'));

    return {
      type: 'OrderByClause',
      items,
    };
  }

  // 解析模式
  private parsePattern(): Pattern {
    const elements: PathElement[] = [];

    // 解析第一个节点
    elements.push(this.parseNode());

    // 解析关系-节点链
    while (this.check('DASH') || this.check('LEFT_ARROW') || this.checkRelationshipStart()) {
      elements.push(this.parseRelationship());
      elements.push(this.parseNode());
    }

    return {
      type: 'Pattern',
      elements,
    };
  }

  // 解析节点模式：(variable:Label {prop: value})
  private parseNode(): NodePattern {
    this.consume('LEFT_PAREN', '期望 "("');

    let variable: string | undefined;
    const labels: string[] = [];
    let properties: PropertyMap | undefined;

    // 解析变量名
    if (this.check('IDENTIFIER')) {
      variable = this.advance().value;
    }

    // 解析标签
    while (this.match('COLON')) {
      const labelToken = this.consume('IDENTIFIER', '期望标签名');
      labels.push(labelToken.value);
    }

    // 解析属性
    if (this.check('LEFT_BRACE')) {
      properties = this.parsePropertyMap();
    }

    this.consume('RIGHT_PAREN', '期望 ")"');

    return {
      type: 'NodePattern',
      variable,
      labels,
      properties,
    };
  }

  // 解析关系模式：-[variable:TYPE*1..3 {prop: value}]->
  private parseRelationship(): RelationshipPattern {
    let direction: Direction = 'UNDIRECTED';

    // 解析方向前缀
    if (this.match('LEFT_ARROW')) {
      direction = 'RIGHT_TO_LEFT';
    } else if (this.match('DASH')) {
      // 可能是无向或右向
      direction = 'UNDIRECTED';
    }

    let variable: string | undefined;
    const types: string[] = [];
    let properties: PropertyMap | undefined;
    let variableLength: VariableLength | undefined;

    // 解析关系详情 [...]
    if (this.match('LEFT_BRACKET')) {
      // 解析变量名
      if (this.check('IDENTIFIER')) {
        variable = this.advance().value;
      }

      // 解析关系类型
      while (this.match('COLON')) {
        const typeToken = this.consume('IDENTIFIER', '期望关系类型');
        types.push(typeToken.value);
      }

      // 解析变长语法 *1..3
      if (this.match('ASTERISK')) {
        let min: number | undefined;
        let max: number | undefined;

        if (this.check('NUMBER')) {
          min = parseInt(this.advance().value, 10);

          if (this.match('RANGE_DOTS')) {
            if (this.check('NUMBER')) {
              max = parseInt(this.advance().value, 10);
            } else {
              max = Number.MAX_SAFE_INTEGER; // 无上限
            }
          } else {
            max = min; // 固定长度
          }
        } else {
          // 只有 * 表示任意长度
          min = 1;
          max = Number.MAX_SAFE_INTEGER;
        }

        variableLength = {
          type: 'VariableLength',
          min,
          max,
          uniqueness: 'NODE', // 默认节点唯一性
        };
      }

      // 解析属性
      if (this.check('LEFT_BRACE')) {
        properties = this.parsePropertyMap();
      }

      this.consume('RIGHT_BRACKET', '期望 "]"');
    }

    // 解析方向后缀
    if (this.match('RIGHT_ARROW')) {
      if (direction === 'RIGHT_TO_LEFT') {
        throw new ParseError('无效的关系方向: <->');
      }
      direction = 'LEFT_TO_RIGHT';
    } else if (this.match('DASH')) {
      // 保持当前方向
    }

    return {
      type: 'RelationshipPattern',
      variable,
      types,
      direction,
      properties,
      variableLength,
    };
  }

  // 解析属性映射：{key1: value1, key2: value2}
  private parsePropertyMap(): PropertyMap {
    this.consume('LEFT_BRACE', '期望 "{"');

    const properties: PropertyPair[] = [];

    if (!this.check('RIGHT_BRACE')) {
      do {
        const keyToken = this.consume('IDENTIFIER', '期望属性键');
        this.consume('COLON', '期望 ":"');
        const value = this.parseExpression();

        properties.push({
          type: 'PropertyPair',
          key: keyToken.value,
          value,
        });
      } while (this.match('COMMA'));
    }

    this.consume('RIGHT_BRACE', '期望 "}"');

    return {
      type: 'PropertyMap',
      properties,
    };
  }

  // 解析表达式（递归下降）
  private parseExpression(): Expression {
    return this.parseOr();
  }

  private parseOr(): Expression {
    let expr = this.parseAnd();

    while (this.match('OR')) {
      const operator = 'OR';
      const right = this.parseAnd();
      expr = {
        type: 'BinaryExpression',
        operator,
        left: expr,
        right,
      };
    }

    return expr;
  }

  private parseAnd(): Expression {
    let expr = this.parseEquality();

    while (this.match('AND')) {
      const operator = 'AND';
      const right = this.parseEquality();
      expr = {
        type: 'BinaryExpression',
        operator,
        left: expr,
        right,
      };
    }

    return expr;
  }

  private parseEquality(): Expression {
    let expr = this.parseComparison();

    while (this.match('EQUALS', 'NOT_EQUALS')) {
      const operator = this.previous().value === '=' ? '=' : '<>';
      const right = this.parseComparison();
      expr = {
        type: 'BinaryExpression',
        operator,
        left: expr,
        right,
      };
    }

    return expr;
  }

  private parseComparison(): Expression {
    let expr = this.parseInExpression();

    while (this.match('GREATER_THAN', 'GREATER_EQUAL', 'LESS_THAN', 'LESS_EQUAL')) {
      const tokenValue = this.previous().value;
      const operator =
        tokenValue === '>' ? '>' : tokenValue === '>=' ? '>=' : tokenValue === '<' ? '<' : '<=';
      const right = this.parseInExpression();
      expr = {
        type: 'BinaryExpression',
        operator,
        left: expr,
        right,
      };
    }

    return expr;
  }

  private parseInExpression(): Expression {
    let expr = this.parseAddition();

    while (
      this.match('IN') ||
      (this.check('NOT') && this.peekNext() && this.peekNext()!.type === 'IN')
    ) {
      let operator: 'IN' | 'NOT IN' = 'IN';

      if (this.previous().type === 'NOT') {
        this.advance(); // consume IN after NOT
        operator = 'NOT IN';
      } else if (this.check('NOT')) {
        this.advance(); // consume NOT
        this.consume('IN', '期望 IN');
        operator = 'NOT IN';
      }

      // IN 操作符后面应该跟一个列表或子查询
      const right = this.parseAddition();
      expr = {
        type: 'BinaryExpression',
        operator,
        left: expr,
        right,
      };
    }

    return expr;
  }

  private parseAddition(): Expression {
    let expr = this.parseMultiplication();

    while (this.match('PLUS', 'MINUS')) {
      const operator = this.previous().value === '+' ? '+' : '-';
      const right = this.parseMultiplication();
      expr = {
        type: 'BinaryExpression',
        operator,
        left: expr,
        right,
      };
    }

    return expr;
  }

  private parseMultiplication(): Expression {
    let expr = this.parseUnary();

    while (this.match('MULTIPLY', 'DIVIDE', 'MODULO')) {
      const tokenValue = this.previous().value;
      const operator = tokenValue === '*' ? '*' : tokenValue === '/' ? '/' : '%';
      const right = this.parseUnary();
      expr = {
        type: 'BinaryExpression',
        operator,
        left: expr,
        right,
      };
    }

    return expr;
  }

  private parseUnary(): Expression {
    // EXISTS 或 NOT EXISTS 子查询优先解析
    if (this.check('EXISTS') || (this.check('NOT') && this.peekNext()?.type === 'EXISTS')) {
      return this.parseSubquery();
    }

    if (this.match('NOT')) {
      // 普通的 NOT 表达式
      const argument = this.parseUnary();
      return {
        type: 'UnaryExpression',
        operator: 'NOT',
        argument,
      };
    }

    if (this.match('MINUS')) {
      const argument = this.parseUnary();
      return {
        type: 'UnaryExpression',
        operator: '-',
        argument,
      };
    }

    return this.parsePrimary();
  }

  private parsePrimary(): Expression {
    // EXISTS 子查询
    if (this.check('EXISTS')) {
      return this.parseSubquery();
    }

    // 字面量
    if (this.match('STRING', 'NUMBER', 'BOOLEAN', 'NULL')) {
      const token = this.previous();
      let value: string | number | boolean | null;

      switch (token.type) {
        case 'STRING':
          value = token.value;
          break;
        case 'NUMBER':
          value = token.value.includes('.') ? parseFloat(token.value) : parseInt(token.value, 10);
          break;
        case 'BOOLEAN':
          value = token.value.toUpperCase() === 'TRUE';
          break;
        case 'NULL':
          value = null;
          break;
        default:
          value = null;
          break;
      }

      return {
        type: 'Literal',
        value,
        raw: token.value,
      };
    }

    // 参数变量 ($param)
    if (this.match('VARIABLE')) {
      const name = this.previous().value;
      return {
        type: 'ParameterExpression',
        name: name.substring(1), // 移除 $ 前缀
      };
    }

    // 变量或属性访问
    if (this.match('IDENTIFIER')) {
      const name = this.previous().value;

      // 检查是否是属性访问
      if (this.match('DOT')) {
        const property = this.consume('IDENTIFIER', '期望属性名').value;
        return {
          type: 'PropertyAccess',
          object: {
            type: 'Variable',
            name,
          },
          property,
        };
      }

      return {
        type: 'Variable',
        name,
      };
    }

    // 列表表达式 [value1, value2, value3]
    if (this.match('LEFT_BRACKET')) {
      const elements: Expression[] = [];

      if (!this.check('RIGHT_BRACKET')) {
        do {
          elements.push(this.parseExpression());
        } while (this.match('COMMA'));
      }

      this.consume('RIGHT_BRACKET', '期望 "]"');

      return {
        type: 'ListExpression',
        elements,
      };
    }

    // 括号表达式
    if (this.match('LEFT_PAREN')) {
      const expr = this.parseExpression();
      this.consume('RIGHT_PAREN', '期望 ")"');
      return expr;
    }

    throw new ParseError(
      `意外的标记: ${this.peek().type} "${this.peek().value}"`,
      this.peek().location,
    );
  }

  // 解析子查询表达式：EXISTS {...} 或 NOT EXISTS {...}
  private parseSubquery(): SubqueryExpression {
    let operator: SubqueryOperator = 'EXISTS';

    // 检查是否是 NOT EXISTS
    if (this.check('NOT')) {
      this.advance(); // consume NOT
      operator = 'NOT EXISTS';
    }

    // 消费 EXISTS 关键字
    this.consume('EXISTS', '期望 EXISTS 关键字');

    // 消费左大括号
    this.consume('LEFT_BRACE', '期望 "{"');

    // 解析子查询的 MATCH 子句
    if (!this.match('MATCH')) {
      throw new ParseError('子查询必须以 MATCH 开始');
    }

    const pattern = this.parsePattern();

    // 可选的 WHERE 子句
    let where: WhereClause | undefined;
    if (this.match('WHERE')) {
      const expression = this.parseExpression();
      where = {
        type: 'WhereClause',
        expression,
      };
    }

    // 消费右大括号
    this.consume('RIGHT_BRACE', '期望 "}"');

    const query: SubqueryPattern = {
      type: 'SubqueryPattern',
      pattern,
      where,
    };

    return {
      type: 'SubqueryExpression',
      operator,
      query,
    };
  }

  // 工具方法
  private match(...types: TokenType[]): boolean {
    for (const type of types) {
      if (this.check(type)) {
        this.advance();
        return true;
      }
    }
    return false;
  }

  private check(type: TokenType): boolean {
    if (this.isAtEnd()) return false;
    return this.peek().type === type;
  }

  private checkRelationshipStart(): boolean {
    // 检查是否是关系开始：- 或者 [
    return this.check('DASH') || this.check('LEFT_BRACKET');
  }

  private advance(): Token {
    if (!this.isAtEnd()) this.current++;
    return this.previous();
  }

  private isAtEnd(): boolean {
    return this.peek().type === 'EOF';
  }

  private peek(): Token {
    return this.tokens[this.current];
  }

  private peekNext(): Token | undefined {
    if (this.current + 1 < this.tokens.length) {
      return this.tokens[this.current + 1];
    }
    return undefined;
  }

  private previous(): Token {
    return this.tokens[this.current - 1];
  }

  private consume(type: TokenType, message: string): Token {
    if (this.check(type)) return this.advance();

    const current = this.peek();
    throw new ParseError(`${message}，但得到 ${current.type} "${current.value}"`, current.location);
  }

  // SET 子句解析：SET n.name = 'value', m.age = 30
  private parseSet(): SetClause {
    const items: SetItem[] = [];

    do {
      const property = this.parsePropertyAccess();
      this.consume('EQUALS', '期望 "="');
      const value = this.parseExpression();

      items.push({
        type: 'SetItem',
        property: property as PropertyAccess,
        value,
      });
    } while (this.match('COMMA'));

    return {
      type: 'SetClause',
      items,
    };
  }

  // DELETE 子句解析：DELETE n, r
  private parseDelete(): DeleteClause {
    const expressions: Expression[] = [];

    do {
      expressions.push(this.parseExpression());
    } while (this.match('COMMA'));

    return {
      type: 'DeleteClause',
      detach: false,
      expressions,
    };
  }

  // DETACH DELETE 子句解析
  private parseDetachDelete(): DeleteClause {
    this.consume('DELETE', '期望 DELETE');
    const expressions: Expression[] = [];

    do {
      expressions.push(this.parseExpression());
    } while (this.match('COMMA'));

    return {
      type: 'DeleteClause',
      detach: true,
      expressions,
    };
  }

  // MERGE 子句解析：MERGE (n:Label {prop: value})
  private parseMerge(): MergeClause {
    const pattern = this.parsePattern();
    let onCreate: OnClause | undefined;
    let onMatch: OnClause | undefined;

    // 解析可选的 ON CREATE SET / ON MATCH SET
    while (this.match('ON')) {
      if (this.match('CREATE')) {
        this.consume('SET', '期望 SET');
        onCreate = this.parseOnClause();
      } else if (this.match('MATCH')) {
        this.consume('SET', '期望 SET');
        onMatch = this.parseOnClause();
      } else {
        throw new ParseError('期望 CREATE 或 MATCH');
      }
    }

    return {
      type: 'MergeClause',
      pattern,
      onCreate,
      onMatch,
    };
  }

  private parseOnClause(): OnClause {
    const items: SetItem[] = [];

    do {
      const property = this.parsePropertyAccess();
      this.consume('EQUALS', '期望 "="');
      const value = this.parseExpression();

      items.push({
        type: 'SetItem',
        property: property as PropertyAccess,
        value,
      });
    } while (this.match('COMMA'));

    return {
      type: 'OnClause',
      items,
    };
  }

  // REMOVE 子句解析：REMOVE n.prop, m:Label
  private parseRemove(): RemoveClause {
    const items: RemoveItem[] = [];

    do {
      if (this.check('IDENTIFIER')) {
        const variable = this.advance().value;

        if (this.match('COLON')) {
          // 移除标签：REMOVE n:Label
          const labels: string[] = [];
          do {
            labels.push(this.consume('IDENTIFIER', '期望标签名').value);
          } while (this.match('COLON'));

          items.push({
            type: 'RemoveLabelItem',
            variable,
            labels,
          });
        } else if (this.match('DOT')) {
          // 移除属性：REMOVE n.prop
          const property = this.consume('IDENTIFIER', '期望属性名').value;
          items.push({
            type: 'RemovePropertyItem',
            property: {
              type: 'PropertyAccess',
              object: { type: 'Variable', name: variable },
              property,
            },
          });
        } else {
          throw new ParseError('期望 ":" 或 "." 在变量之后');
        }
      } else {
        throw new ParseError('期望变量名');
      }
    } while (this.match('COMMA'));

    return {
      type: 'RemoveClause',
      items,
    };
  }

  // UNWIND 子句解析：UNWIND collection AS item
  private parseUnwind(): UnwindClause {
    const expression = this.parseExpression();
    this.consume('AS', '期望 AS');
    const alias = this.consume('IDENTIFIER', '期望别名').value;

    return {
      type: 'UnwindClause',
      expression,
      alias,
    };
  }

  // UNION 子句解析：UNION / UNION ALL
  private parseUnion(): UnionClause {
    const all = this.match('ALL');

    // 简化实现：直接解析剩余的子句作为单独的查询
    // 这里可以优化为更完整的UNION查询解析
    const subQuery: CypherQuery = {
      type: 'CypherQuery',
      clauses: [],
    };

    // 解析后续的子句直到结尾
    while (!this.isAtEnd()) {
      const clause = this.parseClause();
      if (clause) {
        subQuery.clauses.push(clause);
      }
    }

    return {
      type: 'UnionClause',
      all,
      query: subQuery,
    };
  }

  // 解析属性访问表达式：variable.property
  private parsePropertyAccess(): Expression {
    const variable = this.consume('IDENTIFIER', '期望变量名').value;
    this.consume('DOT', '期望 "."');
    const property = this.consume('IDENTIFIER', '期望属性名').value;

    return {
      type: 'PropertyAccess',
      object: { type: 'Variable', name: variable },
      property,
    };
  }
}

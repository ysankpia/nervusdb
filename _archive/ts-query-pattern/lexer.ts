/**
 * Cypher 模式词法分析器
 *
 * 功能：
 * - 将 Cypher 文本转换为标记流
 * - 支持基础语法：节点、关系、属性、变量等
 * - 提供位置信息用于错误报告
 */

import type { Position, SourceLocation } from './ast.js';

// 标记类型定义
export type TokenType =
  // 关键字
  | 'MATCH'
  | 'CREATE'
  | 'RETURN'
  | 'WHERE'
  | 'WITH'
  | 'OPTIONAL'
  | 'ORDER'
  | 'BY'
  | 'ASC'
  | 'DESC'
  | 'LIMIT'
  | 'SKIP'
  | 'DISTINCT'
  | 'AND'
  | 'OR'
  | 'NOT'
  | 'XOR'
  | 'IN'
  | 'STARTS'
  | 'ENDS'
  | 'CONTAINS'
  // 扩展的Cypher关键字
  | 'SET'
  | 'DELETE'
  | 'DETACH'
  | 'REMOVE'
  | 'MERGE'
  | 'UNION'
  | 'ALL'
  | 'UNWIND'
  | 'AS'
  | 'CASE'
  | 'WHEN'
  | 'THEN'
  | 'ELSE'
  | 'END'
  | 'CALL'
  | 'YIELD'
  | 'FOREACH'
  | 'ON'
  | 'EXISTS'

  // 符号
  | 'LEFT_PAREN'
  | 'RIGHT_PAREN' // ( )
  | 'LEFT_BRACKET'
  | 'RIGHT_BRACKET' // [ ]
  | 'LEFT_BRACE'
  | 'RIGHT_BRACE' // { }
  | 'COLON'
  | 'SEMICOLON'
  | 'COMMA' // : ; ,
  | 'DOT'
  | 'PIPE' // . |

  // 关系方向
  | 'LEFT_ARROW'
  | 'RIGHT_ARROW'
  | 'DASH' // <- -> -

  // 操作符
  | 'EQUALS'
  | 'NOT_EQUALS'
  | 'LESS_THAN'
  | 'LESS_EQUAL'
  | 'GREATER_THAN'
  | 'GREATER_EQUAL' // = <> != < <= > >=
  | 'PLUS'
  | 'MINUS'
  | 'MULTIPLY'
  | 'DIVIDE'
  | 'MODULO'
  | 'POWER'

  // 字面量
  | 'STRING'
  | 'NUMBER'
  | 'BOOLEAN'
  | 'NULL'

  // 标识符和变量
  | 'IDENTIFIER'
  | 'VARIABLE'

  // 特殊
  | 'ASTERISK' // * (用于变长关系)
  | 'RANGE_DOTS' // .. (用于范围)
  | 'EOF'
  | 'NEWLINE'
  | 'WHITESPACE';

// 标记定义
export interface Token {
  type: TokenType;
  value: string;
  location: SourceLocation;
}

// 关键字映射
const KEYWORDS = new Map<string, TokenType>([
  ['MATCH', 'MATCH'],
  ['CREATE', 'CREATE'],
  ['RETURN', 'RETURN'],
  ['WHERE', 'WHERE'],
  ['WITH', 'WITH'],
  ['OPTIONAL', 'OPTIONAL'],
  ['ORDER', 'ORDER'],
  ['BY', 'BY'],
  ['ASC', 'ASC'],
  ['DESC', 'DESC'],
  ['LIMIT', 'LIMIT'],
  ['SKIP', 'SKIP'],
  ['DISTINCT', 'DISTINCT'],
  ['AND', 'AND'],
  ['OR', 'OR'],
  ['NOT', 'NOT'],
  ['XOR', 'XOR'],
  ['IN', 'IN'],
  ['STARTS', 'STARTS'],
  ['ENDS', 'ENDS'],
  ['CONTAINS', 'CONTAINS'],
  // 扩展关键字
  ['SET', 'SET'],
  ['DELETE', 'DELETE'],
  ['DETACH', 'DETACH'],
  ['REMOVE', 'REMOVE'],
  ['MERGE', 'MERGE'],
  ['UNION', 'UNION'],
  ['ALL', 'ALL'],
  ['UNWIND', 'UNWIND'],
  ['AS', 'AS'],
  ['CASE', 'CASE'],
  ['WHEN', 'WHEN'],
  ['THEN', 'THEN'],
  ['ELSE', 'ELSE'],
  ['END', 'END'],
  ['CALL', 'CALL'],
  ['YIELD', 'YIELD'],
  ['FOREACH', 'FOREACH'],
  ['ON', 'ON'],
  ['EXISTS', 'EXISTS'],
  ['TRUE', 'BOOLEAN'],
  ['FALSE', 'BOOLEAN'],
  ['NULL', 'NULL'],
]);

export class CypherLexer {
  private input: string;
  private position = 0;
  private line = 1;
  private column = 1;

  constructor(input: string) {
    this.input = input;
  }

  // 标记化主方法
  tokenize(): Token[] {
    const tokens: Token[] = [];

    while (!this.isAtEnd()) {
      const startPos = this.getCurrentPosition();
      const token = this.nextToken();

      if (token) {
        tokens.push(token);
      }
    }

    // 添加 EOF 标记
    tokens.push({
      type: 'EOF',
      value: '',
      location: {
        start: this.getCurrentPosition(),
        end: this.getCurrentPosition(),
      },
    });

    return tokens;
  }

  private nextToken(): Token | null {
    const startPos = this.getCurrentPosition();
    const char = this.current();

    // 跳过空白字符
    if (this.isWhitespace(char)) {
      this.skipWhitespace();
      return null;
    }

    // 跳过注释
    if (char === '/' && this.peek() === '/') {
      this.skipLineComment();
      return null;
    }

    if (char === '/' && this.peek() === '*') {
      this.skipBlockComment();
      return null;
    }

    // 处理字符串字面量
    if (char === "'" || char === '"') {
      return this.readString(startPos);
    }

    // 处理数字字面量
    if (this.isDigit(char)) {
      return this.readNumber(startPos);
    }

    // 处理参数 ($param)
    if (char === '$') {
      return this.readParameter(startPos);
    }

    // 处理标识符和关键字
    if (this.isLetter(char)) {
      return this.readIdentifier(startPos);
    }

    // 处理两字符操作符
    const twoChar = char + this.peek();
    switch (twoChar) {
      case '<-':
        this.advance();
        this.advance();
        return this.createToken('LEFT_ARROW', '<-', startPos);
      case '->':
        this.advance();
        this.advance();
        return this.createToken('RIGHT_ARROW', '->', startPos);
      case '<>':
      case '!=':
        this.advance();
        this.advance();
        return this.createToken('NOT_EQUALS', twoChar, startPos);
      case '<=':
        this.advance();
        this.advance();
        return this.createToken('LESS_EQUAL', '<=', startPos);
      case '>=':
        this.advance();
        this.advance();
        return this.createToken('GREATER_EQUAL', '>=', startPos);
      case '..':
        this.advance();
        this.advance();
        return this.createToken('RANGE_DOTS', '..', startPos);
    }

    // 处理单字符标记
    switch (char) {
      case '(':
        this.advance();
        return this.createToken('LEFT_PAREN', '(', startPos);
      case ')':
        this.advance();
        return this.createToken('RIGHT_PAREN', ')', startPos);
      case '[':
        this.advance();
        return this.createToken('LEFT_BRACKET', '[', startPos);
      case ']':
        this.advance();
        return this.createToken('RIGHT_BRACKET', ']', startPos);
      case '{':
        this.advance();
        return this.createToken('LEFT_BRACE', '{', startPos);
      case '}':
        this.advance();
        return this.createToken('RIGHT_BRACE', '}', startPos);
      case ':':
        this.advance();
        return this.createToken('COLON', ':', startPos);
      case ';':
        this.advance();
        return this.createToken('SEMICOLON', ';', startPos);
      case ',':
        this.advance();
        return this.createToken('COMMA', ',', startPos);
      case '.':
        this.advance();
        return this.createToken('DOT', '.', startPos);
      case '|':
        this.advance();
        return this.createToken('PIPE', '|', startPos);
      case '-':
        this.advance();
        return this.createToken('DASH', '-', startPos);
      case '=':
        this.advance();
        return this.createToken('EQUALS', '=', startPos);
      case '<':
        this.advance();
        return this.createToken('LESS_THAN', '<', startPos);
      case '>':
        this.advance();
        return this.createToken('GREATER_THAN', '>', startPos);
      case '+':
        this.advance();
        return this.createToken('PLUS', '+', startPos);
      case '*':
        this.advance();
        return this.createToken('ASTERISK', '*', startPos);
      case '/':
        this.advance();
        return this.createToken('DIVIDE', '/', startPos);
      case '%':
        this.advance();
        return this.createToken('MODULO', '%', startPos);
      case '^':
        this.advance();
        return this.createToken('POWER', '^', startPos);
      default:
        throw new SyntaxError(`意外的字符: '${char}' 在位置 ${this.line}:${this.column}`);
    }
  }

  private readString(startPos: Position): Token {
    const quote = this.current();
    this.advance(); // 跳过开始引号

    let value = '';
    while (!this.isAtEnd() && this.current() !== quote) {
      if (this.current() === '\\') {
        this.advance();
        if (this.isAtEnd()) break;

        // 处理转义字符
        const escaped = this.current();
        switch (escaped) {
          case 'n':
            value += '\n';
            break;
          case 't':
            value += '\t';
            break;
          case 'r':
            value += '\r';
            break;
          case '\\':
            value += '\\';
            break;
          case "'":
            value += "'";
            break;
          case '"':
            value += '"';
            break;
          default:
            value += escaped;
            break;
        }
      } else {
        value += this.current();
      }
      this.advance();
    }

    if (this.isAtEnd()) {
      throw new SyntaxError('未终止的字符串字面量');
    }

    this.advance(); // 跳过结束引号
    return this.createToken('STRING', value, startPos);
  }

  private readNumber(startPos: Position): Token {
    let value = '';

    // 读取整数部分
    while (!this.isAtEnd() && this.isDigit(this.current())) {
      value += this.current();
      this.advance();
    }

    // 读取小数部分
    if (this.current() === '.' && this.isDigit(this.peek())) {
      value += this.current();
      this.advance();

      while (!this.isAtEnd() && this.isDigit(this.current())) {
        value += this.current();
        this.advance();
      }
    }

    // 读取指数部分
    if (this.current() === 'e' || this.current() === 'E') {
      value += this.current();
      this.advance();

      if (this.current() === '+' || this.current() === '-') {
        value += this.current();
        this.advance();
      }

      while (!this.isAtEnd() && this.isDigit(this.current())) {
        value += this.current();
        this.advance();
      }
    }

    return this.createToken('NUMBER', value, startPos);
  }

  private readIdentifier(startPos: Position): Token {
    let value = '';

    while (!this.isAtEnd() && (this.isAlphaNumeric(this.current()) || this.current() === '_')) {
      value += this.current();
      this.advance();
    }

    const upperValue = value.toUpperCase();
    const tokenType = KEYWORDS.get(upperValue) || 'IDENTIFIER';

    return this.createToken(tokenType, value, startPos);
  }

  private readParameter(startPos: Position): Token {
    let value = '';
    this.advance(); // 跳过 '$'

    // 参数名必须是有效标识符
    if (!this.isLetter(this.current())) {
      throw new SyntaxError(`无效的参数名: $ 后必须跟字母`);
    }

    while (!this.isAtEnd() && (this.isAlphaNumeric(this.current()) || this.current() === '_')) {
      value += this.current();
      this.advance();
    }

    return this.createToken('VARIABLE', '$' + value, startPos);
  }

  private skipWhitespace(): void {
    while (!this.isAtEnd() && this.isWhitespace(this.current())) {
      this.advance();
    }
  }

  private skipLineComment(): void {
    while (!this.isAtEnd() && this.current() !== '\n') {
      this.advance();
    }
  }

  private skipBlockComment(): void {
    this.advance(); // skip '/'
    this.advance(); // skip '*'

    while (!this.isAtEnd()) {
      if (this.current() === '*' && this.peek() === '/') {
        this.advance(); // skip '*'
        this.advance(); // skip '/'
        break;
      }
      this.advance();
    }
  }

  // 工具方法
  private current(): string {
    return this.isAtEnd() ? '\0' : this.input[this.position];
  }

  private peek(): string {
    return this.position + 1 >= this.input.length ? '\0' : this.input[this.position + 1];
  }

  private advance(): string {
    const char = this.current();
    this.position++;

    if (char === '\n') {
      this.line++;
      this.column = 1;
    } else {
      this.column++;
    }

    return char;
  }

  private isAtEnd(): boolean {
    return this.position >= this.input.length;
  }

  private isWhitespace(char: string): boolean {
    return /\s/.test(char);
  }

  private isDigit(char: string): boolean {
    return /\d/.test(char);
  }

  private isLetter(char: string): boolean {
    return /[a-zA-Z]/.test(char);
  }

  private isAlphaNumeric(char: string): boolean {
    return this.isLetter(char) || this.isDigit(char);
  }

  private getCurrentPosition(): Position {
    return {
      line: this.line,
      column: this.column,
      offset: this.position,
    };
  }

  private createToken(type: TokenType, value: string, startPos: Position): Token {
    return {
      type,
      value,
      location: {
        start: startPos,
        end: this.getCurrentPosition(),
      },
    };
  }
}

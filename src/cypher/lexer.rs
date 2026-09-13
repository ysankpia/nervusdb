use crate::graph::{GraphError, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // 关键字
    Create,
    Match,
    Where,
    Explain,
    Unwind,
    Merge,
    On,
    Return,
    Limit,
    Skip,
    Delete,
    Detach,
    Set,
    Order,
    By,
    Asc,
    Desc,
    And,
    Or,
    As,

    // 聚合函数
    Count,
    Sum,
    Avg,
    Min,
    Max,

    // 标识符与字面量
    Ident(String),
    Literal(Value),

    // 标点与语法符号
    LParen,     // (
    RParen,     // )
    LBracket,   // [
    RBracket,   // ]
    LBrace,     // {
    RBrace,     // }
    Colon,      // :
    Comma,      // ,
    Dot,        // .
    Semicolon,  // ;
    Dash,       // -
    ArrowRight, // ->
    ArrowLeft,  // <-
    Star,       // *
    DotDot,     // ..

    // 比较与逻辑
    Eq,  // =
    Neq, // !=
    Lt,  // <
    Lte, // <=
    Gt,  // >
    Gte, // >=
}

pub struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            chars: input.chars().peekable(),
        }
    }

    /// 前瞻第 2 个字符（不消费）
    fn peek_second(&self) -> Option<char> {
        let mut it = self.chars.clone();
        it.next();
        it.next()
    }

    /// 前瞻第 3 个字符（不消费）
    fn peek_third(&self) -> Option<char> {
        let mut it = self.chars.clone();
        it.next();
        it.next();
        it.next()
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>, GraphError> {
        let mut tokens = Vec::new();
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() {
                self.chars.next();
                continue;
            }

            match c {
                // 行注释：`//` 与 `--`（后者要求后续字符不是 '>'，避免与 `-->` 箭头歧义）
                '/' if self.peek_second() == Some('/') => {
                    self.chars.next();
                    self.chars.next();
                    for ch in self.chars.by_ref() {
                        if ch == '\n' {
                            break;
                        }
                    }
                }
                '-' if self.peek_second() == Some('-') && self.peek_third() != Some('>') => {
                    self.chars.next();
                    self.chars.next();
                    for ch in self.chars.by_ref() {
                        if ch == '\n' {
                            break;
                        }
                    }
                }
                '(' => {
                    self.chars.next();
                    tokens.push(Token::LParen);
                }
                ')' => {
                    self.chars.next();
                    tokens.push(Token::RParen);
                }
                '[' => {
                    self.chars.next();
                    tokens.push(Token::LBracket);
                }
                ']' => {
                    self.chars.next();
                    tokens.push(Token::RBracket);
                }
                '{' => {
                    self.chars.next();
                    tokens.push(Token::LBrace);
                }
                '}' => {
                    self.chars.next();
                    tokens.push(Token::RBrace);
                }
                ':' => {
                    self.chars.next();
                    tokens.push(Token::Colon);
                }
                ',' => {
                    self.chars.next();
                    tokens.push(Token::Comma);
                }
                ';' => {
                    self.chars.next();
                    tokens.push(Token::Semicolon);
                }
                '*' => {
                    self.chars.next();
                    tokens.push(Token::Star);
                }
                '.' => {
                    self.chars.next();
                    if self.chars.peek() == Some(&'.') {
                        self.chars.next();
                        tokens.push(Token::DotDot);
                    } else {
                        tokens.push(Token::Dot);
                    }
                }
                '-' => {
                    self.chars.next();
                    if self.chars.peek() == Some(&'>') {
                        self.chars.next();
                        tokens.push(Token::ArrowRight);
                    } else if matches!(self.chars.peek(), Some(c) if c.is_ascii_digit()) {
                        // 负数字面量：`-7`、`-7.5`。
                        //
                        // 负号在这里直接折进数字，而不是先发一个 `Dash` 再让语法层拼。
                        // 原因是**词法与语法的接缝处**：`SET n.k = -7` 与模式里的
                        // `{v: -7.5}` 都走 `parse_primary_expr`，它只接受单个 primary
                        // token，没有一元运算符的概念；在那里补一个 `Expr::Unary`
                        // 会牵动求值器，而折进字面量对两侧都透明。
                        //
                        // 不这样做的话，`dump_cypher` 会**写出自己的解析器读不回来的
                        // 脚本**：属性值为负的库 dump 出 `SET n.v = -42`，重导入直接
                        // 报 `Unexpected expression token: Some(Dash)`。文档把
                        // dump→re-import 指定为版本迁移路径，因此这条往返必须成立。
                        let mut num_str = String::from("-");
                        let mut is_float = false;
                        while let Some(&ch) = self.chars.peek() {
                            if ch.is_ascii_digit() {
                                num_str.push(ch);
                                self.chars.next();
                            } else if ch == '.'
                                && !is_float
                                && !matches!(self.chars.clone().nth(1), Some('.'))
                            {
                                is_float = true;
                                num_str.push('.');
                                self.chars.next();
                            } else {
                                break;
                            }
                        }
                        if is_float {
                            let val: f64 = num_str.parse().map_err(|e| {
                                GraphError::General(format!("Invalid float literal: {}", e))
                            })?;
                            tokens.push(Token::Literal(Value::from(val)));
                        } else {
                            let val: i64 = num_str.parse().map_err(|e| {
                                GraphError::General(format!("Invalid integer literal: {}", e))
                            })?;
                            tokens.push(Token::Literal(Value::from(val)));
                        }
                    } else {
                        tokens.push(Token::Dash);
                    }
                }
                '<' => {
                    self.chars.next();
                    if self.chars.peek() == Some(&'-') {
                        self.chars.next();
                        tokens.push(Token::ArrowLeft);
                    } else if self.chars.peek() == Some(&'=') {
                        self.chars.next();
                        tokens.push(Token::Lte);
                    } else {
                        tokens.push(Token::Lt);
                    }
                }
                '>' => {
                    self.chars.next();
                    if self.chars.peek() == Some(&'=') {
                        self.chars.next();
                        tokens.push(Token::Gte);
                    } else {
                        tokens.push(Token::Gt);
                    }
                }
                '=' => {
                    self.chars.next();
                    tokens.push(Token::Eq);
                }
                '!' => {
                    self.chars.next();
                    if self.chars.peek() == Some(&'=') {
                        self.chars.next();
                        tokens.push(Token::Neq);
                    } else {
                        return Err(GraphError::General("Unexpected character: '!'".into()));
                    }
                }
                '"' | '\'' => {
                    let quote = c;
                    self.chars.next();
                    let mut s = String::new();
                    let mut closed = false;
                    for ch in self.chars.by_ref() {
                        if ch == quote {
                            closed = true;
                            break;
                        }
                        s.push(ch);
                    }
                    if !closed {
                        return Err(GraphError::General("Unterminated string literal".into()));
                    }
                    tokens.push(Token::Literal(Value::from(s)));
                }
                _ if c.is_ascii_digit() => {
                    let mut num_str = String::new();
                    let mut is_float = false;
                    while let Some(&ch) = self.chars.peek() {
                        if ch.is_ascii_digit() {
                            num_str.push(ch);
                            self.chars.next();
                        } else if ch == '.' {
                            // 查看后一个字符是否还是点（如 1..3）
                            let mut clone_iter = self.chars.clone();
                            clone_iter.next();
                            if clone_iter.peek() == Some(&'.') {
                                break;
                            }
                            if is_float {
                                break;
                            }
                            is_float = true;
                            num_str.push('.');
                            self.chars.next();
                        } else {
                            break;
                        }
                    }

                    if is_float {
                        let val: f64 = num_str.parse().map_err(|e| {
                            GraphError::General(format!("Invalid float literal: {}", e))
                        })?;
                        tokens.push(Token::Literal(Value::from(val)));
                    } else {
                        let val: i64 = num_str.parse().map_err(|e| {
                            GraphError::General(format!("Invalid integer literal: {}", e))
                        })?;
                        tokens.push(Token::Literal(Value::from(val)));
                    }
                }
                _ if c.is_alphabetic() || c == '_' => {
                    let mut ident = String::new();
                    while let Some(&ch) = self.chars.peek() {
                        if ch.is_alphanumeric() || ch == '_' {
                            ident.push(ch);
                            self.chars.next();
                        } else {
                            break;
                        }
                    }

                    match ident.to_uppercase().as_str() {
                        "CREATE" => tokens.push(Token::Create),
                        "MATCH" => tokens.push(Token::Match),
                        "WHERE" => tokens.push(Token::Where),
                        "EXPLAIN" => tokens.push(Token::Explain),
                        "UNWIND" => tokens.push(Token::Unwind),
                        "MERGE" => tokens.push(Token::Merge),
                        "ON" => tokens.push(Token::On),
                        "RETURN" => tokens.push(Token::Return),
                        "LIMIT" => tokens.push(Token::Limit),
                        "SKIP" => tokens.push(Token::Skip),
                        "DELETE" => tokens.push(Token::Delete),
                        "DETACH" => tokens.push(Token::Detach),
                        "SET" => tokens.push(Token::Set),
                        "ORDER" => tokens.push(Token::Order),
                        "BY" => tokens.push(Token::By),
                        "ASC" => tokens.push(Token::Asc),
                        "DESC" => tokens.push(Token::Desc),
                        "AND" => tokens.push(Token::And),
                        "OR" => tokens.push(Token::Or),
                        "AS" => tokens.push(Token::As),
                        "COUNT" => tokens.push(Token::Count),
                        "SUM" => tokens.push(Token::Sum),
                        "AVG" => tokens.push(Token::Avg),
                        "MIN" => tokens.push(Token::Min),
                        "MAX" => tokens.push(Token::Max),
                        "TRUE" => tokens.push(Token::Literal(Value::from(true))),
                        "FALSE" => tokens.push(Token::Literal(Value::from(false))),
                        _ => tokens.push(Token::Ident(ident)),
                    }
                }
                _ => {
                    return Err(GraphError::General(format!(
                        "Unexpected character in query: '{}'",
                        c
                    )));
                }
            }
        }

        Ok(tokens)
    }
}

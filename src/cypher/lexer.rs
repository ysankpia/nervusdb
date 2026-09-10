use crate::graph::{GraphError, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // 关键字
    Create,
    Match,
    Where,
    Return,
    Limit,
    Delete,
    Detach,
    And,
    Or,
    As,

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

    pub fn tokenize(mut self) -> Result<Vec<Token>, GraphError> {
        let mut tokens = Vec::new();

        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() {
                self.chars.next();
                continue;
            }

            match c {
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
                        "RETURN" => tokens.push(Token::Return),
                        "LIMIT" => tokens.push(Token::Limit),
                        "DELETE" => tokens.push(Token::Delete),
                        "DETACH" => tokens.push(Token::Detach),
                        "AND" => tokens.push(Token::And),
                        "OR" => tokens.push(Token::Or),
                        "AS" => tokens.push(Token::As),
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

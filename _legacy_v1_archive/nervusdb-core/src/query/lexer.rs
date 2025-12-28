use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // Keywords
    Match,
    Create,
    Return,
    Where,
    With,
    Optional,
    Order,
    By,
    Asc,
    Desc,
    Limit,
    Skip,
    Distinct,
    And,
    Or,
    Not,
    Xor,
    In,
    Starts,
    Ends,
    Contains,
    Set,
    Delete,
    Detach,
    Remove,
    Merge,
    Union,
    All,
    Unwind,
    As,
    Case,
    When,
    Then,
    Else,
    End,
    Call,
    Yield,
    Foreach,
    On,
    Exists,

    // Symbols
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Colon,
    Semicolon,
    Comma,
    Dot,
    Pipe,

    // Relationships
    LeftArrow,
    RightArrow,
    Dash,

    // Operators
    Equals,
    NotEquals,
    LessThan,
    LessEqual,
    GreaterThan,
    GreaterEqual,
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,
    Power,

    // Literals
    String(String),
    Number(f64),
    Boolean(bool),
    Null,

    // Identifiers
    Identifier(String),
    Variable(String), // $param

    // Special
    Asterisk,
    RangeDots,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub token_type: TokenType,
    pub line: usize,
    pub column: usize,
}

pub struct Lexer<'a> {
    chars: Peekable<Chars<'a>>,
    position: usize,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            chars: input.chars().peekable(),
            position: 0,
            line: 1,
            column: 1,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        while let Some(token) = self.next_token()? {
            tokens.push(token);
        }
        tokens.push(Token {
            token_type: TokenType::Eof,
            line: self.line,
            column: self.column,
        });
        Ok(tokens)
    }

    fn next_token(&mut self) -> Result<Option<Token>, String> {
        self.skip_whitespace();

        if self.chars.peek().is_none() {
            return Ok(None);
        }

        let start_line = self.line;
        let start_column = self.column;
        let char = self.advance().unwrap();

        // Comments
        if char == '/' {
            if let Some(&'/') = self.chars.peek() {
                self.skip_line_comment();
                return self.next_token();
            } else if let Some(&'*') = self.chars.peek() {
                self.skip_block_comment();
                return self.next_token();
            }
        }

        // String literals
        if char == '\'' || char == '"' {
            return Ok(Some(self.read_string(char, start_line, start_column)?));
        }

        // Number literals
        if char.is_ascii_digit() {
            return Ok(Some(self.read_number(char, start_line, start_column)?));
        }

        // Parameters ($param)
        if char == '$' {
            return Ok(Some(self.read_parameter(start_line, start_column)?));
        }

        // Identifiers and Keywords
        if char.is_alphabetic() {
            return Ok(Some(self.read_identifier(
                char,
                start_line,
                start_column,
            )?));
        }

        // Operators and Symbols
        let token_type = match char {
            '(' => TokenType::LeftParen,
            ')' => TokenType::RightParen,
            '[' => TokenType::LeftBracket,
            ']' => TokenType::RightBracket,
            '{' => TokenType::LeftBrace,
            '}' => TokenType::RightBrace,
            ':' => TokenType::Colon,
            ';' => TokenType::Semicolon,
            ',' => TokenType::Comma,
            '.' => {
                if let Some(&'.') = self.chars.peek() {
                    self.advance();
                    TokenType::RangeDots
                } else {
                    TokenType::Dot
                }
            }
            '|' => TokenType::Pipe,
            '-' => {
                if let Some(&'>') = self.chars.peek() {
                    self.advance();
                    TokenType::RightArrow
                } else {
                    TokenType::Dash
                }
            }
            '<' => {
                if let Some(&'-') = self.chars.peek() {
                    self.advance();
                    TokenType::LeftArrow
                } else if let Some(&'=') = self.chars.peek() {
                    self.advance();
                    TokenType::LessEqual
                } else if let Some(&'>') = self.chars.peek() {
                    self.advance();
                    TokenType::NotEquals
                } else {
                    TokenType::LessThan
                }
            }
            '>' => {
                if let Some(&'=') = self.chars.peek() {
                    self.advance();
                    TokenType::GreaterEqual
                } else {
                    TokenType::GreaterThan
                }
            }
            '=' => TokenType::Equals,
            '!' => {
                if let Some(&'=') = self.chars.peek() {
                    self.advance();
                    TokenType::NotEquals
                } else {
                    return Err(format!(
                        "Unexpected character '!' at {}:{}",
                        self.line, self.column
                    ));
                }
            }
            '+' => TokenType::Plus,
            '*' => TokenType::Asterisk,
            '/' => TokenType::Divide,
            '%' => TokenType::Modulo,
            '^' => TokenType::Power,
            _ => {
                return Err(format!(
                    "Unexpected character '{}' at {}:{}",
                    char, self.line, self.column
                ));
            }
        };

        Ok(Some(Token {
            token_type,
            line: start_line,
            column: start_column,
        }))
    }

    fn advance(&mut self) -> Option<char> {
        let char = self.chars.next();
        if let Some(c) = char {
            self.position += c.len_utf8();
            if c == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
        char
    }

    fn skip_whitespace(&mut self) {
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(&c) = self.chars.peek() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn skip_block_comment(&mut self) {
        self.advance(); // skip *
        while let Some(c) = self.advance() {
            if c == '*'
                && let Some(&'/') = self.chars.peek()
            {
                self.advance();
                return;
            }
        }
    }

    fn read_string(&mut self, quote: char, line: usize, column: usize) -> Result<Token, String> {
        let mut value = String::new();
        while let Some(&c) = self.chars.peek() {
            if c == quote {
                self.advance();
                return Ok(Token {
                    token_type: TokenType::String(value),
                    line,
                    column,
                });
            }
            if c == '\\' {
                self.advance();
                if let Some(escaped) = self.advance() {
                    match escaped {
                        'n' => value.push('\n'),
                        't' => value.push('\t'),
                        'r' => value.push('\r'),
                        '\\' => value.push('\\'),
                        '\'' => value.push('\''),
                        '"' => value.push('"'),
                        _ => value.push(escaped),
                    }
                }
            } else {
                value.push(self.advance().unwrap());
            }
        }
        Err("Unterminated string literal".to_string())
    }

    fn read_number(&mut self, first: char, line: usize, column: usize) -> Result<Token, String> {
        let mut value = String::new();
        value.push(first);

        while let Some(&c) = self.chars.peek() {
            if c.is_ascii_digit() {
                value.push(self.advance().unwrap());
            } else {
                break;
            }
        }

        if let Some(&'.') = self.chars.peek() {
            // Disambiguate `1..2` (range) from `1.23` (float).
            // If the next char is also '.', this is a range operator and we must NOT
            // consume the '.' as part of the number.
            let mut lookahead = self.chars.clone();
            lookahead.next(); // consume the '.'
            let is_range = matches!(lookahead.peek(), Some('.'));

            if !is_range {
                value.push(self.advance().unwrap());
            }
            while let Some(&c) = self.chars.peek() {
                if c.is_ascii_digit() {
                    value.push(self.advance().unwrap());
                } else {
                    break;
                }
            }
        }

        let num: f64 = value
            .parse()
            .map_err(|_| "Invalid number format".to_string())?;
        Ok(Token {
            token_type: TokenType::Number(num),
            line,
            column,
        })
    }

    fn read_identifier(
        &mut self,
        first: char,
        line: usize,
        column: usize,
    ) -> Result<Token, String> {
        let mut value = String::new();
        value.push(first);

        while let Some(&c) = self.chars.peek() {
            if c.is_alphanumeric() || c == '_' {
                value.push(self.advance().unwrap());
            } else {
                break;
            }
        }

        let token_type = match value.to_uppercase().as_str() {
            "MATCH" => TokenType::Match,
            "CREATE" => TokenType::Create,
            "RETURN" => TokenType::Return,
            "WHERE" => TokenType::Where,
            "WITH" => TokenType::With,
            "OPTIONAL" => TokenType::Optional,
            "ORDER" => TokenType::Order,
            "BY" => TokenType::By,
            "ASC" => TokenType::Asc,
            "DESC" => TokenType::Desc,
            "LIMIT" => TokenType::Limit,
            "SKIP" => TokenType::Skip,
            "DISTINCT" => TokenType::Distinct,
            "AND" => TokenType::And,
            "OR" => TokenType::Or,
            "NOT" => TokenType::Not,
            "XOR" => TokenType::Xor,
            "IN" => TokenType::In,
            "STARTS" => TokenType::Starts,
            "ENDS" => TokenType::Ends,
            "CONTAINS" => TokenType::Contains,
            "SET" => TokenType::Set,
            "DELETE" => TokenType::Delete,
            "DETACH" => TokenType::Detach,
            "REMOVE" => TokenType::Remove,
            "MERGE" => TokenType::Merge,
            "UNION" => TokenType::Union,
            "ALL" => TokenType::All,
            "UNWIND" => TokenType::Unwind,
            "AS" => TokenType::As,
            "CASE" => TokenType::Case,
            "WHEN" => TokenType::When,
            "THEN" => TokenType::Then,
            "ELSE" => TokenType::Else,
            "END" => TokenType::End,
            "CALL" => TokenType::Call,
            "YIELD" => TokenType::Yield,
            "FOREACH" => TokenType::Foreach,
            "ON" => TokenType::On,
            "EXISTS" => TokenType::Exists,
            "TRUE" => TokenType::Boolean(true),
            "FALSE" => TokenType::Boolean(false),
            "NULL" => TokenType::Null,
            _ => TokenType::Identifier(value),
        };

        Ok(Token {
            token_type,
            line,
            column,
        })
    }

    fn read_parameter(&mut self, line: usize, column: usize) -> Result<Token, String> {
        let mut value = String::new();
        while let Some(&c) = self.chars.peek() {
            if c.is_alphanumeric() || c == '_' {
                value.push(self.advance().unwrap());
            } else {
                break;
            }
        }
        Ok(Token {
            token_type: TokenType::Variable(value),
            line,
            column,
        })
    }
}

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
            '+' => TokenType::Plus,
            '*' => TokenType::Asterisk,
            '/' => TokenType::Divide,
            '%' => TokenType::Modulo,
            '^' => TokenType::Power,
            '!' => {
                if let Some(&'=') = self.chars.peek() {
                    self.advance();
                    TokenType::NotEquals
                } else {
                    return Err(format!("Unexpected character: {char}"));
                }
            }
            _ => return Err(format!("Unexpected character: {char}")),
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
            self.position += 1;
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
        while let Some(&char) = self.chars.peek() {
            if char.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        self.advance(); // consume second '/'
        while let Some(&char) = self.chars.peek() {
            if char == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn skip_block_comment(&mut self) {
        self.advance(); // consume '*'
        while let Some(char) = self.advance() {
            if char == '*'
                && let Some(&'/') = self.chars.peek()
            {
                self.advance();
                break;
            }
        }
    }

    fn read_string(&mut self, quote: char, line: usize, column: usize) -> Result<Token, String> {
        let mut value = String::new();
        while let Some(&char) = self.chars.peek() {
            if char == quote {
                self.advance();
                break;
            }
            value.push(char);
            self.advance();
        }
        Ok(Token {
            token_type: TokenType::String(value),
            line,
            column,
        })
    }

    fn read_number(&mut self, first: char, line: usize, column: usize) -> Result<Token, String> {
        let mut value = String::new();
        value.push(first);
        let mut has_dot = false;
        while let Some(&char) = self.chars.peek() {
            if char.is_ascii_digit() {
                value.push(char);
                self.advance();
            } else if char == '.' && !has_dot {
                // Peek ahead to see if this is a range operator (..) or float (2.5)
                let mut chars = self.chars.clone();
                chars.next(); // consume the '.'
                if let Some(&next_char) = chars.peek()
                    && next_char == '.'
                {
                    // This is a range operator, not a float
                    break;
                }
                // It's a float
                has_dot = true;
                value.push(char);
                self.advance();
            } else {
                break;
            }
        }
        let number = value
            .parse::<f64>()
            .map_err(|_| format!("Invalid number: {value}"))?;
        Ok(Token {
            token_type: TokenType::Number(number),
            line,
            column,
        })
    }

    fn read_parameter(&mut self, line: usize, column: usize) -> Result<Token, String> {
        let mut value = String::new();
        while let Some(&char) = self.chars.peek() {
            if char.is_alphanumeric() || char == '_' {
                value.push(char);
                self.advance();
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

    fn read_identifier(
        &mut self,
        first: char,
        line: usize,
        column: usize,
    ) -> Result<Token, String> {
        let mut value = String::new();
        value.push(first);
        while let Some(&char) = self.chars.peek() {
            if char.is_alphanumeric() || char == '_' {
                value.push(char);
                self.advance();
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
}

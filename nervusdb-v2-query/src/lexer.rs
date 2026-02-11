use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, Clone, PartialEq)]
pub struct NumericLiteral {
    pub raw: String,
    pub value: f64,
}

impl NumericLiteral {
    pub fn is_integer(&self) -> bool {
        !self
            .raw
            .chars()
            .any(|ch| ch == '.' || ch == 'e' || ch == 'E')
    }
}

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
    Is,
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
    Number(NumericLiteral),
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

        // Backtick-escaped identifiers
        if char == '`' {
            return Ok(Some(
                self.read_backtick_identifier(start_line, start_column)?,
            ));
        }

        // Number literals (supports leading dot: .1)
        if char.is_ascii_digit()
            || (char == '.' && self.chars.peek().is_some_and(|c| c.is_ascii_digit()))
        {
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

        while let Some(&ch) = self.chars.peek() {
            if ch == quote {
                self.advance();
                if let Some(&next_ch) = self.chars.peek()
                    && next_ch == quote
                {
                    self.advance();
                    value.push(quote);
                    continue;
                }
                return Ok(Token {
                    token_type: TokenType::String(value),
                    line,
                    column,
                });
            }

            if ch == '\\' {
                self.advance();
                match self.chars.peek().copied() {
                    Some('u') => {
                        self.advance(); // consume 'u'
                        let mut hex = String::new();
                        for _ in 0..4 {
                            let Some(hex_char) = self.advance() else {
                                return Err(
                                    "syntax error: Invalid unicode escape in string literal"
                                        .to_string(),
                                );
                            };
                            hex.push(hex_char);
                        }
                        let code = u32::from_str_radix(&hex, 16).map_err(|_| {
                            "syntax error: Invalid unicode escape in string literal".to_string()
                        })?;
                        let Some(decoded) = char::from_u32(code) else {
                            return Err(
                                "syntax error: Invalid unicode codepoint in string literal"
                                    .to_string(),
                            );
                        };
                        value.push(decoded);
                    }
                    Some(next) => {
                        if next == '\\' {
                            value.push('\\');
                            value.push('\\');
                            value.push('\\');
                            value.push('\\');
                            self.advance();
                        } else {
                            value.push('\\');
                            value.push(next);
                            self.advance();
                        }
                    }
                    None => {
                        return Err("Unterminated string literal".to_string());
                    }
                }
                continue;
            }

            value.push(ch);
            self.advance();
        }

        Err("Unterminated string literal".to_string())
    }

    fn read_number(&mut self, first: char, line: usize, column: usize) -> Result<Token, String> {
        // Integer literals with base prefix (0x / 0o) are parsed as integer tokens
        // with decimal `raw` so downstream parser/evaluator can keep a single integer path.
        if first == '0'
            && let Some(&prefix) = self.chars.peek()
        {
            let radix = match prefix {
                'x' | 'X' => Some(16),
                'o' | 'O' => Some(8),
                _ => None,
            };
            if let Some(radix) = radix {
                self.advance(); // consume base marker
                return self.read_prefixed_integer(radix, line, column);
            }
        }

        let mut value = String::new();
        value.push(first);
        let mut has_dot = first == '.';

        while let Some(&ch) = self.chars.peek() {
            if ch.is_ascii_digit() {
                value.push(ch);
                self.advance();
            } else if ch == '.' && !has_dot {
                let mut chars = self.chars.clone();
                chars.next();
                if let Some(&next_char) = chars.peek()
                    && next_char == '.'
                {
                    break;
                }
                has_dot = true;
                value.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        if let Some(&exp_char) = self.chars.peek()
            && (exp_char == 'e' || exp_char == 'E')
        {
            let mut probe = self.chars.clone();
            probe.next();

            let has_exponent = match probe.peek().copied() {
                Some('+') | Some('-') => {
                    probe.next();
                    probe.peek().is_some_and(|c| c.is_ascii_digit())
                }
                Some(c) => c.is_ascii_digit(),
                None => false,
            };

            if has_exponent {
                value.push(exp_char);
                self.advance();

                if let Some(&sign) = self.chars.peek()
                    && (sign == '+' || sign == '-')
                {
                    value.push(sign);
                    self.advance();
                }

                let mut has_exp_digits = false;
                while let Some(&digit) = self.chars.peek() {
                    if digit.is_ascii_digit() {
                        has_exp_digits = true;
                        value.push(digit);
                        self.advance();
                    } else {
                        break;
                    }
                }

                if !has_exp_digits {
                    return Err(format!("syntax error: Invalid number: {value}"));
                }
            }
        }

        let number = value
            .parse::<f64>()
            .map_err(|_| format!("syntax error: Invalid number: {value}"))?;
        if !number.is_finite() {
            return Err(format!("syntax error: Invalid number: {value}"));
        }
        Ok(Token {
            token_type: TokenType::Number(NumericLiteral {
                raw: value,
                value: number,
            }),
            line,
            column,
        })
    }

    fn read_prefixed_integer(
        &mut self,
        radix: u32,
        line: usize,
        column: usize,
    ) -> Result<Token, String> {
        let mut digits = String::new();
        while let Some(&ch) = self.chars.peek() {
            if ch.is_digit(radix) {
                digits.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        if digits.is_empty() {
            return Err("syntax error: InvalidNumberLiteral".to_string());
        }

        // Reject `0x1foo` / `0o7bar` style literals as invalid number literals.
        if let Some(&ch) = self.chars.peek()
            && (ch.is_ascii_alphanumeric() || ch == '_')
        {
            return Err("syntax error: InvalidNumberLiteral".to_string());
        }

        let magnitude = u128::from_str_radix(&digits, radix)
            .map_err(|_| "syntax error: IntegerOverflow".to_string())?;
        let max_signed_magnitude = i64::MAX as u128 + 1;
        if magnitude > max_signed_magnitude {
            return Err("syntax error: IntegerOverflow".to_string());
        }

        let raw = magnitude.to_string();
        let value = raw
            .parse::<f64>()
            .map_err(|_| format!("syntax error: Invalid number: {raw}"))?;
        if !value.is_finite() {
            return Err(format!("syntax error: Invalid number: {raw}"));
        }

        Ok(Token {
            token_type: TokenType::Number(NumericLiteral { raw, value }),
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

    fn read_backtick_identifier(&mut self, line: usize, column: usize) -> Result<Token, String> {
        let mut value = String::new();
        loop {
            let Some(ch) = self.advance() else {
                return Err("Unterminated escaped identifier".to_string());
            };

            if ch == '`' {
                if let Some(&'`') = self.chars.peek() {
                    self.advance();
                    value.push('`');
                    continue;
                }
                break;
            }

            value.push(ch);
        }

        Ok(Token {
            token_type: TokenType::Identifier(value),
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
            "ASC" | "ASCENDING" => TokenType::Asc,
            "DESC" | "DESCENDING" => TokenType::Desc,
            "LIMIT" => TokenType::Limit,
            "SKIP" => TokenType::Skip,
            "DISTINCT" => TokenType::Distinct,
            "AND" => TokenType::And,
            "OR" => TokenType::Or,
            "NOT" => TokenType::Not,
            "XOR" => TokenType::Xor,
            "IS" => TokenType::Is,
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

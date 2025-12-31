use crate::ast::*;
use crate::error::Error;
use crate::lexer::{Lexer, Token, TokenType};

pub struct Parser<'a> {
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> Parser<'a> {
    pub fn parse(input: &'a str) -> Result<Query, Error> {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize().map_err(Error::Other)?;
        let mut parser = TokenParser::new(tokens);
        parser.parse_query()
    }
}

struct TokenParser {
    tokens: Vec<Token>,
    position: usize,
}

impl TokenParser {
    // Pratt parser binding powers (higher = tighter binding).
    const BP_OR: u8 = 1;
    const BP_XOR: u8 = 2;
    const BP_AND: u8 = 3;
    const BP_CMP: u8 = 4;
    const BP_ADD: u8 = 5;
    const BP_MUL: u8 = 6;
    const BP_POW: u8 = 7;
    const BP_PREFIX: u8 = 8;

    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    fn parse_query(&mut self) -> Result<Query, Error> {
        let mut clauses = self.parse_single_query_clauses()?;

        while self.match_token(&TokenType::Union) {
            let all = self.match_token(&TokenType::All);
            let right_clauses = self.parse_single_query_clauses()?;
            clauses.push(Clause::Union(UnionClause {
                all,
                query: Query {
                    clauses: right_clauses,
                },
            }));
        }

        Ok(Query { clauses })
    }

    fn parse_single_query_clauses(&mut self) -> Result<Vec<Clause>, Error> {
        let mut clauses = Vec::new();
        while !self.is_at_end() && !self.check(&TokenType::Union) {
            if let Some(clause) = self.parse_clause()? {
                clauses.push(clause);
            } else {
                break;
            }
        }
        Ok(clauses)
    }

    fn parse_clause(&mut self) -> Result<Option<Clause>, Error> {
        // Ignore optional trailing semicolons.
        if self.match_token(&TokenType::Semicolon) {
            return Ok(None);
        }

        // Fail-fast on unsupported top-level clauses/keywords.
        match &self.peek().token_type {
            TokenType::Foreach => return Err(Error::NotImplemented("FOREACH")),
            _ => {}
        }

        if self.match_token(&TokenType::Optional) {
            self.consume(&TokenType::Match, "Expected MATCH after OPTIONAL")?;
            return Ok(Some(Clause::Match(self.parse_optional_match()?)));
        }
        if self.match_token(&TokenType::Match) {
            return Ok(Some(Clause::Match(self.parse_match()?)));
        }
        if self.match_token(&TokenType::Create) {
            return Ok(Some(Clause::Create(self.parse_create()?)));
        }
        if self.match_token(&TokenType::Merge) {
            return Ok(Some(Clause::Merge(self.parse_merge()?)));
        }
        if self.match_token(&TokenType::Unwind) {
            return Ok(Some(Clause::Unwind(self.parse_unwind()?)));
        }
        if self.match_token(&TokenType::Call) {
            return Ok(Some(Clause::Call(self.parse_call()?)));
        }
        if self.match_token(&TokenType::Return) {
            return Ok(Some(Clause::Return(self.parse_return()?)));
        }
        if self.match_token(&TokenType::With) {
            return Ok(Some(Clause::With(self.parse_with()?)));
        }
        if self.match_token(&TokenType::Where) {
            return Ok(Some(Clause::Where(self.parse_where()?)));
        }
        if self.match_token(&TokenType::Set) {
            return Ok(Some(Clause::Set(self.parse_set()?)));
        }
        if self.match_token(&TokenType::Remove) {
            return Ok(Some(Clause::Remove(self.parse_remove()?)));
        }
        if self.check(&TokenType::Detach) || self.check(&TokenType::Delete) {
            return Ok(Some(Clause::Delete(self.parse_delete()?)));
        }

        if !self.is_at_end() {
            return Err(Error::Other(format!("Unexpected token {:?}", self.peek())));
        }

        Ok(None)
    }

    fn parse_call(&mut self) -> Result<CallClause, Error> {
        if !self.check(&TokenType::LeftBrace) {
            return Err(Error::NotImplemented("CALL (procedure)"));
        }

        let query = self.parse_braced_subquery()?;
        Ok(CallClause { query })
    }

    fn parse_match(&mut self) -> Result<MatchClause, Error> {
        let pattern = self.parse_pattern()?;
        Ok(MatchClause {
            optional: false,
            pattern,
        })
    }

    fn parse_optional_match(&mut self) -> Result<MatchClause, Error> {
        let pattern = self.parse_pattern()?;
        Ok(MatchClause {
            optional: true,
            pattern,
        })
    }

    fn parse_create(&mut self) -> Result<CreateClause, Error> {
        let pattern = self.parse_pattern()?;
        Ok(CreateClause { pattern })
    }

    fn parse_merge(&mut self) -> Result<MergeClause, Error> {
        let pattern = self.parse_pattern()?;
        Ok(MergeClause { pattern })
    }

    fn parse_unwind(&mut self) -> Result<UnwindClause, Error> {
        let expression = self.parse_expression()?;
        self.consume(&TokenType::As, "Expected AS after UNWIND expression")?;

        let alias = if let TokenType::Identifier(name) = &self.advance().token_type {
            name.clone()
        } else {
            return Err(Error::Other(
                "Expected identifier after UNWIND AS".to_string(),
            ));
        };

        Ok(UnwindClause { expression, alias })
    }

    fn parse_return(&mut self) -> Result<ReturnClause, Error> {
        let distinct = self.match_token(&TokenType::Distinct);
        let mut items = Vec::new();

        loop {
            items.push(self.parse_return_item()?);
            if !self.match_token(&TokenType::Comma) {
                break;
            }
        }

        // Parse ORDER BY
        let order_by = if self.match_token(&TokenType::Order) {
            self.consume(&TokenType::By, "Expected BY after ORDER")?;
            Some(self.parse_order_by()?)
        } else {
            None
        };

        // Parse SKIP
        let skip = if self.match_token(&TokenType::Skip) {
            Some(self.parse_integer("SKIP")?)
        } else {
            None
        };

        // Parse LIMIT
        let limit = if self.match_token(&TokenType::Limit) {
            Some(self.parse_integer("LIMIT")?)
        } else {
            None
        };

        Ok(ReturnClause {
            distinct,
            items,
            order_by,
            limit,
            skip,
        })
    }

    fn parse_return_item(&mut self) -> Result<ReturnItem, Error> {
        let expression = self.parse_expression()?;

        // Parse alias: `AS foo` or bare identifier after expression.
        let alias = if self.match_token(&TokenType::As) || self.peek_is_identifier() {
            Some(self.parse_identifier("RETURN alias")?)
        } else {
            None
        };

        Ok(ReturnItem { expression, alias })
    }

    fn parse_with(&mut self) -> Result<WithClause, Error> {
        let distinct = self.match_token(&TokenType::Distinct);
        let mut items = Vec::new();

        loop {
            items.push(self.parse_return_item()?);
            if !self.match_token(&TokenType::Comma) {
                break;
            }
        }

        let where_clause = if self.match_token(&TokenType::Where) {
            Some(self.parse_where()?)
        } else {
            None
        };

        // Parse ORDER BY
        let order_by = if self.match_token(&TokenType::Order) {
            self.consume(&TokenType::By, "Expected BY after ORDER")?;
            Some(self.parse_order_by()?)
        } else {
            None
        };

        // Parse SKIP
        let skip = if self.match_token(&TokenType::Skip) {
            Some(self.parse_integer("SKIP")?)
        } else {
            None
        };

        // Parse LIMIT
        let limit = if self.match_token(&TokenType::Limit) {
            Some(self.parse_integer("LIMIT")?)
        } else {
            None
        };

        Ok(WithClause {
            distinct,
            items,
            where_clause,
            order_by,
            limit,
            skip,
        })
    }

    fn parse_where(&mut self) -> Result<WhereClause, Error> {
        let expression = self.parse_expression()?;
        Ok(WhereClause { expression })
    }

    fn parse_set(&mut self) -> Result<SetClause, Error> {
        let mut items = Vec::new();
        loop {
            items.push(self.parse_set_item()?);
            if !self.match_token(&TokenType::Comma) {
                break;
            }
        }
        Ok(SetClause { items })
    }

    fn parse_set_item(&mut self) -> Result<SetItem, Error> {
        let property = self.parse_property_access()?;
        self.consume(&TokenType::Equals, "Expected '=' in SET clause")?;
        let value = self.parse_expression()?;
        Ok(SetItem { property, value })
    }

    fn parse_remove(&mut self) -> Result<RemoveClause, Error> {
        let mut properties = Vec::new();
        loop {
            properties.push(self.parse_property_access()?);
            if !self.match_token(&TokenType::Comma) {
                break;
            }
        }
        Ok(RemoveClause { properties })
    }

    fn parse_delete(&mut self) -> Result<DeleteClause, Error> {
        let detach = self.match_token(&TokenType::Detach);
        self.consume(&TokenType::Delete, "Expected DELETE")?;

        let mut expressions = Vec::new();
        loop {
            expressions.push(self.parse_expression()?);
            if !self.match_token(&TokenType::Comma) {
                break;
            }
        }

        Ok(DeleteClause {
            detach,
            expressions,
        })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, Error> {
        let mut elements = Vec::new();
        elements.push(PathElement::Node(self.parse_node_pattern()?));

        while self.check_relationship_start() {
            elements.push(PathElement::Relationship(
                self.parse_relationship_pattern()?,
            ));
            elements.push(PathElement::Node(self.parse_node_pattern()?));
        }
        Ok(Pattern { elements })
    }

    fn check_relationship_start(&self) -> bool {
        matches!(
            self.peek().token_type,
            TokenType::LeftArrow | TokenType::Dash
        )
    }

    fn parse_node_pattern(&mut self) -> Result<NodePattern, Error> {
        self.consume(&TokenType::LeftParen, "Expected '('")?;
        let variable = if self.peek_is_identifier() {
            Some(self.parse_identifier("node variable")?)
        } else {
            None
        };

        let mut labels = Vec::new();
        while self.match_token(&TokenType::Colon) {
            match &self.peek().token_type {
                TokenType::Identifier(label) => {
                    labels.push(label.clone());
                    self.advance();
                }
                TokenType::Number(n) => {
                    let n = *n;
                    self.advance();
                    if n.fract() != 0.0 || n < 0.0 {
                        return Err(Error::Other(
                            "Label id must be a non-negative integer".into(),
                        ));
                    }
                    labels.push(format!("{}", n as u64));
                }
                _ => return Err(Error::Other("Expected label identifier".to_string())),
            }
        }

        let properties = if self.check(&TokenType::LeftBrace) {
            Some(self.parse_property_map()?)
        } else {
            None
        };

        self.consume(&TokenType::RightParen, "Expected ')'")?;
        Ok(NodePattern {
            variable,
            labels,
            properties,
        })
    }

    fn parse_relationship_pattern(&mut self) -> Result<RelationshipPattern, Error> {
        let mut direction = if self.match_token(&TokenType::LeftArrow) {
            RelationshipDirection::RightToLeft
        } else if self.match_token(&TokenType::Dash) {
            RelationshipDirection::Undirected
        } else {
            return Err(Error::Other("Expected relationship start".to_string()));
        };

        let mut variable = None;
        let mut types = Vec::new();
        let mut properties = None;
        let mut variable_length = None;

        if self.match_token(&TokenType::LeftBracket) {
            if let TokenType::Identifier(name) = &self.peek().token_type {
                variable = Some(name.clone());
                self.advance();
            }

            while self.match_token(&TokenType::Colon) {
                match &self.peek().token_type {
                    TokenType::Identifier(t) => {
                        types.push(t.clone());
                        self.advance();
                    }
                    TokenType::Number(n) => {
                        let n = *n;
                        self.advance();
                        if n.fract() != 0.0 || n < 0.0 {
                            return Err(Error::Other(
                                "Relationship type id must be a non-negative integer".into(),
                            ));
                        }
                        types.push(format!("{}", n as u64));
                    }
                    _ => {
                        return Err(Error::Other(
                            "Expected relationship type identifier".to_string(),
                        ));
                    }
                }
            }

            if self.match_token(&TokenType::Asterisk) {
                variable_length = Some(self.parse_variable_length()?);
            }

            if self.check(&TokenType::LeftBrace) {
                properties = Some(self.parse_property_map()?);
            }

            self.consume(&TokenType::RightBracket, "Expected ']'")?;
        }

        if self.match_token(&TokenType::RightArrow) {
            if direction == RelationshipDirection::RightToLeft {
                return Err(Error::Other(
                    "Invalid relationship direction <->".to_string(),
                ));
            }
            direction = RelationshipDirection::LeftToRight;
        } else if self.match_token(&TokenType::Dash) {
            // Keep current direction
        } else if direction == RelationshipDirection::RightToLeft {
            self.consume(&TokenType::Dash, "Expected '-'")?;
        };

        Ok(RelationshipPattern {
            variable,
            types,
            direction,
            properties,
            variable_length,
        })
    }

    fn parse_variable_length(&mut self) -> Result<VariableLength, Error> {
        let mut min = None;
        let mut max = None;
        if matches!(self.peek().token_type, TokenType::Number(_)) {
            let n = self.parse_integer("path length")?;
            min = Some(n);
            if self.match_token(&TokenType::RangeDots) {
                if matches!(self.peek().token_type, TokenType::Number(_)) {
                    max = Some(self.parse_integer("path length")?);
                }
            } else {
                max = Some(n);
            }
            return Ok(VariableLength { min, max });
        }
        if self.match_token(&TokenType::RangeDots) {
            if matches!(self.peek().token_type, TokenType::Number(_)) {
                max = Some(self.parse_integer("path length")?);
            }
            return Ok(VariableLength { min, max });
        }
        Ok(VariableLength { min, max })
    }

    fn parse_property_map(&mut self) -> Result<PropertyMap, Error> {
        self.consume(&TokenType::LeftBrace, "Expected '{'")?;
        let mut properties = Vec::new();

        while !self.check(&TokenType::RightBrace) {
            let key = self.parse_identifier("property key")?;
            self.consume(&TokenType::Colon, "Expected ':' in property map")?;
            let value = self.parse_expression()?;
            properties.push(PropertyPair { key, value });

            if !self.match_token(&TokenType::Comma) {
                break;
            }
        }

        self.consume(&TokenType::RightBrace, "Expected '}'")?;
        Ok(PropertyMap { properties })
    }

    fn parse_property_access(&mut self) -> Result<PropertyAccess, Error> {
        let variable = self.parse_identifier("property variable")?;
        self.consume(&TokenType::Dot, "Expected '.' in property access")?;
        let property = self.parse_identifier("property name")?;
        Ok(PropertyAccess { variable, property })
    }

    fn parse_order_by(&mut self) -> Result<OrderByClause, Error> {
        let mut items = Vec::new();
        loop {
            let expression = self.parse_expression()?;
            let direction = if self.match_token(&TokenType::Asc) {
                Direction::Ascending
            } else if self.match_token(&TokenType::Desc) {
                Direction::Descending
            } else {
                Direction::Ascending
            };

            items.push(OrderByItem {
                expression,
                direction,
            });

            if !self.match_token(&TokenType::Comma) {
                break;
            }
        }

        Ok(OrderByClause { items })
    }

    fn parse_integer(&mut self, ctx: &'static str) -> Result<u32, Error> {
        match &self.advance().token_type {
            TokenType::Number(n) if *n >= 0.0 => Ok(*n as u32),
            _ => Err(Error::Other(format!("Expected integer after {ctx}"))),
        }
    }

    fn parse_identifier(&mut self, ctx: &'static str) -> Result<String, Error> {
        match &self.advance().token_type {
            TokenType::Identifier(name) => Ok(name.clone()),
            _ => Err(Error::Other(format!("Expected identifier for {ctx}"))),
        }
    }

    fn parse_expression(&mut self) -> Result<Expression, Error> {
        self.parse_expression_bp(0)
    }

    fn parse_expression_bp(&mut self, min_bp: u8) -> Result<Expression, Error> {
        let mut lhs = self.parse_prefix_expression()?;

        loop {
            let Some((op, lbp, rbp, needs_with)) = self.peek_infix_operator() else {
                break;
            };
            if lbp < min_bp {
                break;
            }

            // Consume operator token(s)
            self.advance();
            if needs_with {
                self.consume(&TokenType::With, "Expected WITH after STARTS/ENDS")?;
            }

            let rhs = self.parse_expression_bp(rbp)?;
            lhs = Expression::Binary(Box::new(BinaryExpression {
                left: lhs,
                operator: op,
                right: rhs,
            }));
        }

        Ok(lhs)
    }

    fn parse_prefix_expression(&mut self) -> Result<Expression, Error> {
        if self.match_token(&TokenType::Not) {
            let operand = self.parse_expression_bp(Self::BP_PREFIX)?;
            return Ok(Expression::Unary(Box::new(UnaryExpression {
                operator: UnaryOperator::Not,
                operand,
            })));
        }

        // NOTE: The lexer tokenizes '-' as `Dash` (shared with pattern syntax).
        // In expression context, we interpret it as unary negation / binary subtraction.
        if self.match_token(&TokenType::Dash) {
            let operand = self.parse_expression_bp(Self::BP_PREFIX)?;
            return Ok(Expression::Unary(Box::new(UnaryExpression {
                operator: UnaryOperator::Negate,
                operand,
            })));
        }

        // Unary plus: no-op (still parses for completeness).
        if self.match_token(&TokenType::Plus) {
            return self.parse_expression_bp(Self::BP_PREFIX);
        }

        self.parse_primary_expression()
    }

    fn parse_primary_expression(&mut self) -> Result<Expression, Error> {
        match &self.peek().token_type {
            TokenType::LeftParen => {
                self.advance(); // '('
                let expr = self.parse_expression_bp(0)?;
                self.consume(&TokenType::RightParen, "Expected ')'")?;
                Ok(expr)
            }
            TokenType::Number(n) => {
                let n = *n;
                self.advance();
                Ok(Expression::Literal(Literal::Number(n)))
            }
            TokenType::String(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expression::Literal(Literal::String(s)))
            }
            TokenType::Boolean(b) => {
                let b = *b;
                self.advance();
                Ok(Expression::Literal(Literal::Boolean(b)))
            }
            TokenType::Null => {
                self.advance();
                Ok(Expression::Literal(Literal::Null))
            }
            TokenType::Variable(name) => {
                let name = name.clone();
                self.advance();
                Ok(Expression::Parameter(name))
            }
            TokenType::Identifier(name) => {
                let name = name.clone();
                self.advance();

                // Function call: foo(...)
                if self.check(&TokenType::LeftParen) {
                    self.advance(); // '('
                    let args = self.parse_function_arguments()?;
                    return Ok(Expression::FunctionCall(FunctionCall { name, args }));
                }

                // Property access: n.prop
                if self.check(&TokenType::Dot) {
                    self.advance(); // '.'
                    return Ok(Expression::PropertyAccess(PropertyAccess {
                        variable: name,
                        property: self.parse_identifier("property name")?,
                    }));
                }

                Ok(Expression::Variable(name))
            }
            TokenType::LeftBracket => {
                // List literal: [a, b, c]
                self.advance(); // '['
                Ok(Expression::List(self.parse_list()?))
            }
            TokenType::LeftBrace => {
                // Map literal: {k: v, ...}
                Ok(Expression::Map(self.parse_property_map()?))
            }
            TokenType::Asterisk => {
                // Used for COUNT(*). We encode it as a string literal "*" for the aggregate parser.
                self.advance();
                Ok(Expression::Literal(Literal::String("*".to_string())))
            }
            TokenType::Case => {
                self.advance(); // 'CASE'
                Ok(Expression::Case(Box::new(self.parse_case_expression()?)))
            }
            _ => Err(Error::NotImplemented("expression")),
        }
    }

    fn parse_case_expression(&mut self) -> Result<CaseExpression, Error> {
        // Supported form:
        //
        // CASE
        //   WHEN <cond> THEN <expr>
        //   [WHEN ... THEN ...]*
        //   [ELSE <expr>]?
        // END
        if !self.check(&TokenType::When) {
            return Err(Error::NotImplemented("simple CASE expression"));
        }

        let mut when_clauses = Vec::new();
        while self.match_token(&TokenType::When) {
            let cond = self.parse_expression_bp(0)?;
            self.consume(&TokenType::Then, "Expected THEN after CASE WHEN condition")?;
            let value = self.parse_expression_bp(0)?;
            when_clauses.push((cond, value));
        }

        let else_expression = if self.match_token(&TokenType::Else) {
            Some(self.parse_expression_bp(0)?)
        } else {
            None
        };

        self.consume(&TokenType::End, "Expected END to close CASE expression")?;

        Ok(CaseExpression {
            when_clauses,
            else_expression,
        })
    }

    fn peek_infix_operator(&mut self) -> Option<(BinaryOperator, u8, u8, bool)> {
        // returns: (op, lbp, rbp, needs_with_token)
        match &self.peek().token_type {
            TokenType::Or => Some((BinaryOperator::Or, Self::BP_OR, Self::BP_OR + 1, false)),
            TokenType::Xor => Some((BinaryOperator::Xor, Self::BP_XOR, Self::BP_XOR + 1, false)),
            TokenType::And => Some((BinaryOperator::And, Self::BP_AND, Self::BP_AND + 1, false)),

            // Comparisons / predicates
            TokenType::Equals => Some((
                BinaryOperator::Equals,
                Self::BP_CMP,
                Self::BP_CMP + 1,
                false,
            )),
            TokenType::NotEquals => Some((
                BinaryOperator::NotEquals,
                Self::BP_CMP,
                Self::BP_CMP + 1,
                false,
            )),
            TokenType::LessThan => Some((
                BinaryOperator::LessThan,
                Self::BP_CMP,
                Self::BP_CMP + 1,
                false,
            )),
            TokenType::LessEqual => Some((
                BinaryOperator::LessEqual,
                Self::BP_CMP,
                Self::BP_CMP + 1,
                false,
            )),
            TokenType::GreaterThan => Some((
                BinaryOperator::GreaterThan,
                Self::BP_CMP,
                Self::BP_CMP + 1,
                false,
            )),
            TokenType::GreaterEqual => Some((
                BinaryOperator::GreaterEqual,
                Self::BP_CMP,
                Self::BP_CMP + 1,
                false,
            )),
            TokenType::In => Some((BinaryOperator::In, Self::BP_CMP, Self::BP_CMP + 1, false)),
            TokenType::Contains => Some((
                BinaryOperator::Contains,
                Self::BP_CMP,
                Self::BP_CMP + 1,
                false,
            )),
            TokenType::Starts => {
                if self.check_next(&TokenType::With) {
                    Some((
                        BinaryOperator::StartsWith,
                        Self::BP_CMP,
                        Self::BP_CMP + 1,
                        true,
                    ))
                } else {
                    None
                }
            }
            TokenType::Ends => {
                if self.check_next(&TokenType::With) {
                    Some((
                        BinaryOperator::EndsWith,
                        Self::BP_CMP,
                        Self::BP_CMP + 1,
                        true,
                    ))
                } else {
                    None
                }
            }

            // Arithmetic
            TokenType::Plus => Some((BinaryOperator::Add, Self::BP_ADD, Self::BP_ADD + 1, false)),
            TokenType::Dash => Some((
                BinaryOperator::Subtract,
                Self::BP_ADD,
                Self::BP_ADD + 1,
                false,
            )),
            TokenType::Asterisk => Some((
                BinaryOperator::Multiply,
                Self::BP_MUL,
                Self::BP_MUL + 1,
                false,
            )),
            TokenType::Divide => Some((
                BinaryOperator::Divide,
                Self::BP_MUL,
                Self::BP_MUL + 1,
                false,
            )),
            TokenType::Modulo => Some((
                BinaryOperator::Modulo,
                Self::BP_MUL,
                Self::BP_MUL + 1,
                false,
            )),
            TokenType::Power => Some((BinaryOperator::Power, Self::BP_POW, Self::BP_POW, false)), // right-assoc
            _ => None,
        }
    }

    fn parse_braced_subquery(&mut self) -> Result<Query, Error> {
        self.consume(&TokenType::LeftBrace, "Expected '{' after CALL")?;
        let query = self.parse_query()?;
        self.consume(&TokenType::RightBrace, "Expected '}' after subquery")?;
        Ok(query)
    }

    fn parse_function_arguments(&mut self) -> Result<Vec<Expression>, Error> {
        let mut args = Vec::new();

        // Handle empty arguments (e.g., COUNT())
        if self.check(&TokenType::RightParen) {
            self.advance();
            return Ok(args);
        }

        // Parse first argument
        args.push(self.parse_expression()?);

        // Parse additional arguments
        while self.match_token(&TokenType::Comma) {
            args.push(self.parse_expression()?);
        }

        self.consume(
            &TokenType::RightParen,
            "Expected ')' after function arguments",
        )?;
        Ok(args)
    }

    fn parse_list(&mut self) -> Result<Vec<Expression>, Error> {
        let mut items = Vec::new();

        // Handle empty list: []
        if self.check(&TokenType::RightBracket) {
            self.advance();
            return Ok(items);
        }

        // Parse first item
        items.push(self.parse_expression()?);

        // Parse additional items
        while self.match_token(&TokenType::Comma) {
            items.push(self.parse_expression()?);
        }

        self.consume(&TokenType::RightBracket, "Expected ']' after list")?;
        Ok(items)
    }

    fn peek_is_identifier(&self) -> bool {
        matches!(self.peek().token_type, TokenType::Identifier(_))
    }

    fn check_next(&self, token_type: &TokenType) -> bool {
        if self.position + 1 >= self.tokens.len() {
            return false;
        }
        let next = &self.tokens[self.position + 1];
        match (token_type, &next.token_type) {
            (TokenType::Identifier(_), TokenType::Identifier(_)) => true,
            _ => std::mem::discriminant(token_type) == std::mem::discriminant(&next.token_type),
        }
    }

    fn match_token(&mut self, token_type: &TokenType) -> bool {
        if self.check(token_type) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check(&self, token_type: &TokenType) -> bool {
        match (token_type, &self.peek().token_type) {
            (TokenType::Identifier(_), TokenType::Identifier(_)) => true,
            _ => {
                std::mem::discriminant(token_type)
                    == std::mem::discriminant(&self.peek().token_type)
            }
        }
    }

    fn consume(&mut self, token_type: &TokenType, message: &str) -> Result<(), Error> {
        if self.check(token_type) {
            self.advance();
            Ok(())
        } else {
            Err(Error::Other(message.to_string()))
        }
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek().token_type, TokenType::Eof)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.position]
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.position += 1;
        }
        &self.tokens[self.position - 1]
    }
}

use crate::Error;
use crate::query::ast::*;
use crate::query::lexer::{Lexer, Token, TokenType};

pub struct Parser<'a> {
    // lexer: Lexer<'a>, // Not used in this simplified version
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
            TokenType::Remove => return Err(Error::NotImplemented("REMOVE")),
            TokenType::Unwind => return Err(Error::NotImplemented("UNWIND")),
            TokenType::Call => return Err(Error::NotImplemented("CALL")),
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
        if self.check(&TokenType::Detach) || self.check(&TokenType::Delete) {
            return Ok(Some(Clause::Delete(self.parse_delete()?)));
        }

        if !self.is_at_end() {
            return Err(Error::Other(format!("Unexpected token {:?}", self.peek())));
        }

        Ok(None)
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

    fn parse_return(&mut self) -> Result<ReturnClause, Error> {
        if self.match_token(&TokenType::Distinct) {
            return Err(Error::NotImplemented("RETURN DISTINCT"));
        }
        let distinct = false;
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

    fn parse_order_by(&mut self) -> Result<OrderByClause, Error> {
        let mut items = Vec::new();
        loop {
            let expression = self.parse_expression()?;
            let direction = if self.match_token(&TokenType::Desc) {
                Direction::Descending
            } else {
                self.match_token(&TokenType::Asc); // Optional ASC
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

    fn parse_integer(&mut self, context: &str) -> Result<u32, Error> {
        match &self.advance().token_type {
            TokenType::Number(n) => {
                if *n < 0.0 || n.fract() != 0.0 || *n > (u32::MAX as f64) {
                    return Err(Error::Other(format!(
                        "{} expects a non-negative integer",
                        context
                    )));
                }
                Ok(*n as u32)
            }
            _ => Err(Error::Other(format!("Expected integer after {}", context))),
        }
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

        // Parse optional WHERE after WITH
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

    fn parse_return_item(&mut self) -> Result<ReturnItem, Error> {
        let expression = self.parse_expression()?;
        let alias = if self.match_token(&TokenType::As) {
            if let TokenType::Identifier(name) = &self.advance().token_type {
                Some(name.clone())
            } else {
                return Err(Error::Other("Expected identifier after AS".to_string()));
            }
        } else {
            None
        };
        Ok(ReturnItem { expression, alias })
    }

    fn parse_where(&mut self) -> Result<WhereClause, Error> {
        let expression = self.parse_expression()?;
        Ok(WhereClause { expression })
    }

    fn parse_set(&mut self) -> Result<SetClause, Error> {
        let mut items = Vec::new();

        loop {
            // Parse variable name (e.g., "n")
            let var_name = if let TokenType::Identifier(name) = &self.peek().token_type {
                let name = name.clone();
                self.advance();
                name
            } else {
                return Err(Error::Other(
                    "Expected variable name in SET clause".to_string(),
                ));
            };

            // Expect dot
            if !self.match_token(&TokenType::Dot) {
                return Err(Error::Other(
                    "Expected '.' after variable in SET clause".to_string(),
                ));
            }

            // Parse property name
            let property = if let TokenType::Identifier(prop) = &self.peek().token_type {
                let prop = prop.clone();
                self.advance();
                prop
            } else {
                return Err(Error::Other(
                    "Expected property name in SET clause".to_string(),
                ));
            };

            // Expect '='
            if !self.match_token(&TokenType::Equals) {
                return Err(Error::Other("Expected '=' in SET clause".to_string()));
            }

            // Parse value expression
            let value = self.parse_expression()?;

            items.push(SetItem {
                property: PropertyAccess {
                    variable: var_name,
                    property,
                },
                value,
            });

            // Check for more items
            if !self.match_token(&TokenType::Comma) {
                break;
            }
        }

        Ok(SetClause { items })
    }

    fn parse_delete(&mut self) -> Result<DeleteClause, Error> {
        // Check for optional DETACH keyword
        let detach = self.match_token(&TokenType::Detach);

        // Expect DELETE keyword
        if !self.match_token(&TokenType::Delete) {
            return Err(Error::Other("Expected DELETE keyword".to_string()));
        }

        // Parse comma-separated list of expressions to delete
        let mut expressions = Vec::new();

        loop {
            let expr = self.parse_expression()?;
            expressions.push(expr);

            // Check for more items
            if !self.match_token(&TokenType::Comma) {
                break;
            }
        }

        if expressions.is_empty() {
            return Err(Error::Other(
                "DELETE requires at least one expression".to_string(),
            ));
        }

        Ok(DeleteClause {
            detach,
            expressions,
        })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, Error> {
        let mut elements = Vec::new();
        elements.push(PathElement::Node(self.parse_node()?));

        while self.check_relationship_start() {
            elements.push(PathElement::Relationship(self.parse_relationship()?));
            elements.push(PathElement::Node(self.parse_node()?));
        }

        Ok(Pattern { elements })
    }

    fn parse_node(&mut self) -> Result<NodePattern, Error> {
        self.consume(&TokenType::LeftParen, "Expected '('")?;

        let variable = if let TokenType::Identifier(name) = &self.peek().token_type {
            let name = name.clone();
            self.advance();
            Some(name)
        } else {
            None
        };

        let mut labels = Vec::new();
        while self.match_token(&TokenType::Colon) {
            if let TokenType::Identifier(label) = &self.advance().token_type {
                labels.push(label.clone());
            } else {
                return Err(Error::Other("Expected label identifier".to_string()));
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

    fn parse_relationship(&mut self) -> Result<RelationshipPattern, Error> {
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
                if let TokenType::Identifier(t) = &self.advance().token_type {
                    types.push(t.clone());
                } else {
                    return Err(Error::Other(
                        "Expected relationship type identifier".to_string(),
                    ));
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
        } else {
            // If we started with <-, we expect -
            if direction == RelationshipDirection::RightToLeft {
                self.consume(&TokenType::Dash, "Expected '-'")?;
            }
        }

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
            let key = if let TokenType::Identifier(k) = &self.advance().token_type {
                k.clone()
            } else {
                return Err(Error::Other("Expected property key".to_string()));
            };

            self.consume(&TokenType::Colon, "Expected ':'")?;
            let value = self.parse_expression()?;
            properties.push(PropertyPair { key, value });

            if !self.match_token(&TokenType::Comma) {
                break;
            }
        }

        self.consume(&TokenType::RightBrace, "Expected '}'")?;
        Ok(PropertyMap { properties })
    }

    fn parse_expression(&mut self) -> Result<Expression, Error> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expression, Error> {
        let mut expr = self.parse_and()?;
        while self.match_token(&TokenType::Or) {
            let right = self.parse_and()?;
            expr = Expression::Binary(Box::new(BinaryExpression {
                operator: BinaryOperator::Or,
                left: expr,
                right,
            }));
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expression, Error> {
        let mut expr = self.parse_equality()?;
        while self.match_token(&TokenType::And) {
            let right = self.parse_equality()?;
            expr = Expression::Binary(Box::new(BinaryExpression {
                operator: BinaryOperator::And,
                left: expr,
                right,
            }));
        }
        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expression, Error> {
        let mut expr = self.parse_comparison()?;
        loop {
            let operator = if self.match_token(&TokenType::Equals) {
                BinaryOperator::Equal
            } else if self.match_token(&TokenType::NotEquals) {
                BinaryOperator::NotEqual
            } else {
                break;
            };
            let right = self.parse_comparison()?;
            expr = Expression::Binary(Box::new(BinaryExpression {
                operator,
                left: expr,
                right,
            }));
        }
        Ok(expr)
    }

    fn parse_comparison(&mut self) -> Result<Expression, Error> {
        let mut expr = self.parse_additive()?;
        loop {
            let operator = if self.match_token(&TokenType::LessThan) {
                BinaryOperator::LessThan
            } else if self.match_token(&TokenType::LessEqual) {
                BinaryOperator::LessThanOrEqual
            } else if self.match_token(&TokenType::GreaterThan) {
                BinaryOperator::GreaterThan
            } else if self.match_token(&TokenType::GreaterEqual) {
                BinaryOperator::GreaterThanOrEqual
            } else {
                break;
            };
            let right = self.parse_additive()?;
            expr = Expression::Binary(Box::new(BinaryExpression {
                operator,
                left: expr,
                right,
            }));
        }
        Ok(expr)
    }

    fn parse_additive(&mut self) -> Result<Expression, Error> {
        let mut expr = self.parse_multiplicative()?;
        loop {
            let operator = if self.match_token(&TokenType::Plus) {
                BinaryOperator::Add
            } else if self.match_token(&TokenType::Minus) {
                BinaryOperator::Subtract
            } else {
                break;
            };
            let right = self.parse_multiplicative()?;
            expr = Expression::Binary(Box::new(BinaryExpression {
                operator,
                left: expr,
                right,
            }));
        }
        Ok(expr)
    }

    fn parse_multiplicative(&mut self) -> Result<Expression, Error> {
        let mut expr = self.parse_unary()?;
        loop {
            let operator = if self.match_token(&TokenType::Multiply) {
                BinaryOperator::Multiply
            } else if self.match_token(&TokenType::Divide) {
                BinaryOperator::Divide
            } else if self.match_token(&TokenType::Modulo) {
                BinaryOperator::Modulo
            } else {
                break;
            };
            let right = self.parse_unary()?;
            expr = Expression::Binary(Box::new(BinaryExpression {
                operator,
                left: expr,
                right,
            }));
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expression, Error> {
        if self.match_token(&TokenType::Not) {
            let argument = self.parse_unary()?;
            return Ok(Expression::Unary(Box::new(UnaryExpression {
                operator: UnaryOperator::Not,
                argument,
            })));
        }
        if self.match_token(&TokenType::Minus) {
            let argument = self.parse_unary()?;
            return Ok(Expression::Unary(Box::new(UnaryExpression {
                operator: UnaryOperator::Negate,
                argument,
            })));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expression, Error> {
        let token = self.peek().clone();
        match &token.token_type {
            TokenType::String(s) => {
                self.advance();
                Ok(Expression::Literal(Literal::String(s.clone())))
            }
            TokenType::Number(n) => {
                self.advance();
                Ok(Expression::Literal(Literal::Float(*n)))
            }
            TokenType::Boolean(b) => {
                self.advance();
                Ok(Expression::Literal(Literal::Boolean(*b)))
            }
            TokenType::Null => {
                self.advance();
                Ok(Expression::Literal(Literal::Null))
            }
            TokenType::Identifier(name) => {
                let name = name.clone();
                self.advance();
                // Check for function call: name(...)
                if self.match_token(&TokenType::LeftParen) {
                    let mut arguments = Vec::new();
                    // Handle count(*) special case
                    if self.match_token(&TokenType::Asterisk) {
                        // count(*) - no arguments, just consume the star
                    } else if !self.check(&TokenType::RightParen) {
                        loop {
                            arguments.push(self.parse_expression()?);
                            if !self.match_token(&TokenType::Comma) {
                                break;
                            }
                        }
                    }
                    self.consume(
                        &TokenType::RightParen,
                        "Expected ')' after function arguments",
                    )?;
                    Ok(Expression::FunctionCall(FunctionCall { name, arguments }))
                }
                // Check for property access: name.prop
                else if self.match_token(&TokenType::Dot) {
                    if let TokenType::Identifier(prop) = &self.advance().token_type {
                        Ok(Expression::PropertyAccess(PropertyAccess {
                            variable: name,
                            property: prop.clone(),
                        }))
                    } else {
                        Err(Error::Other("Expected property name".to_string()))
                    }
                } else {
                    Ok(Expression::Variable(name))
                }
            }
            TokenType::Variable(name) => {
                self.advance();
                Ok(Expression::Parameter(name.clone()))
            }
            TokenType::LeftParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.consume(&TokenType::RightParen, "Expected ')'")?;
                Ok(expr)
            }
            TokenType::Case => {
                self.advance(); // consume CASE
                Ok(Expression::Case(Box::new(self.parse_case_expression()?)))
            }
            _ => Err(Error::Other(format!("Unexpected token {:?}", token))),
        }
    }

    fn parse_case_expression(&mut self) -> Result<CaseExpression, Error> {
        let mut alternatives = Vec::new();

        while self.match_token(&TokenType::When) {
            let when = self.parse_expression()?;
            self.consume(&TokenType::Then, "Expected THEN after CASE WHEN")?;
            let then = self.parse_expression()?;
            alternatives.push(CaseAlternative { when, then });
        }

        if alternatives.is_empty() {
            return Err(Error::Other("CASE requires at least one WHEN".to_string()));
        }

        let else_expression = if self.match_token(&TokenType::Else) {
            Some(self.parse_expression()?)
        } else {
            None
        };

        self.consume(&TokenType::End, "Expected END to close CASE")?;

        Ok(CaseExpression {
            alternatives,
            else_expression,
        })
    }

    // Helpers
    fn peek(&self) -> &Token {
        if self.position >= self.tokens.len() {
            &self.tokens[self.tokens.len() - 1] // EOF
        } else {
            &self.tokens[self.position]
        }
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.position += 1;
        }
        self.previous()
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.position - 1]
    }

    fn is_at_end(&self) -> bool {
        self.peek().token_type == TokenType::Eof
    }

    fn check(&self, type_: &TokenType) -> bool {
        if self.is_at_end() {
            false
        } else {
            std::mem::discriminant(&self.peek().token_type) == std::mem::discriminant(type_)
        }
    }

    fn match_token(&mut self, type_: &TokenType) -> bool {
        if self.check(type_) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn consume(&mut self, type_: &TokenType, message: &str) -> Result<&Token, Error> {
        if self.check(type_) {
            Ok(self.advance())
        } else {
            Err(Error::Other(message.to_string()))
        }
    }

    fn check_relationship_start(&self) -> bool {
        self.check(&TokenType::LeftArrow) || self.check(&TokenType::Dash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_match_return() {
        let query = "MATCH (n:Person) RETURN n";
        let result = Parser::parse(query);
        assert!(result.is_ok());
        let query = result.unwrap();
        assert_eq!(query.clauses.len(), 2);

        match &query.clauses[0] {
            Clause::Match(m) => {
                assert_eq!(m.pattern.elements.len(), 1);
                match &m.pattern.elements[0] {
                    PathElement::Node(n) => {
                        assert_eq!(n.variable, Some("n".to_string()));
                        assert_eq!(n.labels, vec!["Person".to_string()]);
                    }
                    _ => panic!("Expected Node"),
                }
            }
            _ => panic!("Expected Match Clause"),
        }

        match &query.clauses[1] {
            Clause::Return(r) => {
                assert_eq!(r.items.len(), 1);
                match &r.items[0].expression {
                    Expression::Variable(v) => assert_eq!(v, "n"),
                    _ => panic!("Expected Variable"),
                }
            }
            _ => panic!("Expected Return Clause"),
        }
    }

    #[test]
    fn test_parse_relationship() {
        let query = "MATCH (a)-[:KNOWS]->(b) RETURN a, b";
        let result = Parser::parse(query);
        assert!(result.is_ok());
        let query = result.unwrap();

        match &query.clauses[0] {
            Clause::Match(m) => {
                assert_eq!(m.pattern.elements.len(), 3); // Node, Rel, Node
                match &m.pattern.elements[1] {
                    PathElement::Relationship(r) => {
                        assert_eq!(r.types, vec!["KNOWS".to_string()]);
                        assert_eq!(r.direction, RelationshipDirection::LeftToRight);
                    }
                    _ => panic!("Expected Relationship"),
                }
            }
            _ => panic!("Expected Match Clause"),
        }
    }

    #[test]
    fn test_parse_optional_match() {
        let query = "MATCH (a) OPTIONAL MATCH (a)-[:KNOWS]->(b) RETURN a, b";
        let parsed = Parser::parse(query).unwrap();
        assert_eq!(parsed.clauses.len(), 3);

        match &parsed.clauses[0] {
            Clause::Match(m) => assert!(!m.optional),
            _ => panic!("Expected MATCH"),
        }

        match &parsed.clauses[1] {
            Clause::Match(m) => assert!(m.optional),
            _ => panic!("Expected OPTIONAL MATCH"),
        }
    }

    #[test]
    fn test_parse_merge() {
        let query = "MERGE (n:Person) RETURN n";
        let parsed = Parser::parse(query).unwrap();
        assert_eq!(parsed.clauses.len(), 2);
        assert!(matches!(&parsed.clauses[0], Clause::Merge(_)));
    }

    #[test]
    fn test_parse_variable_length_relationship() {
        let query = "MATCH (a)-[:KNOWS*1..2]->(b) RETURN b";
        let parsed = Parser::parse(query).unwrap();
        match &parsed.clauses[0] {
            Clause::Match(m) => match &m.pattern.elements[1] {
                PathElement::Relationship(r) => {
                    let len = r.variable_length.as_ref().expect("missing variable length");
                    assert_eq!(len.min, Some(1));
                    assert_eq!(len.max, Some(2));
                }
                _ => panic!("Expected relationship"),
            },
            _ => panic!("Expected MATCH"),
        }
    }

    #[test]
    fn test_parse_union() {
        let query = "MATCH (a)-[:KNOWS]->(b) RETURN b UNION MATCH (a)-[:LIKES]->(b) RETURN b";
        let parsed = Parser::parse(query).unwrap();
        assert!(matches!(parsed.clauses.last(), Some(Clause::Union(_))));
    }
}

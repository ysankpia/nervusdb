use crate::ast::*;
use crate::error::Error;
use crate::lexer::{Lexer, Token, TokenType};

#[derive(Debug, Clone, Default)]
pub(crate) struct MergeSubclauses {
    pub on_create: Vec<SetClause>,
    pub on_match: Vec<SetClause>,
}

pub struct Parser<'a> {
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> Parser<'a> {
    pub fn parse(input: &'a str) -> Result<Query, Error> {
        let (query, _merge_subclauses) = Self::parse_with_merge_subclauses(input)?;
        Ok(query)
    }

    pub(crate) fn parse_with_merge_subclauses(
        input: &'a str,
    ) -> Result<(Query, Vec<MergeSubclauses>), Error> {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize().map_err(Error::Other)?;
        let mut parser = TokenParser::new(tokens);
        let query = parser.parse_query()?;
        parser.ensure_budget()?;
        Ok((query, parser.merge_subclauses))
    }
}

struct TokenParser {
    tokens: Vec<Token>,
    position: usize,
    merge_subclauses: Vec<MergeSubclauses>,
    parse_steps: usize,
    max_parse_steps: usize,
    budget_exhausted: bool,
}

impl TokenParser {
    // Pratt parser binding powers (higher = tighter binding).
    const BP_OR: u8 = 10;
    const BP_XOR: u8 = 20;
    const BP_AND: u8 = 30;
    const BP_CMP: u8 = 40;
    const BP_PRED: u8 = 45;
    const BP_ADD: u8 = 50;
    const BP_MUL: u8 = 60;
    const BP_POW: u8 = 70;
    const BP_PREFIX: u8 = 80;
    const BP_NOT: u8 = 40;
    const PARSE_STEP_FACTOR: usize = 2_048;
    const PARSE_STEP_FLOOR: usize = 50_000;

    fn new(tokens: Vec<Token>) -> Self {
        let max_parse_steps = Self::max_parse_steps_for(tokens.len());
        Self {
            tokens,
            position: 0,
            merge_subclauses: Vec::new(),
            parse_steps: 0,
            max_parse_steps,
            budget_exhausted: false,
        }
    }

    #[cfg(test)]
    fn new_with_step_budget(tokens: Vec<Token>, max_parse_steps: usize) -> Self {
        let mut parser = Self::new(tokens);
        parser.max_parse_steps = max_parse_steps.max(1);
        parser
    }

    fn max_parse_steps_for(tokens_len: usize) -> usize {
        tokens_len
            .saturating_mul(Self::PARSE_STEP_FACTOR)
            .max(Self::PARSE_STEP_FLOOR)
    }

    fn parser_complexity_error() -> Error {
        Error::Other("syntax error: ParserComplexityLimitExceeded".to_string())
    }

    fn ensure_budget(&self) -> Result<(), Error> {
        if self.budget_exhausted {
            Err(Self::parser_complexity_error())
        } else {
            Ok(())
        }
    }

    fn parse_query(&mut self) -> Result<Query, Error> {
        self.ensure_budget()?;
        let mut clauses = self.parse_single_query_clauses()?;
        let mut union_mode: Option<bool> = None;

        while self.match_token(&TokenType::Union) {
            let all = self.match_token(&TokenType::All);
            if let Some(existing) = union_mode {
                if existing != all {
                    return Err(Error::Other(
                        "syntax error: InvalidClauseComposition".to_string(),
                    ));
                }
            } else {
                union_mode = Some(all);
            }
            let right_clauses = self.parse_single_query_clauses()?;
            clauses.push(Clause::Union(UnionClause {
                all,
                query: Query {
                    clauses: right_clauses,
                },
            }));
        }

        self.ensure_budget()?;
        Ok(Query { clauses })
    }

    fn parse_single_query_clauses(&mut self) -> Result<Vec<Clause>, Error> {
        self.ensure_budget()?;
        let mut clauses = Vec::new();
        while !self.is_at_end()
            && !self.check(&TokenType::Union)
            && !self.check(&TokenType::RightBrace)
        {
            if let Some(clause) = self.parse_clause()? {
                clauses.push(clause);
            } else {
                break;
            }
        }
        Ok(clauses)
    }

    fn parse_clause(&mut self) -> Result<Option<Clause>, Error> {
        self.ensure_budget()?;
        // Ignore optional trailing semicolons.
        if self.match_token(&TokenType::Semicolon) {
            return Ok(None);
        }

        // Fail-fast on unsupported top-level clauses/keywords.
        // if self.peek().token_type == TokenType::Foreach { ... }

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

        if self.match_token(&TokenType::Foreach) {
            return Ok(Some(Clause::Foreach(self.parse_foreach()?)));
        }

        if !self.is_at_end() {
            return Err(Error::Other(format!("Unexpected token {:?}", self.peek())));
        }

        Ok(None)
    }

    fn parse_call(&mut self) -> Result<CallClause, Error> {
        if self.check(&TokenType::LeftBrace) {
            let query = self.parse_braced_subquery()?;
            Ok(CallClause::Subquery(query))
        } else {
            let procedure = self.parse_procedure_call()?;
            Ok(CallClause::Procedure(procedure))
        }
    }

    fn parse_procedure_call(&mut self) -> Result<ProcedureCall, Error> {
        // 1. Parse name (e.g. db.info)
        let mut name = Vec::new();
        name.push(self.consume_identifier("Expected procedure name")?);
        while self.match_token(&TokenType::Dot) {
            name.push(self.consume_identifier("Expected procedure name segment after '.'")?);
        }

        // 2. Parse arguments: (arg1, arg2)
        // openCypher also allows implicit argument passing with `CALL ns.proc` (no parentheses).
        let arguments = if self.match_token(&TokenType::LeftParen) {
            self.parse_function_arguments()?
        } else {
            Vec::new()
        };

        // 3. Optional YIELD
        let mut yields = None;
        if self.match_token(&TokenType::Yield) {
            if self.match_token(&TokenType::Asterisk) {
                yields = Some(vec![YieldItem {
                    name: "*".to_string(),
                    alias: None,
                }]);
            } else {
                let mut yield_items = Vec::new();
                yield_items.push(self.parse_yield_item()?);
                while self.match_token(&TokenType::Comma) {
                    yield_items.push(self.parse_yield_item()?);
                }
                yields = Some(yield_items);
            }
        }

        Ok(ProcedureCall {
            name,
            arguments,
            yields,
        })
    }

    fn parse_yield_item(&mut self) -> Result<YieldItem, Error> {
        let name = self.consume_identifier("Expected yield column name")?;
        let mut alias = None;
        if self.match_token(&TokenType::As) {
            alias = Some(self.consume_identifier("Expected alias after AS")?);
        }
        Ok(YieldItem { name, alias })
    }

    fn consume_identifier(&mut self, message: &str) -> Result<String, Error> {
        let token = self.peek();
        if let TokenType::Identifier(id) = &token.token_type {
            let id = id.clone();
            self.advance();
            Ok(id)
        } else {
            Err(Error::Other(message.to_string()))
        }
    }

    fn parse_match(&mut self) -> Result<MatchClause, Error> {
        let mut patterns = Vec::new();
        patterns.push(self.parse_pattern()?);
        while self.match_token(&TokenType::Comma) {
            patterns.push(self.parse_pattern()?);
        }
        Ok(MatchClause {
            optional: false,
            patterns,
        })
    }

    fn parse_optional_match(&mut self) -> Result<MatchClause, Error> {
        let mut patterns = Vec::new();
        patterns.push(self.parse_pattern()?);
        while self.match_token(&TokenType::Comma) {
            patterns.push(self.parse_pattern()?);
        }
        Ok(MatchClause {
            optional: true,
            patterns,
        })
    }

    fn parse_create(&mut self) -> Result<CreateClause, Error> {
        let mut patterns = Vec::new();
        patterns.push(self.parse_pattern()?);
        while self.match_token(&TokenType::Comma) {
            patterns.push(self.parse_pattern()?);
        }
        Ok(CreateClause { patterns })
    }

    fn parse_merge(&mut self) -> Result<MergeClause, Error> {
        let pattern = self.parse_pattern()?;
        let mut subclauses = MergeSubclauses::default();

        while self.match_token(&TokenType::On) {
            if self.match_token(&TokenType::Create) {
                self.consume(&TokenType::Set, "Expected SET after ON CREATE")?;
                subclauses.on_create.push(self.parse_set()?);
                continue;
            }
            if self.match_token(&TokenType::Match) {
                self.consume(&TokenType::Set, "Expected SET after ON MATCH")?;
                subclauses.on_match.push(self.parse_set()?);
                continue;
            }
            return Err(Error::Other(
                "Expected CREATE or MATCH after ON".to_string(),
            ));
        }

        self.merge_subclauses.push(subclauses);
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
            Some(self.parse_expression()?)
        } else {
            None
        };

        // Parse LIMIT
        let limit = if self.match_token(&TokenType::Limit) {
            Some(self.parse_expression()?)
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
            Some(self.parse_expression()?)
        } else {
            None
        };

        // Parse LIMIT
        let limit = if self.match_token(&TokenType::Limit) {
            Some(self.parse_expression()?)
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
        let mut map_items = Vec::new();
        let mut labels = Vec::new();

        loop {
            let variable = self.parse_set_target_variable()?;
            if self.match_token(&TokenType::Dot) {
                let property = self.parse_identifier("property name")?;
                self.consume(&TokenType::Equals, "Expected '=' in SET clause")?;
                let value = self.parse_expression()?;
                items.push(SetItem {
                    property: PropertyAccess { variable, property },
                    value,
                });
            } else if self.check(&TokenType::Colon) {
                let label_names = self.parse_label_chain()?;
                labels.push(LabelSetItem {
                    variable,
                    labels: label_names,
                });
            } else if self.match_token(&TokenType::Equals) {
                let value = self.parse_expression()?;
                map_items.push(MapSetItem {
                    variable,
                    value,
                    append: false,
                });
            } else if self.match_token(&TokenType::Plus) {
                self.consume(&TokenType::Equals, "Expected '=' after '+' in SET clause")?;
                let value = self.parse_expression()?;
                map_items.push(MapSetItem {
                    variable,
                    value,
                    append: true,
                });
            } else {
                return Err(Error::Other(
                    "Expected '.', ':', '=' or '+=' after variable in SET clause".to_string(),
                ));
            }

            if !self.match_token(&TokenType::Comma) {
                break;
            }
        }

        Ok(SetClause {
            items,
            map_items,
            labels,
        })
    }

    fn parse_set_target_variable(&mut self) -> Result<String, Error> {
        if self.match_token(&TokenType::LeftParen) {
            let variable = self.parse_identifier("SET variable")?;
            self.consume(
                &TokenType::RightParen,
                "Expected ')' after SET variable expression",
            )?;
            Ok(variable)
        } else {
            self.parse_identifier("SET variable")
        }
    }

    fn parse_remove(&mut self) -> Result<RemoveClause, Error> {
        let mut properties = Vec::new();
        let mut labels = Vec::new();

        loop {
            let variable = self.parse_identifier("REMOVE variable")?;
            if self.match_token(&TokenType::Dot) {
                let property = self.parse_identifier("property name")?;
                properties.push(PropertyAccess { variable, property });
            } else if self.check(&TokenType::Colon) {
                let label_names = self.parse_label_chain()?;
                labels.push(LabelRemoveItem {
                    variable,
                    labels: label_names,
                });
            } else {
                return Err(Error::Other(
                    "Expected '.' or ':' after variable in REMOVE clause".to_string(),
                ));
            }

            if !self.match_token(&TokenType::Comma) {
                break;
            }
        }

        Ok(RemoveClause { properties, labels })
    }

    fn parse_label_chain(&mut self) -> Result<Vec<String>, Error> {
        let mut labels = Vec::new();
        while self.match_token(&TokenType::Colon) {
            labels.push(self.parse_identifier("label name")?);
        }
        if labels.is_empty() {
            return Err(Error::Other("Expected label after ':'".to_string()));
        }
        Ok(labels)
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
        self.ensure_budget()?;
        let variable = if self.peek_is_identifier() && self.check_next(&TokenType::Equals) {
            let var = self.parse_identifier("path variable")?;
            self.consume(&TokenType::Equals, "Expected '='")?;
            Some(var)
        } else {
            None
        };

        let mut elements = Vec::new();
        elements.push(PathElement::Node(self.parse_node_pattern()?));

        while self.check_relationship_start() {
            elements.push(PathElement::Relationship(
                self.parse_relationship_pattern()?,
            ));
            elements.push(PathElement::Node(self.parse_node_pattern()?));
        }
        Ok(Pattern { variable, elements })
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
                    let raw = n.raw.clone();
                    self.advance();
                    labels.push(parse_non_negative_u64(&raw, "Label id")?.to_string());
                }
                TokenType::End => {
                    labels.push("End".to_string());
                    self.advance();
                }
                other => {
                    if let Some(name) = Self::keyword_symbolic_name(other) {
                        labels.push(name.to_string());
                        self.advance();
                    } else {
                        return Err(Error::Other("Expected label identifier".to_string()));
                    }
                }
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
        self.ensure_budget()?;
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

            if self.match_token(&TokenType::Colon) {
                loop {
                    match &self.peek().token_type {
                        TokenType::Identifier(t) => {
                            types.push(t.clone());
                            self.advance();
                        }
                        TokenType::Number(n) => {
                            let raw = n.raw.clone();
                            self.advance();
                            types.push(
                                parse_non_negative_u64(&raw, "Relationship type id")?.to_string(),
                            );
                        }
                        other => {
                            if let Some(name) = Self::keyword_symbolic_name(other) {
                                types.push(name.to_string());
                                self.advance();
                            } else {
                                return Err(Error::Other(
                                    "Expected relationship type identifier".to_string(),
                                ));
                            }
                        }
                    }

                    if !self.match_token(&TokenType::Pipe) {
                        break;
                    }
                    // Accept both `[:A|B]` and `[:A|:B]` forms.
                    self.match_token(&TokenType::Colon);
                }

                if types.len() > 1 {
                    let mut deduped = Vec::with_capacity(types.len());
                    for ty in types {
                        if !deduped.iter().any(|existing| existing == &ty) {
                            deduped.push(ty);
                        }
                    }
                    types = deduped;
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
            direction = if direction == RelationshipDirection::RightToLeft {
                RelationshipDirection::Undirected
            } else {
                RelationshipDirection::LeftToRight
            };
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
            let key = self.parse_property_key()?;
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

    fn parse_foreach(&mut self) -> Result<ForeachClause, Error> {
        self.consume(&TokenType::LeftParen, "Expected '(' after FOREACH")?;
        let variable = self.parse_identifier("FOREACH variable")?;
        self.consume(&TokenType::In, "Expected IN after FOREACH variable")?;
        let list = self.parse_expression()?;
        self.consume(&TokenType::Pipe, "Expected '|' after FOREACH list")?;

        let mut updates = Vec::new();
        while !self.check(&TokenType::RightParen) && !self.is_at_end() {
            if let Some(clause) = self.parse_clause()? {
                match clause {
                    Clause::Create(_)
                    | Clause::Merge(_)
                    | Clause::Set(_)
                    | Clause::Delete(_)
                    | Clause::Remove(_)
                    | Clause::Foreach(_) => {
                        updates.push(clause);
                    }
                    _ => {
                        return Err(Error::Other(format!(
                            "Invalid clause inside FOREACH: {:?}",
                            clause
                        )));
                    }
                }
            } else {
                break;
            }
        }

        self.consume(&TokenType::RightParen, "Expected ')' at end of FOREACH")?;
        Ok(ForeachClause {
            variable,
            list,
            updates,
        })
    }

    fn parse_integer(&mut self, ctx: &'static str) -> Result<u32, Error> {
        match &self.advance().token_type {
            TokenType::Number(n) if n.is_integer() => n
                .raw
                .parse::<u32>()
                .map_err(|_| Error::Other(format!("Expected integer after {ctx}"))),
            _ => Err(Error::Other(format!("Expected integer after {ctx}"))),
        }
    }

    fn parse_property_key(&mut self) -> Result<String, Error> {
        match &self.advance().token_type {
            TokenType::Identifier(name) => Ok(name.clone()),
            TokenType::String(name) => Ok(name.clone()),
            TokenType::Boolean(true) => Ok("true".to_string()),
            TokenType::Boolean(false) => Ok("false".to_string()),
            TokenType::Null => Ok("null".to_string()),
            TokenType::Match => Ok("match".to_string()),
            TokenType::Create => Ok("create".to_string()),
            TokenType::Return => Ok("return".to_string()),
            TokenType::Where => Ok("where".to_string()),
            TokenType::With => Ok("with".to_string()),
            TokenType::Optional => Ok("optional".to_string()),
            TokenType::Order => Ok("order".to_string()),
            TokenType::By => Ok("by".to_string()),
            TokenType::Asc => Ok("asc".to_string()),
            TokenType::Desc => Ok("desc".to_string()),
            TokenType::Limit => Ok("limit".to_string()),
            TokenType::Skip => Ok("skip".to_string()),
            TokenType::Distinct => Ok("distinct".to_string()),
            TokenType::And => Ok("and".to_string()),
            TokenType::Or => Ok("or".to_string()),
            TokenType::Not => Ok("not".to_string()),
            TokenType::Xor => Ok("xor".to_string()),
            TokenType::Is => Ok("is".to_string()),
            TokenType::In => Ok("in".to_string()),
            TokenType::Starts => Ok("starts".to_string()),
            TokenType::Ends => Ok("ends".to_string()),
            TokenType::Contains => Ok("contains".to_string()),
            TokenType::Set => Ok("set".to_string()),
            TokenType::Delete => Ok("delete".to_string()),
            TokenType::Detach => Ok("detach".to_string()),
            TokenType::Remove => Ok("remove".to_string()),
            TokenType::Merge => Ok("merge".to_string()),
            TokenType::Union => Ok("union".to_string()),
            TokenType::All => Ok("all".to_string()),
            TokenType::Unwind => Ok("unwind".to_string()),
            TokenType::As => Ok("as".to_string()),
            TokenType::Case => Ok("case".to_string()),
            TokenType::When => Ok("when".to_string()),
            TokenType::Then => Ok("then".to_string()),
            TokenType::Else => Ok("else".to_string()),
            TokenType::End => Ok("end".to_string()),
            TokenType::Call => Ok("call".to_string()),
            TokenType::Yield => Ok("yield".to_string()),
            TokenType::Foreach => Ok("foreach".to_string()),
            TokenType::On => Ok("on".to_string()),
            TokenType::Exists => Ok("exists".to_string()),
            _ => Err(Error::Other(
                "Expected identifier for property key".to_string(),
            )),
        }
    }

    fn parse_identifier(&mut self, ctx: &'static str) -> Result<String, Error> {
        match &self.advance().token_type {
            TokenType::Identifier(name) => Ok(name.clone()),
            _ => Err(Error::Other(format!("Expected identifier for {ctx}"))),
        }
    }

    fn keyword_symbolic_name(token: &TokenType) -> Option<&'static str> {
        match token {
            TokenType::Match => Some("MATCH"),
            TokenType::Create => Some("CREATE"),
            TokenType::Return => Some("RETURN"),
            TokenType::Where => Some("WHERE"),
            TokenType::With => Some("WITH"),
            TokenType::Optional => Some("OPTIONAL"),
            TokenType::Order => Some("ORDER"),
            TokenType::By => Some("BY"),
            TokenType::Asc => Some("ASC"),
            TokenType::Desc => Some("DESC"),
            TokenType::Limit => Some("LIMIT"),
            TokenType::Skip => Some("SKIP"),
            TokenType::Distinct => Some("DISTINCT"),
            TokenType::And => Some("AND"),
            TokenType::Or => Some("OR"),
            TokenType::Not => Some("NOT"),
            TokenType::Xor => Some("XOR"),
            TokenType::Is => Some("IS"),
            TokenType::In => Some("IN"),
            TokenType::Starts => Some("STARTS"),
            TokenType::Ends => Some("ENDS"),
            TokenType::Contains => Some("CONTAINS"),
            TokenType::Set => Some("SET"),
            TokenType::Delete => Some("DELETE"),
            TokenType::Detach => Some("DETACH"),
            TokenType::Remove => Some("REMOVE"),
            TokenType::Merge => Some("MERGE"),
            TokenType::Union => Some("UNION"),
            TokenType::All => Some("ALL"),
            TokenType::Unwind => Some("UNWIND"),
            TokenType::As => Some("AS"),
            TokenType::Case => Some("CASE"),
            TokenType::When => Some("WHEN"),
            TokenType::Then => Some("THEN"),
            TokenType::Else => Some("ELSE"),
            TokenType::End => Some("END"),
            TokenType::Call => Some("CALL"),
            TokenType::Yield => Some("YIELD"),
            TokenType::Foreach => Some("FOREACH"),
            TokenType::On => Some("ON"),
            TokenType::Exists => Some("EXISTS"),
            _ => None,
        }
    }

    fn parse_expression(&mut self) -> Result<Expression, Error> {
        self.ensure_budget()?;
        self.parse_expression_bp(0)
    }

    fn parse_expression_bp(&mut self, min_bp: u8) -> Result<Expression, Error> {
        self.ensure_budget()?;
        let mut lhs = self.parse_prefix_expression()?;

        // Postfix null predicates: <expr> IS [NOT] NULL
        loop {
            if !self.match_token(&TokenType::Is) {
                break;
            }
            let op = if self.match_token(&TokenType::Not) {
                self.consume_null_keyword("Expected NULL after IS NOT")?;
                BinaryOperator::IsNotNull
            } else {
                self.consume_null_keyword("Expected NULL after IS")?;
                BinaryOperator::IsNull
            };
            lhs = Self::binary_expr(lhs, op, Expression::Literal(Literal::Null));
        }

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
            if Self::is_chainable_comparison_operator(&op) {
                // Comparison chains are equivalent to pairwise comparisons joined by AND:
                // a < b <= c     => (a < b) AND (b <= c)
                // a < b = c <> d => (a < b) AND (b = c) AND (c <> d)
                let mut combined = Self::binary_expr(lhs, op, rhs.clone());
                let mut chain_left = rhs;

                while let Some((next_op, next_lbp, next_rbp, next_needs_with)) =
                    self.peek_infix_operator()
                {
                    if next_lbp != lbp || !Self::is_chainable_comparison_operator(&next_op) {
                        break;
                    }
                    self.advance();
                    if next_needs_with {
                        self.consume(&TokenType::With, "Expected WITH after STARTS/ENDS")?;
                    }
                    let next_rhs = self.parse_expression_bp(next_rbp)?;
                    let chained_cmp = Self::binary_expr(chain_left, next_op, next_rhs.clone());
                    combined = Self::binary_expr(combined, BinaryOperator::And, chained_cmp);
                    chain_left = next_rhs;
                }

                lhs = combined;
                continue;
            }

            lhs = Self::binary_expr(lhs, op, rhs);
        }

        Ok(lhs)
    }

    fn binary_expr(left: Expression, operator: BinaryOperator, right: Expression) -> Expression {
        Expression::Binary(Box::new(BinaryExpression {
            left,
            operator,
            right,
        }))
    }

    fn is_chainable_comparison_operator(op: &BinaryOperator) -> bool {
        matches!(
            op,
            BinaryOperator::Equals
                | BinaryOperator::NotEquals
                | BinaryOperator::LessThan
                | BinaryOperator::LessEqual
                | BinaryOperator::GreaterThan
                | BinaryOperator::GreaterEqual
        )
    }

    fn parse_prefix_expression(&mut self) -> Result<Expression, Error> {
        self.ensure_budget()?;
        if self.match_token(&TokenType::Not) {
            let operand = self.parse_expression_bp(Self::BP_NOT)?;
            return Ok(Expression::Unary(Box::new(UnaryExpression {
                operator: UnaryOperator::Not,
                operand,
            })));
        }

        // NOTE: The lexer tokenizes '-' as `Dash` (shared with pattern syntax).
        // In expression context, we interpret it as unary negation / binary subtraction.
        if self.match_token(&TokenType::Dash) {
            // Support minimum i64 literal: -9223372036854775808.
            if let TokenType::Number(n) = &self.peek().token_type
                && n.is_integer()
                && n.raw == "9223372036854775808"
            {
                self.advance();
                return Ok(Expression::Literal(Literal::Integer(i64::MIN)));
            }
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
        self.ensure_budget()?;
        let mut expr = match &self.peek().token_type {
            TokenType::LeftParen => {
                if let Some(pattern) = self.try_parse_relationship_pattern_predicate() {
                    Expression::Exists(Box::new(ExistsExpression::Pattern(pattern)))
                } else {
                    self.advance(); // '('
                    let expr = self.parse_expression_bp(0)?;
                    self.consume(&TokenType::RightParen, "Expected ')'")?;
                    expr
                }
            }
            TokenType::Number(n) => {
                let number = n.clone();
                self.advance();
                if number.is_integer() {
                    let parsed = number
                        .raw
                        .parse::<i64>()
                        .map_err(|_| Error::Other("syntax error: IntegerOverflow".to_string()))?;
                    Expression::Literal(Literal::Integer(parsed))
                } else {
                    Expression::Literal(Literal::Float(number.value))
                }
            }
            TokenType::String(s) => {
                let s = s.clone();
                self.advance();
                Expression::Literal(Literal::String(s))
            }
            TokenType::Boolean(b) => {
                let b = *b;
                self.advance();
                Expression::Literal(Literal::Boolean(b))
            }
            TokenType::Null => {
                self.advance();
                Expression::Literal(Literal::Null)
            }
            TokenType::Variable(name) => {
                let name = name.clone();
                self.advance();
                Expression::Parameter(name)
            }
            TokenType::Identifier(name) => {
                let name = name.clone();
                self.advance();

                if name.eq_ignore_ascii_case("true") {
                    return Ok(Expression::Literal(Literal::Boolean(true)));
                }
                if name.eq_ignore_ascii_case("false") {
                    return Ok(Expression::Literal(Literal::Boolean(false)));
                }
                if name.eq_ignore_ascii_case("null") {
                    return Ok(Expression::Literal(Literal::Null));
                }

                let mut function_name = name.clone();
                let mut is_function = false;

                // Function call: foo(...) or namespaced function call: foo.bar(...)
                if self.check(&TokenType::LeftParen) {
                    is_function = true;
                } else if self.is_namespaced_function_call() {
                    while self.match_token(&TokenType::Dot) {
                        let segment = self.parse_identifier("function name segment after '.'")?;
                        function_name.push('.');
                        function_name.push_str(&segment);
                    }
                    is_function = true;
                }

                if is_function {
                    self.consume(&TokenType::LeftParen, "Expected '(' after function name")?;

                    // Quantifiers: any/all/none/single (x IN list WHERE pred)
                    let quant_name = function_name.to_lowercase();
                    if matches!(quant_name.as_str(), "any" | "all" | "none" | "single") {
                        let variable = self.parse_identifier("quantifier variable")?;
                        self.consume(&TokenType::In, "Expected IN in quantifier")?;
                        let list_expr = self.parse_expression()?;
                        let pred_expr = if self.match_token(&TokenType::Where) {
                            self.parse_expression()?
                        } else {
                            Expression::Literal(Literal::Boolean(true))
                        };
                        self.consume(
                            &TokenType::RightParen,
                            "Expected ')' after quantifier arguments",
                        )?;
                        return Ok(Expression::FunctionCall(FunctionCall {
                            name: format!("__quant_{quant_name}"),
                            args: vec![Expression::Variable(variable), list_expr, pred_expr],
                        }));
                    }

                    let has_distinct_arg = self.match_token(&TokenType::Distinct);
                    let mut args = self.parse_function_arguments()?;
                    if has_distinct_arg {
                        if args.len() != 1 {
                            return Err(Error::Other(
                                "DISTINCT inside function call expects exactly one argument"
                                    .to_string(),
                            ));
                        }
                        let distinct_arg = args.remove(0);
                        args = vec![Expression::FunctionCall(FunctionCall {
                            name: "__distinct".to_string(),
                            args: vec![distinct_arg],
                        })];
                    }
                    Expression::FunctionCall(FunctionCall {
                        name: function_name,
                        args,
                    })
                } else {
                    Expression::Variable(name)
                }
            }
            TokenType::All => {
                let name = "all".to_string();
                self.advance();

                if self.check(&TokenType::LeftParen) {
                    self.advance(); // '('

                    let variable = self.parse_identifier("quantifier variable")?;
                    self.consume(&TokenType::In, "Expected IN in quantifier")?;
                    let list_expr = self.parse_expression()?;
                    let pred_expr = if self.match_token(&TokenType::Where) {
                        self.parse_expression()?
                    } else {
                        Expression::Literal(Literal::Boolean(true))
                    };
                    self.consume(
                        &TokenType::RightParen,
                        "Expected ')' after quantifier arguments",
                    )?;
                    Expression::FunctionCall(FunctionCall {
                        name: format!("__quant_{name}"),
                        args: vec![Expression::Variable(variable), list_expr, pred_expr],
                    })
                } else {
                    Expression::Variable(name)
                }
            }
            TokenType::LeftBracket => {
                // List literal: [a, b, c]
                self.advance(); // '['
                self.parse_list_or_comprehension()?
            }
            TokenType::LeftBrace => {
                // Map literal: {k: v, ...}
                Expression::Map(self.parse_property_map()?)
            }
            TokenType::Asterisk => {
                // Used for COUNT(*). We encode it as a string literal "*" for the aggregate parser.
                self.advance();
                Expression::Literal(Literal::String("*".to_string()))
            }
            TokenType::Case => {
                self.advance(); // 'CASE'
                Expression::Case(Box::new(self.parse_case_expression()?))
            }
            TokenType::Exists => {
                self.advance(); // 'EXISTS'
                Expression::Exists(Box::new(self.parse_exists_expression()?))
            }
            _ => return Err(Error::NotImplemented("expression")),
        };

        // Postfix operators: property access, indexing/slicing, label predicates.
        loop {
            if self.match_token(&TokenType::Dot) {
                let property = self.parse_property_key()?;
                expr = match expr {
                    Expression::Variable(variable) => {
                        Expression::PropertyAccess(PropertyAccess { variable, property })
                    }
                    other => Expression::FunctionCall(FunctionCall {
                        name: "__getprop".to_string(),
                        args: vec![other, Expression::Literal(Literal::String(property))],
                    }),
                };
                continue;
            }

            if self.match_token(&TokenType::LeftBracket) {
                // Parse index/slice: expr[idx] / expr[start..end]
                let start_expr =
                    if self.check(&TokenType::RangeDots) || self.check(&TokenType::RightBracket) {
                        None
                    } else {
                        Some(self.parse_expression()?)
                    };

                if self.match_token(&TokenType::RangeDots) {
                    let end_expr = if self.check(&TokenType::RightBracket) {
                        None
                    } else {
                        Some(self.parse_expression()?)
                    };
                    let has_start = start_expr.is_some();
                    let has_end = end_expr.is_some();
                    self.consume(&TokenType::RightBracket, "Expected ']' after slice")?;
                    expr = Expression::FunctionCall(FunctionCall {
                        name: "__slice".to_string(),
                        args: vec![
                            expr,
                            start_expr.unwrap_or(Expression::Literal(Literal::Null)),
                            end_expr.unwrap_or(Expression::Literal(Literal::Null)),
                            Expression::Literal(Literal::Boolean(has_start)),
                            Expression::Literal(Literal::Boolean(has_end)),
                        ],
                    });
                    continue;
                }

                let index_expr = start_expr.ok_or_else(|| {
                    Error::Other("Expected index expression before ']'".to_string())
                })?;
                self.consume(&TokenType::RightBracket, "Expected ']' after index")?;
                expr = Expression::FunctionCall(FunctionCall {
                    name: "__index".to_string(),
                    args: vec![expr, index_expr],
                });
                continue;
            }

            if self.match_token(&TokenType::Colon) {
                let labels = self.parse_expression_label_chain()?;
                expr = self.build_expression_label_predicate(expr, labels);
                continue;
            }

            break;
        }

        Ok(expr)
    }

    fn consume_null_keyword(&mut self, message: &'static str) -> Result<(), Error> {
        match &self.peek().token_type {
            TokenType::Null => {
                self.advance();
                Ok(())
            }
            TokenType::Identifier(name) if name.eq_ignore_ascii_case("null") => {
                self.advance();
                Ok(())
            }
            _ => Err(Error::Other(message.to_string())),
        }
    }

    fn parse_expression_label_chain(&mut self) -> Result<Vec<String>, Error> {
        let mut labels = vec![self.parse_identifier("label identifier")?];
        while self.match_token(&TokenType::Colon) {
            labels.push(self.parse_identifier("label identifier")?);
        }
        Ok(labels)
    }

    fn build_expression_label_predicate(
        &self,
        base_expr: Expression,
        labels: Vec<String>,
    ) -> Expression {
        debug_assert!(!labels.is_empty());

        let mut labels_iter = labels.into_iter();
        let first = labels_iter
            .next()
            .expect("label chain should contain at least one label");

        let mut out = Expression::Binary(Box::new(BinaryExpression {
            left: base_expr.clone(),
            operator: BinaryOperator::HasLabel,
            right: Expression::Literal(Literal::String(first)),
        }));

        for label in labels_iter {
            let has_label = Expression::Binary(Box::new(BinaryExpression {
                left: base_expr.clone(),
                operator: BinaryOperator::HasLabel,
                right: Expression::Literal(Literal::String(label)),
            }));
            out = Expression::Binary(Box::new(BinaryExpression {
                left: out,
                operator: BinaryOperator::And,
                right: has_label,
            }));
        }

        out
    }

    fn try_parse_relationship_pattern_predicate(&mut self) -> Option<Pattern> {
        if !self.check(&TokenType::LeftParen) {
            return None;
        }

        let checkpoint = self.position;
        match self.parse_pattern() {
            Ok(pattern) if pattern.elements.len() >= 3 => Some(pattern),
            _ => {
                self.position = checkpoint;
                None
            }
        }
    }

    fn parse_exists_expression(&mut self) -> Result<ExistsExpression, Error> {
        self.ensure_budget()?;
        self.consume(&TokenType::LeftBrace, "Expected '{' after EXISTS")?;
        if self.check(&TokenType::Match)
            || self.check(&TokenType::With)
            || self.check(&TokenType::Return)
            || self.check(&TokenType::Unwind)
            || self.check(&TokenType::Call)
        {
            let subquery = self.parse_query()?;
            self.consume(&TokenType::RightBrace, "Expected '}' after EXISTS subquery")?;
            self.validate_exists_subquery_clauses(&subquery)?;
            return Ok(ExistsExpression::Subquery(subquery));
        }

        let pattern = self.parse_pattern()?;
        if self.match_token(&TokenType::Where) {
            let predicate = self.parse_expression()?;
            self.consume(&TokenType::RightBrace, "Expected '}' after EXISTS pattern")?;
            let subquery = self.build_exists_pattern_subquery(pattern, Some(predicate));
            return Ok(ExistsExpression::Subquery(subquery));
        }

        self.consume(&TokenType::RightBrace, "Expected '}' after EXISTS pattern")?;
        Ok(ExistsExpression::Pattern(pattern))
    }

    fn validate_exists_subquery_clauses(&self, subquery: &Query) -> Result<(), Error> {
        for clause in &subquery.clauses {
            if matches!(
                clause,
                Clause::Create(_)
                    | Clause::Merge(_)
                    | Clause::Set(_)
                    | Clause::Remove(_)
                    | Clause::Delete(_)
                    | Clause::Foreach(_)
            ) {
                return Err(Error::Other(
                    "syntax error: InvalidClauseComposition".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn build_exists_pattern_subquery(
        &self,
        pattern: Pattern,
        where_expr: Option<Expression>,
    ) -> Query {
        let mut clauses = vec![Clause::Match(MatchClause {
            optional: false,
            patterns: vec![pattern],
        })];
        if let Some(expression) = where_expr {
            clauses.push(Clause::Where(WhereClause { expression }));
        }
        clauses.push(Clause::Return(ReturnClause {
            distinct: false,
            items: vec![ReturnItem {
                expression: Expression::Literal(Literal::Boolean(true)),
                alias: None,
            }],
            order_by: None,
            limit: None,
            skip: None,
        }));
        Query { clauses }
    }

    fn parse_case_expression(&mut self) -> Result<CaseExpression, Error> {
        self.ensure_budget()?;
        // Supported forms:
        // 1) Searched CASE:
        //    CASE WHEN <cond> THEN <expr> ... [ELSE <expr>] END
        // 2) Simple CASE:
        //    CASE <expr> WHEN <value> THEN <expr> ... [ELSE <expr>] END
        let case_operand = if self.check(&TokenType::When) {
            None
        } else {
            Some(self.parse_expression_bp(0)?)
        };

        let mut when_clauses = Vec::new();
        while self.match_token(&TokenType::When) {
            let raw_cond = self.parse_expression_bp(0)?;
            self.consume(&TokenType::Then, "Expected THEN after CASE WHEN condition")?;
            let value = self.parse_expression_bp(0)?;
            let cond = if let Some(ref operand) = case_operand {
                Expression::Binary(Box::new(BinaryExpression {
                    left: operand.clone(),
                    operator: BinaryOperator::Equals,
                    right: raw_cond,
                }))
            } else {
                raw_cond
            };
            when_clauses.push((cond, value));
        }

        let else_expression = if self.match_token(&TokenType::Else) {
            Some(self.parse_expression_bp(0)?)
        } else {
            None
        };

        self.consume(&TokenType::End, "Expected END to close CASE expression")?;

        Ok(CaseExpression {
            expression: case_operand,
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
            TokenType::In => Some((BinaryOperator::In, Self::BP_PRED, Self::BP_PRED + 1, false)),
            TokenType::Contains => Some((
                BinaryOperator::Contains,
                Self::BP_PRED,
                Self::BP_PRED + 1,
                false,
            )),
            TokenType::Starts => {
                if self.check_next(&TokenType::With) {
                    Some((
                        BinaryOperator::StartsWith,
                        Self::BP_PRED,
                        Self::BP_PRED + 1,
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
                        Self::BP_PRED,
                        Self::BP_PRED + 1,
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
            // openCypher exponentiation is left-associative: 4 ^ 6 ^ 3 == (4 ^ 6) ^ 3.
            TokenType::Power => {
                Some((BinaryOperator::Power, Self::BP_POW, Self::BP_POW + 1, false))
            }
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

    fn parse_list_or_comprehension(&mut self) -> Result<Expression, Error> {
        self.ensure_budget()?;
        if self.check(&TokenType::RightBracket) {
            self.advance();
            return Ok(Expression::List(vec![]));
        }

        if self.peek_is_identifier() && self.check_next(&TokenType::In) {
            let variable = self.parse_identifier("list comprehension variable")?;
            self.consume(&TokenType::In, "Expected IN in list comprehension")?;

            let list_expr = self.parse_expression()?;
            let where_expression = if self.match_token(&TokenType::Where) {
                Some(self.parse_expression()?)
            } else {
                None
            };

            let map_expression = if self.match_token(&TokenType::Pipe) {
                Some(self.parse_expression()?)
            } else {
                None
            };

            self.consume(
                &TokenType::RightBracket,
                "Expected ']' after list comprehension",
            )?;
            return Ok(Expression::ListComprehension(Box::new(ListComprehension {
                variable,
                list: list_expr,
                where_expression,
                map_expression,
            })));
        }

        if self.maybe_pattern_comprehension_start() {
            let checkpoint = self.position;
            match self.parse_pattern_comprehension() {
                Ok(expr) => return Ok(expr),
                Err(_) => {
                    // Not a pattern comprehension: rewind and parse as a list literal.
                    self.position = checkpoint;
                }
            }
        }

        Ok(Expression::List(self.parse_list()?))
    }

    fn maybe_pattern_comprehension_start(&self) -> bool {
        self.check(&TokenType::LeftParen)
            || (self.peek_is_identifier() && self.check_next(&TokenType::Equals))
    }

    fn parse_pattern_comprehension(&mut self) -> Result<Expression, Error> {
        self.ensure_budget()?;
        let pattern = self.parse_pattern()?;
        let where_expression = if self.match_token(&TokenType::Where) {
            Some(self.parse_expression()?)
        } else {
            None
        };
        self.consume(&TokenType::Pipe, "Expected '|' in pattern comprehension")?;
        let projection = self.parse_expression()?;
        self.consume(
            &TokenType::RightBracket,
            "Expected ']' after pattern comprehension",
        )?;
        Ok(Expression::PatternComprehension(Box::new(
            PatternComprehension {
                pattern,
                where_expression,
                projection,
            },
        )))
    }

    fn parse_list(&mut self) -> Result<Vec<Expression>, Error> {
        self.ensure_budget()?;
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

    fn is_namespaced_function_call(&self) -> bool {
        let mut idx = self.position;
        let mut saw_segment = false;

        while idx + 1 < self.tokens.len() {
            let is_dot = matches!(self.tokens[idx].token_type, TokenType::Dot);
            let has_identifier =
                matches!(self.tokens[idx + 1].token_type, TokenType::Identifier(_));
            if !(is_dot && has_identifier) {
                break;
            }
            saw_segment = true;
            idx += 2;
        }

        saw_segment
            && idx < self.tokens.len()
            && matches!(self.tokens[idx].token_type, TokenType::LeftParen)
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
        self.parse_steps = self.parse_steps.saturating_add(1);
        if self.parse_steps > self.max_parse_steps {
            self.budget_exhausted = true;
            self.position = self.tokens.len().saturating_sub(1);
            return &self.tokens[self.position];
        }

        let current = self.position;
        if !self.is_at_end() {
            self.position += 1;
        }
        &self.tokens[current]
    }
}

fn parse_non_negative_u64(raw: &str, ctx: &str) -> Result<u64, Error> {
    if raw.chars().any(|ch| ch == '.' || ch == 'e' || ch == 'E') {
        return Err(Error::Other(format!(
            "{ctx} must be a non-negative integer"
        )));
    }
    raw.parse::<u64>()
        .map_err(|_| Error::Other(format!("{ctx} must be a non-negative integer")))
}

#[cfg(test)]
mod tests {
    use super::{Parser, TokenParser};
    use crate::ast::{Clause, Expression, Literal};
    use crate::lexer::Lexer;

    #[test]
    fn rejects_mixed_union_and_union_all() {
        let err = Parser::parse("RETURN 1 AS a UNION RETURN 2 AS a UNION ALL RETURN 3 AS a")
            .expect_err("mixed UNION / UNION ALL should fail");
        assert_eq!(err.to_string(), "syntax error: InvalidClauseComposition");
    }

    #[test]
    fn map_literal_keyword_keys_preserve_case() {
        let query = Parser::parse("RETURN {null: 'Mats', NULL: 'Pontus'} AS m")
            .expect("query should parse");
        let Clause::Return(ret) = &query.clauses[0] else {
            panic!("expected RETURN clause");
        };
        let Expression::Map(map) = &ret.items[0].expression else {
            panic!("expected map literal");
        };

        let keys: Vec<&str> = map
            .properties
            .iter()
            .map(|pair| pair.key.as_str())
            .collect();
        assert_eq!(keys, vec!["null", "NULL"]);
    }

    #[test]
    fn identifier_true_false_null_are_parsed_as_literals() {
        let query =
            Parser::parse("RETURN true AS t, FALSE AS f, null AS n").expect("query should parse");
        let Clause::Return(ret) = &query.clauses[0] else {
            panic!("expected RETURN clause");
        };

        assert!(matches!(
            ret.items[0].expression,
            Expression::Literal(Literal::Boolean(true))
        ));
        assert!(matches!(
            ret.items[1].expression,
            Expression::Literal(Literal::Boolean(false))
        ));
        assert!(matches!(
            ret.items[2].expression,
            Expression::Literal(Literal::Null)
        ));
    }

    #[test]
    fn parser_complexity_guard_trips_with_tiny_budget() {
        let mut lexer = Lexer::new("WITH x AS y RETURN y");
        let tokens = lexer.tokenize().expect("tokenize should succeed");
        let mut parser = TokenParser::new_with_step_budget(tokens, 1);
        let err = parser
            .parse_query()
            .expect_err("tiny budget should trigger complexity guard");
        assert_eq!(
            err.to_string(),
            "syntax error: ParserComplexityLimitExceeded"
        );
    }
}

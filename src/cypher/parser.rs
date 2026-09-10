use crate::cypher::ast::{
    BinaryOperator, CypherStatement, DeleteClause, Expr, NodePattern, PathPattern, RelPattern,
    ReturnItem,
};
use crate::cypher::lexer::Token;
use crate::graph::{Direction, GraphError, Value};
use std::collections::HashMap;

pub struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, cursor: 0 }
    }

    pub fn parse(&mut self) -> Result<CypherStatement, GraphError> {
        match self.peek() {
            Some(Token::Create) => {
                self.consume();
                let pattern = self.parse_path_pattern()?;
                // 可选的分号
                if self.peek() == Some(&Token::Semicolon) {
                    self.consume();
                }
                Ok(CypherStatement::Create { pattern })
            }
            Some(Token::Match) => {
                self.consume();
                let pattern = self.parse_path_pattern()?;

                let mut where_clause = None;
                if self.peek() == Some(&Token::Where) {
                    self.consume();
                    where_clause = Some(self.parse_expr()?);
                }

                let mut return_clause = None;
                let mut delete_clause = None;

                if self.peek() == Some(&Token::Detach) {
                    self.consume();
                    if self.peek() == Some(&Token::Delete) {
                        self.consume();
                        let targets = self.parse_ident_list()?;
                        delete_clause = Some(DeleteClause {
                            detach: true,
                            targets,
                        });
                    } else {
                        return Err(GraphError::General("Expected DELETE after DETACH".into()));
                    }
                } else if self.peek() == Some(&Token::Delete) {
                    self.consume();
                    let targets = self.parse_ident_list()?;
                    delete_clause = Some(DeleteClause {
                        detach: false,
                        targets,
                    });
                }

                if self.peek() == Some(&Token::Return) {
                    self.consume();
                    return_clause = Some(self.parse_return_items()?);
                }

                let mut limit = None;
                if self.peek() == Some(&Token::Limit) {
                    self.consume();
                    if let Some(Token::Literal(Value::Int(val))) = self.peek() {
                        limit = Some(*val as usize);
                        self.consume();
                    } else {
                        return Err(GraphError::General("Expected integer after LIMIT".into()));
                    }
                }

                if self.peek() == Some(&Token::Semicolon) {
                    self.consume();
                }

                Ok(CypherStatement::Match {
                    pattern,
                    where_clause,
                    return_clause,
                    delete_clause,
                    limit,
                })
            }
            _ => Err(GraphError::General(
                "Unsupported Cypher query: must start with CREATE or MATCH".into(),
            )),
        }
    }

    /// 解析路径模式：(node)-[rel]->(node)...
    fn parse_path_pattern(&mut self) -> Result<PathPattern, GraphError> {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        nodes.push(self.parse_node_pattern()?);

        while let Some(tok) = self.peek() {
            if *tok == Token::Dash || *tok == Token::ArrowLeft {
                let (edge, dst_node) = self.parse_edge_and_target_node()?;
                edges.push(edge);
                nodes.push(dst_node);
            } else {
                break;
            }
        }

        Ok(PathPattern { nodes, edges })
    }

    /// 解析单节点模式：(var:Label {k: v})
    fn parse_node_pattern(&mut self) -> Result<NodePattern, GraphError> {
        self.expect(&Token::LParen)?;

        let mut variable = None;
        let mut label = None;
        let mut properties = HashMap::new();

        if let Some(Token::Ident(name)) = self.peek() {
            variable = Some(name.clone());
            self.consume();
        }

        if self.peek() == Some(&Token::Colon) {
            self.consume();
            if let Some(Token::Ident(lbl)) = self.peek() {
                label = Some(lbl.clone());
                self.consume();
            } else {
                return Err(GraphError::General("Expected label after ':'".into()));
            }
        }

        if self.peek() == Some(&Token::LBrace) {
            properties = self.parse_properties_map()?;
        }

        self.expect(&Token::RParen)?;

        Ok(NodePattern {
            variable,
            label,
            properties,
        })
    }

    /// 解析边模式及后续的目标节点：-[rel]->(node) 或 <-[rel]-(node)
    fn parse_edge_and_target_node(&mut self) -> Result<(RelPattern, NodePattern), GraphError> {
        let is_left_arrow = self.peek() == Some(&Token::ArrowLeft);
        if is_left_arrow {
            self.consume(); // consume '<-'
        } else {
            self.expect(&Token::Dash)?; // consume '-'
        }

        self.expect(&Token::LBracket)?;

        let mut variable = None;
        let mut rel_type = None;
        let mut properties = HashMap::new();
        let mut hops = None;

        if let Some(Token::Ident(var)) = self.peek() {
            variable = Some(var.clone());
            self.consume();
        }

        if self.peek() == Some(&Token::Colon) {
            self.consume();
            if let Some(Token::Ident(t)) = self.peek() {
                rel_type = Some(t.clone());
                self.consume();
            }
        }

        // 解析多跳语法，如 *1..3 或 *2
        if self.peek() == Some(&Token::Star) {
            self.consume();
            let mut min_hops = 1;
            let mut max_hops = usize::MAX;

            if let Some(Token::Literal(Value::Int(h))) = self.peek() {
                min_hops = *h as usize;
                max_hops = min_hops;
                self.consume();
            }

            if self.peek() == Some(&Token::DotDot) {
                self.consume();
                if let Some(Token::Literal(Value::Int(h))) = self.peek() {
                    max_hops = *h as usize;
                    self.consume();
                } else {
                    max_hops = 100; // 默认上限
                }
            }
            hops = Some((min_hops, max_hops));
        }

        if self.peek() == Some(&Token::LBrace) {
            properties = self.parse_properties_map()?;
        }

        self.expect(&Token::RBracket)?;

        let direction = if is_left_arrow {
            self.expect(&Token::Dash)?;
            Direction::Incoming
        } else if self.peek() == Some(&Token::ArrowRight) {
            self.consume();
            Direction::Outgoing
        } else {
            self.expect(&Token::Dash)?;
            Direction::Both
        };

        let target_node = self.parse_node_pattern()?;

        let weight = properties.get("weight").and_then(|v| v.as_f64());

        let rel = RelPattern {
            variable,
            rel_type,
            properties,
            weight,
            hops,
            direction,
        };

        Ok((rel, target_node))
    }

    /// 解析属性字面量映射：{name: "Alice", age: 28}
    fn parse_properties_map(&mut self) -> Result<HashMap<String, Value>, GraphError> {
        self.expect(&Token::LBrace)?;
        let mut map = HashMap::new();

        while self.peek() != Some(&Token::RBrace) && self.peek().is_some() {
            let key = match self.peek() {
                Some(Token::Ident(k)) => k.clone(),
                _ => return Err(GraphError::General("Expected property key".into())),
            };
            self.consume();

            self.expect(&Token::Colon)?;

            let val = match self.peek() {
                Some(Token::Literal(v)) => v.clone(),
                _ => {
                    return Err(GraphError::General(
                        "Expected property value literal".into(),
                    ))
                }
            };
            self.consume();

            map.insert(key, val);

            if self.peek() == Some(&Token::Comma) {
                self.consume();
            } else {
                break;
            }
        }

        self.expect(&Token::RBrace)?;
        Ok(map)
    }

    /// 解析标识符列表（如 DELETE a, b）
    fn parse_ident_list(&mut self) -> Result<Vec<String>, GraphError> {
        let mut list = Vec::new();
        while let Some(Token::Ident(name)) = self.peek() {
            list.push(name.clone());
            self.consume();
            if self.peek() == Some(&Token::Comma) {
                self.consume();
            } else {
                break;
            }
        }
        Ok(list)
    }

    /// 解析 RETURN 项：RETURN a.name, b.age AS age
    fn parse_return_items(&mut self) -> Result<Vec<ReturnItem>, GraphError> {
        let mut items = Vec::new();

        while self.peek().is_some() {
            if self.peek() == Some(&Token::Star) {
                self.consume();
                items.push(ReturnItem::All);
            } else if let Some(Token::Ident(var)) = self.peek() {
                let var_name = var.clone();
                self.consume();

                if self.peek() == Some(&Token::Dot) {
                    self.consume();
                    if let Some(Token::Ident(prop)) = self.peek() {
                        let prop_name = prop.clone();
                        self.consume();

                        let mut alias = None;
                        if self.peek() == Some(&Token::As) {
                            self.consume();
                            if let Some(Token::Ident(a)) = self.peek() {
                                alias = Some(a.clone());
                                self.consume();
                            }
                        }

                        items.push(ReturnItem::Property {
                            var: var_name,
                            prop: prop_name,
                            alias,
                        });
                    } else {
                        return Err(GraphError::General(
                            "Expected property name after '.'".into(),
                        ));
                    }
                } else {
                    let mut alias = None;
                    if self.peek() == Some(&Token::As) {
                        self.consume();
                        if let Some(Token::Ident(a)) = self.peek() {
                            alias = Some(a.clone());
                            self.consume();
                        }
                    }
                    items.push(ReturnItem::Variable {
                        var: var_name,
                        alias,
                    });
                }
            } else {
                break;
            }

            if self.peek() == Some(&Token::Comma) {
                self.consume();
            } else {
                break;
            }
        }

        Ok(items)
    }

    /// 解析 WHERE 表达式
    fn parse_expr(&mut self) -> Result<Expr, GraphError> {
        self.parse_or_expr()
    }

    fn parse_or_expr(&mut self) -> Result<Expr, GraphError> {
        let mut left = self.parse_and_expr()?;
        while self.peek() == Some(&Token::Or) {
            self.consume();
            let right = self.parse_and_expr()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::Or,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and_expr(&mut self) -> Result<Expr, GraphError> {
        let mut left = self.parse_comparison_expr()?;
        while self.peek() == Some(&Token::And) {
            self.consume();
            let right = self.parse_comparison_expr()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::And,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comparison_expr(&mut self) -> Result<Expr, GraphError> {
        let left = self.parse_primary_expr()?;

        let op = match self.peek() {
            Some(Token::Eq) => Some(BinaryOperator::Eq),
            Some(Token::Neq) => Some(BinaryOperator::Neq),
            Some(Token::Lt) => Some(BinaryOperator::Lt),
            Some(Token::Lte) => Some(BinaryOperator::Lte),
            Some(Token::Gt) => Some(BinaryOperator::Gt),
            Some(Token::Gte) => Some(BinaryOperator::Gte),
            _ => None,
        };

        if let Some(operator) = op {
            self.consume();
            let right = self.parse_primary_expr()?;
            Ok(Expr::BinaryOp {
                left: Box::new(left),
                op: operator,
                right: Box::new(right),
            })
        } else {
            Ok(left)
        }
    }

    fn parse_primary_expr(&mut self) -> Result<Expr, GraphError> {
        match self.peek() {
            Some(Token::Literal(v)) => {
                let val = v.clone();
                self.consume();
                Ok(Expr::Literal(val))
            }
            Some(Token::Ident(var)) => {
                let var_name = var.clone();
                self.consume();

                if self.peek() == Some(&Token::Dot) {
                    self.consume();
                    if let Some(Token::Ident(prop)) = self.peek() {
                        let prop_name = prop.clone();
                        self.consume();
                        Ok(Expr::PropertyAccess {
                            var: var_name,
                            prop: prop_name,
                        })
                    } else {
                        Err(GraphError::General("Expected property after '.'".into()))
                    }
                } else {
                    Ok(Expr::Variable(var_name))
                }
            }
            Some(Token::LParen) => {
                self.consume();
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            _ => Err(GraphError::General("Unexpected expression token".into())),
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }

    fn consume(&mut self) -> Option<Token> {
        if self.cursor < self.tokens.len() {
            let tok = self.tokens[self.cursor].clone();
            self.cursor += 1;
            Some(tok)
        } else {
            None
        }
    }

    fn expect(&mut self, expected: &Token) -> Result<(), GraphError> {
        if self.peek() == Some(expected) {
            self.consume();
            Ok(())
        } else {
            Err(GraphError::General(format!(
                "Syntax error: expected {:?}, found {:?}",
                expected,
                self.peek()
            )))
        }
    }
}

use crate::cypher::ast::{
    AggregateArg, AggregateFunc, BinaryOperator, CypherStatement, DeleteClause, Expr, MatchClause,
    NodePattern, OrderItem, PathPattern, RelPattern, ReturnItem, SetItem,
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
        // EXPLAIN 前缀：包住整条内层语句。
        //
        // 在**递归调用 parse** 之外单独处理前缀，而不是把它塞进每个分支：
        // 这样 `EXPLAIN EXPLAIN` 这类嵌套也会被自然拒绝（内层只允许 CREATE/MATCH）。
        if self.peek() == Some(&Token::Explain) {
            self.consume();
            let inner = self.parse()?;
            return Ok(CypherStatement::Explain(Box::new(inner)));
        }

        let statement = match self.peek() {
            Some(Token::Create) => {
                self.consume();
                let pattern = self.parse_path_pattern()?;
                CypherStatement::Create { pattern }
            }
            Some(Token::Match) => {
                self.consume();

                let mut patterns = vec![self.parse_path_pattern()?];
                while self.peek() == Some(&Token::Comma) {
                    self.consume();
                    patterns.push(self.parse_path_pattern()?);
                }

                // MATCH 的模式属性必须是**字面量**。
                //
                // 这不是实现限制，而是 Cypher 语义：`MATCH (a {k: x})` 里的 `x`
                // 无法在 a 被绑定之前求值（Cypher 只允许常量与查询参数）。
                // 若放行，匹配阶段只能把求不出来的条件当作「不匹配」，而
                // 「静默不匹配」正是 §12 禁止的那类故障——调用方看到 0 行，
                // 却不知道条件根本没被求值。
                for pat in &patterns {
                    for node in &pat.nodes {
                        reject_non_literal_properties(&node.properties, "MATCH (node)")?;
                    }
                    for edge in &pat.edges {
                        reject_non_literal_properties(&edge.properties, "MATCH (relationship)")?;
                    }
                }

                let mut where_clause = None;
                if self.peek() == Some(&Token::Where) {
                    self.consume();
                    where_clause = Some(self.parse_expr()?);
                }

                let mut set_clause = Vec::new();
                if self.peek() == Some(&Token::Set) {
                    self.consume();
                    set_clause = self.parse_set_items()?;
                }

                let mut delete_clause = None;
                if self.peek() == Some(&Token::Detach) {
                    self.consume();
                    if self.peek() == Some(&Token::Delete) {
                        self.consume();
                        delete_clause = Some(DeleteClause {
                            detach: true,
                            targets: self.parse_ident_list()?,
                        });
                    } else {
                        return Err(GraphError::General("Expected DELETE after DETACH".into()));
                    }
                } else if self.peek() == Some(&Token::Delete) {
                    self.consume();
                    delete_clause = Some(DeleteClause {
                        detach: false,
                        targets: self.parse_ident_list()?,
                    });
                }

                let mut create_clause = None;
                if self.peek() == Some(&Token::Create) {
                    self.consume();
                    create_clause = Some(self.parse_path_pattern()?);
                }

                let mut return_clause = None;
                if self.peek() == Some(&Token::Return) {
                    self.consume();
                    return_clause = Some(self.parse_return_items()?);
                }

                let mut order_by = Vec::new();
                if self.peek() == Some(&Token::Order) {
                    self.consume();
                    self.expect(&Token::By)?;
                    order_by = self.parse_order_items()?;
                }

                let mut skip = None;
                if self.peek() == Some(&Token::Skip) {
                    self.consume();
                    skip = Some(self.parse_usize_literal("SKIP")?);
                }

                let mut limit = None;
                if self.peek() == Some(&Token::Limit) {
                    self.consume();
                    limit = Some(self.parse_usize_literal("LIMIT")?);
                }

                CypherStatement::Match(Box::new(MatchClause {
                    patterns,
                    where_clause,
                    set_clause,
                    delete_clause,
                    create_clause,
                    return_clause,
                    order_by,
                    skip,
                    limit,
                }))
            }
            Some(Token::Unwind) => {
                self.consume();
                let expr = self.parse_expr()?;
                self.expect(&Token::As)?;
                let variable = match self.peek() {
                    Some(Token::Ident(v)) => {
                        let name = v.clone();
                        self.consume();
                        name
                    }
                    _ => {
                        return Err(GraphError::General(
                            "UNWIND requires `AS <variable>`".into(),
                        ))
                    }
                };

                // 后续子句：可选 CREATE，可选 RETURN（含 ORDER BY / SKIP / LIMIT）
                let mut create_clause = None;
                if self.peek() == Some(&Token::Create) {
                    self.consume();
                    create_clause = Some(self.parse_path_pattern()?);
                }

                let mut return_clause = None;
                if self.peek() == Some(&Token::Return) {
                    self.consume();
                    return_clause = Some(self.parse_return_items()?);
                }

                let mut order_by = Vec::new();
                if self.peek() == Some(&Token::Order) {
                    self.consume();
                    self.expect(&Token::By)?;
                    order_by = self.parse_order_items()?;
                }
                let mut skip = None;
                if self.peek() == Some(&Token::Skip) {
                    self.consume();
                    skip = Some(self.parse_usize_literal("SKIP")?);
                }
                let mut limit = None;
                if self.peek() == Some(&Token::Limit) {
                    self.consume();
                    limit = Some(self.parse_usize_literal("LIMIT")?);
                }

                CypherStatement::Unwind {
                    expr,
                    variable,
                    return_clause,
                    order_by,
                    skip,
                    limit,
                    create_clause,
                }
            }
            _ => {
                return Err(GraphError::General(
                    "Unsupported Cypher query: must start with CREATE, MATCH or UNWIND".into(),
                ))
            }
        };

        // 尾部必须是干净的：只允许一个可选分号。
        //
        // 这道检查是**正确性防线**，不是风格洁癖。`parse_expr` 在遇到无法识别的
        // 表达式时只消费它认得的部分就返回，剩余 token 会被后续解析静默跳过。
        // 曾经的实际后果：`WHERE id(a) = 1` 里 `id` 被当作裸变量解析，`(a) = 1`
        // 整段丢弃，条件退化成恒真——不报错，但返回全部数据。
        //
        // SQLite 与 Postgres 遇到无法解析的尾部同样报错，理由相同：**返回错误
        // 数据的查询比直接失败的查询危险得多。**
        if self.peek() == Some(&Token::Semicolon) {
            self.consume();
        }
        if let Some(tok) = self.peek() {
            return Err(GraphError::General(format!(
                "Unexpected trailing input after a complete statement: {:?}. \
                 The query was not fully understood; refusing to run it rather than \
                 silently ignoring the remainder.",
                tok
            )));
        }

        Ok(statement)
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

    /// 解析单节点模式：(var:Label1:Label2 {k: v})
    fn parse_node_pattern(&mut self) -> Result<NodePattern, GraphError> {
        self.expect(&Token::LParen)?;

        let mut variable = None;
        let mut labels = Vec::new();
        let mut properties = HashMap::new();

        if let Some(Token::Ident(name)) = self.peek() {
            variable = Some(name.clone());
            self.consume();
        }

        while self.peek() == Some(&Token::Colon) {
            self.consume();
            match self.peek() {
                Some(Token::Ident(lbl)) => {
                    labels.push(lbl.clone());
                    self.consume();
                }
                _ => return Err(GraphError::General("Expected label after ':'".into())),
            }
        }

        if self.peek() == Some(&Token::LBrace) {
            properties = self.parse_properties_map()?;
        }

        self.expect(&Token::RParen)?;

        Ok(NodePattern {
            variable,
            labels,
            properties,
        })
    }

    /// 解析边模式及后续的目标节点：-[rel]->(node) 或 <-[rel]-(node) 或 -[rel]-(node)
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

        // 解析多跳语法，如 *1..3 或 *2 或 *
        if self.peek() == Some(&Token::Star) {
            self.consume();

            // `*` 单独出现时上界取 DEFAULT_MAX_HOPS，否则为精确跳数或 `*n..m` 区间
            let (min_hops, mut max_hops): (usize, usize) = match self.peek() {
                Some(Token::Literal(Value::Int(h))) => {
                    let hops = (*h).max(0) as usize;
                    self.consume();
                    (hops, hops)
                }
                _ => (1, DEFAULT_MAX_HOPS),
            };

            if self.peek() == Some(&Token::DotDot) {
                self.consume();
                if let Some(Token::Literal(Value::Int(h))) = self.peek() {
                    max_hops = (*h).max(0) as usize;
                    self.consume();
                } else {
                    max_hops = DEFAULT_MAX_HOPS;
                }
            }
            let max_hops = max_hops.max(min_hops);
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

        // `weight` 是对 `properties["weight"]` 的语法糖。只有字面量能在解析期确定，
        // 表达式形式（如来自 UNWIND 的变量）留到执行期再由 `edge_weight_from_props` 求值。
        let weight = match properties.get("weight") {
            Some(Expr::Literal(Value::Int(i))) => Some(*i as f64),
            Some(Expr::Literal(Value::Float(f))) => Some(*f),
            _ => None,
        };

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
    /// 解析 `{k: expr, ...}`。
    ///
    /// 值用**完整表达式语法**解析，而不是只认字面量：`CREATE (n {name: x})` 里的
    /// `x` 可以来自 `UNWIND ... AS x`，这是批量入库的前提。
    fn parse_properties_map(&mut self) -> Result<HashMap<String, Expr>, GraphError> {
        self.expect(&Token::LBrace)?;
        let mut map = HashMap::new();

        while self.peek() != Some(&Token::RBrace) && self.peek().is_some() {
            let key = match self.peek() {
                Some(Token::Ident(k)) => k.clone(),
                _ => return Err(GraphError::General("Expected property key".into())),
            };
            self.consume();

            self.expect(&Token::Colon)?;

            // 完整表达式：字面量、变量、属性访问、列表字面量都合法
            let val = self.parse_expr()?;
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

    /// 解析 SET 子句：`SET n.a = 1, m:Label`
    fn parse_set_items(&mut self) -> Result<Vec<SetItem>, GraphError> {
        let mut items = Vec::new();

        loop {
            let var = match self.peek() {
                Some(Token::Ident(v)) => v.clone(),
                _ => {
                    return Err(GraphError::General(
                        "Expected variable in SET clause".into(),
                    ))
                }
            };
            self.consume();

            if self.peek() == Some(&Token::Colon) {
                self.consume();
                let label = match self.peek() {
                    Some(Token::Ident(l)) => l.clone(),
                    _ => {
                        return Err(GraphError::General(
                            "Expected label after ':' in SET".into(),
                        ))
                    }
                };
                self.consume();
                items.push(SetItem::Label { var, label });
            } else if self.peek() == Some(&Token::Dot) {
                self.consume();
                let key = match self.peek() {
                    Some(Token::Ident(k)) => k.clone(),
                    _ => {
                        return Err(GraphError::General(
                            "Expected property key after '.'".into(),
                        ))
                    }
                };
                self.consume();
                self.expect(&Token::Eq)?;
                let value = self.parse_primary_expr()?;
                items.push(SetItem::Property { var, key, value });
            } else {
                return Err(GraphError::General(
                    "SET requires either 'var.key = expr' or 'var:Label'".into(),
                ));
            }

            if self.peek() == Some(&Token::Comma) {
                self.consume();
            } else {
                break;
            }
        }

        Ok(items)
    }

    /// 解析 RETURN 项：RETURN a.name, count(b) AS total, *
    fn parse_return_items(&mut self) -> Result<Vec<ReturnItem>, GraphError> {
        let mut items = Vec::new();

        while self.peek().is_some() {
            if self.peek() == Some(&Token::Star) {
                self.consume();
                items.push(ReturnItem::All);
            } else if let Some(func) = self.peek_aggregate_func() {
                self.consume();
                items.push(self.parse_aggregate_item(func)?);
            } else if let Some(Token::Ident(var)) = self.peek() {
                let var_name = var.clone();
                self.consume();

                if self.peek() == Some(&Token::Dot) {
                    self.consume();
                    if let Some(Token::Ident(prop)) = self.peek() {
                        let prop_name = prop.clone();
                        self.consume();
                        let alias = self.parse_optional_alias()?;
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
                } else if self.peek() == Some(&Token::LParen) {
                    // 标量函数调用：`RETURN id(a)`、`labels(n)`、`type(r)`
                    let param = self.parse_function_param()?;
                    if crate::cypher::ast::ScalarFunc::from_name(&var_name).is_none() {
                        return Err(GraphError::General(format!(
                            "Unknown function `{}`. Supported scalar functions are \
                             id(), labels(), type().",
                            var_name
                        )));
                    }
                    let alias = self.parse_optional_alias()?;
                    items.push(ReturnItem::Function {
                        name: var_name,
                        arg: Box::new(param),
                        alias,
                    });
                } else {
                    let alias = self.parse_optional_alias()?;
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

    fn peek_aggregate_func(&self) -> Option<AggregateFunc> {
        match self.peek() {
            Some(Token::Count) => Some(AggregateFunc::Count),
            Some(Token::Sum) => Some(AggregateFunc::Sum),
            Some(Token::Avg) => Some(AggregateFunc::Avg),
            Some(Token::Min) => Some(AggregateFunc::Min),
            Some(Token::Max) => Some(AggregateFunc::Max),
            _ => None,
        }
    }

    fn parse_aggregate_item(&mut self, func: AggregateFunc) -> Result<ReturnItem, GraphError> {
        self.expect(&Token::LParen)?;

        let arg = if self.peek() == Some(&Token::Star) {
            self.consume();
            AggregateArg::Star
        } else if let Some(Token::Ident(var)) = self.peek() {
            let var_name = var.clone();
            self.consume();
            if self.peek() == Some(&Token::Dot) {
                self.consume();
                match self.peek() {
                    Some(Token::Ident(prop)) => {
                        let prop_name = prop.clone();
                        self.consume();
                        AggregateArg::Property {
                            var: var_name,
                            prop: prop_name,
                        }
                    }
                    _ => {
                        return Err(GraphError::General(
                            "Expected property name after '.'".into(),
                        ))
                    }
                }
            } else {
                AggregateArg::Variable(var_name)
            }
        } else {
            return Err(GraphError::General(format!(
                "{}() expects '*', a variable, or a property",
                func.as_str()
            )));
        };

        self.expect(&Token::RParen)?;
        let alias = self.parse_optional_alias()?;

        Ok(ReturnItem::Aggregate { func, arg, alias })
    }

    fn parse_optional_alias(&mut self) -> Result<Option<String>, GraphError> {
        if self.peek() == Some(&Token::As) {
            self.consume();
            match self.peek() {
                Some(Token::Ident(a)) => {
                    let alias = a.clone();
                    self.consume();
                    Ok(Some(alias))
                }
                _ => Err(GraphError::General("Expected alias after AS".into())),
            }
        } else {
            Ok(None)
        }
    }

    /// 解析 ORDER BY 项：ORDER BY n.age DESC, m.name
    fn parse_order_items(&mut self) -> Result<Vec<OrderItem>, GraphError> {
        let mut items = Vec::new();

        loop {
            let expr = self.parse_expr()?;
            let desc = match self.peek() {
                Some(Token::Asc) => {
                    self.consume();
                    false
                }
                Some(Token::Desc) => {
                    self.consume();
                    true
                }
                _ => false,
            };
            items.push(OrderItem { expr, desc });

            if self.peek() == Some(&Token::Comma) {
                self.consume();
            } else {
                break;
            }
        }

        Ok(items)
    }

    fn parse_usize_literal(&mut self, keyword: &str) -> Result<usize, GraphError> {
        match self.peek() {
            Some(Token::Literal(Value::Int(val))) if *val >= 0 => {
                let result = *val as usize;
                self.consume();
                Ok(result)
            }
            _ => Err(GraphError::General(format!(
                "Expected non-negative integer after {}",
                keyword
            ))),
        }
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
            let right = self.parse_comparison_expr()?;
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
                } else if self.peek() == Some(&Token::Colon) {
                    // 类型谓词：n:Person
                    self.consume();
                    if let Some(Token::Ident(label)) = self.peek() {
                        let label_name = label.clone();
                        self.consume();
                        Ok(Expr::LabelCheck {
                            var: var_name,
                            label: label_name,
                        })
                    } else {
                        Err(GraphError::General("Expected label after ':'".into()))
                    }
                } else if self.peek() == Some(&Token::LParen) {
                    // 函数调用：`id(n)`、`labels(n)`、`type(r)`
                    //
                    // 未知函数名必须在此报错。若放行，`WHERE foo(a) = 1` 会解析成
                    // 一个裸变量并与 `= 1` 脱节，条件退化为恒真——静默返回错误数据。
                    let param = self.parse_function_param()?;
                    match crate::cypher::ast::ScalarFunc::from_name(&var_name) {
                        Some(_) => Ok(Expr::FunctionCall {
                            name: var_name,
                            args: vec![param],
                        }),
                        None => Err(GraphError::General(format!(
                            "Unknown function `{}`. Supported scalar functions are \
                             id(), labels(), type().",
                            var_name
                        ))),
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
            // 列表字面量：`[1, 2, 3]`、`['a', 'b']`、`[]`
            //
            // 空列表也要能解析：`UNWIND [] AS x` 是合法的（产生 0 行）。
            Some(Token::LBracket) => {
                self.consume();
                let mut items = Vec::new();
                if self.peek() != Some(&Token::RBracket) {
                    loop {
                        items.push(self.parse_expr()?);
                        if self.peek() == Some(&Token::Comma) {
                            self.consume();
                            continue;
                        }
                        break;
                    }
                }
                self.expect(&Token::RBracket)?;
                Ok(Expr::ListLiteral(items))
            }
            _ => Err(GraphError::General(format!(
                "Unexpected expression token: {:?}",
                self.peek()
            ))),
        }
    }

    /// 解析函数调用的参数列表：`( <expr> )`，恰好一个参数。
    ///
    /// 本项目支持的三个标量函数都是单参数；若将来引入多参数函数，把这里
    /// 改成循环并在调用处按 `ScalarFunc::arity()` 校验即可。
    fn parse_function_param(&mut self) -> Result<Expr, GraphError> {
        self.expect(&Token::LParen)?;
        let param = self.parse_expr()?;
        self.expect(&Token::RParen)?;
        Ok(param)
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

/// 无显式上限的变长匹配默认最大跳数（配合环检测守卫防爆）
const DEFAULT_MAX_HOPS: usize = 64;

/// 校验模式属性全是字面量（见 MATCH 分支里的说明）。
fn reject_non_literal_properties(
    props: &std::collections::HashMap<String, Expr>,
    context: &str,
) -> Result<(), GraphError> {
    for (key, expr) in props {
        if !matches!(expr, Expr::Literal(_)) {
            return Err(GraphError::General(format!(
                "{context} property `{key}` must be a literal: patterns are matched before \
                 any variable is bound, so an expression here can never be evaluated"
            )));
        }
    }
    Ok(())
}

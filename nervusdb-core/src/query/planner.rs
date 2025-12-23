use crate::error::Error;
use crate::query::ast::*;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub enum PhysicalPlan {
    Scan(ScanNode),
    Filter(FilterNode),
    Project(ProjectNode),
    Limit(LimitNode),
    Skip(SkipNode),
    Sort(SortNode),
    Aggregate(AggregateNode),
    NestedLoopJoin(NestedLoopJoinNode),
    LeftOuterJoin(LeftOuterJoinNode),
    Expand(ExpandNode),
    ExpandVarLength(ExpandVarLengthNode),
    Create(CreateNode),
    Set(SetNode),
    Delete(DeleteNode),
}

#[derive(Debug, Clone)]
pub struct ScanNode {
    pub alias: String,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FilterNode {
    pub input: Box<PhysicalPlan>,
    pub predicate: Expression,
}

#[derive(Debug, Clone)]
pub struct ProjectNode {
    pub input: Box<PhysicalPlan>,
    pub projections: Vec<(Expression, String)>, // (Expr, Alias)
}

#[derive(Debug, Clone)]
pub struct LimitNode {
    pub input: Box<PhysicalPlan>,
    pub limit: u32,
}

#[derive(Debug, Clone)]
pub struct SkipNode {
    pub input: Box<PhysicalPlan>,
    pub skip: u32,
}

#[derive(Debug, Clone)]
pub struct SortNode {
    pub input: Box<PhysicalPlan>,
    pub order_by: Vec<(Expression, Direction)>,
}

#[derive(Debug, Clone)]
pub struct AggregateNode {
    pub input: Box<PhysicalPlan>,
    pub aggregations: Vec<(AggregateFunction, String)>, // (function, alias)
    pub group_by: Vec<Expression>,
}

#[derive(Debug, Clone)]
pub enum AggregateFunction {
    Count(Option<Expression>), // None for count(*)
    Sum(Expression),
    Avg(Expression),
    Min(Expression),
    Max(Expression),
    Collect(Expression),
}

#[derive(Debug, Clone)]
pub struct NestedLoopJoinNode {
    pub left: Box<PhysicalPlan>,
    pub right: Box<PhysicalPlan>,
    pub predicate: Option<Expression>,
}

#[derive(Debug, Clone)]
pub struct LeftOuterJoinNode {
    pub left: Box<PhysicalPlan>,
    pub right: Box<PhysicalPlan>,
    pub right_aliases: Vec<String>, // Variables to set to NULL if no match
}

#[derive(Debug, Clone)]
pub struct ExpandNode {
    pub input: Box<PhysicalPlan>,
    pub start_node_alias: String,
    pub rel_alias: String,
    pub end_node_alias: String,
    pub direction: RelationshipDirection,
    pub rel_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExpandVarLengthNode {
    pub input: Box<PhysicalPlan>,
    pub start_node_alias: String,
    pub end_node_alias: String,
    pub direction: RelationshipDirection,
    pub rel_type: Option<String>,
    pub min_hops: u32,
    pub max_hops: u32,
}

#[derive(Debug, Clone)]
pub struct CreateNode {
    pub pattern: Pattern,
}

#[derive(Debug, Clone)]
pub struct SetNode {
    pub input: Box<PhysicalPlan>,
    pub items: Vec<SetItem>,
}

#[derive(Debug, Clone)]
pub struct DeleteNode {
    pub input: Box<PhysicalPlan>,
    pub detach: bool,
    pub expressions: Vec<Expression>,
}

pub struct QueryPlanner;

impl Default for QueryPlanner {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryPlanner {
    pub fn new() -> Self {
        Self
    }

    pub fn plan(&self, query: Query) -> Result<PhysicalPlan, Error> {
        let mut plan: Option<PhysicalPlan> = None;
        let mut where_clause: Option<WhereClause> = None;
        let mut return_clause: Option<ReturnClause> = None;

        for clause in query.clauses {
            match clause {
                Clause::Match(match_clause) => {
                    let is_optional = match_clause.optional;
                    let right_aliases: Vec<String> = if is_optional {
                        match plan.as_ref() {
                            Some(current_plan) => {
                                let left_aliases = Self::extract_plan_output_aliases(current_plan);
                                let pattern_aliases =
                                    Self::extract_pattern_aliases(&match_clause.pattern);
                                pattern_aliases.difference(&left_aliases).cloned().collect()
                            }
                            None => Vec::new(),
                        }
                    } else {
                        Vec::new()
                    };
                    let match_plan = self.plan_match(match_clause)?;

                    if let Some(current_plan) = plan {
                        if is_optional {
                            // OPTIONAL MATCH: Left outer join
                            plan = Some(PhysicalPlan::LeftOuterJoin(LeftOuterJoinNode {
                                left: Box::new(current_plan),
                                right: Box::new(match_plan),
                                right_aliases,
                            }));
                        } else {
                            // Regular MATCH: Inner join
                            plan = Some(PhysicalPlan::NestedLoopJoin(NestedLoopJoinNode {
                                left: Box::new(current_plan),
                                right: Box::new(match_plan),
                                predicate: None,
                            }));
                        }
                    } else {
                        plan = Some(match_plan);
                    }
                }
                Clause::Create(create_clause) => {
                    let create_plan = PhysicalPlan::Create(CreateNode {
                        pattern: create_clause.pattern,
                    });
                    if let Some(current_plan) = plan {
                        // CREATE after MATCH: chain them
                        plan = Some(PhysicalPlan::NestedLoopJoin(NestedLoopJoinNode {
                            left: Box::new(current_plan),
                            right: Box::new(create_plan),
                            predicate: None,
                        }));
                    } else {
                        plan = Some(create_plan);
                    }
                }
                Clause::Merge(_) => return Err(Error::NotImplemented("MERGE")),
                Clause::Set(set_clause) => {
                    // SET requires a previous plan (usually MATCH)
                    let current_plan = plan.ok_or_else(|| {
                        Error::Other(
                            "SET clause requires a preceding MATCH or CREATE clause".to_string(),
                        )
                    })?;
                    plan = Some(PhysicalPlan::Set(SetNode {
                        input: Box::new(current_plan),
                        items: set_clause.items,
                    }));
                }
                Clause::Delete(delete_clause) => {
                    // DELETE requires a previous plan (usually MATCH)
                    let current_plan = plan.ok_or_else(|| {
                        Error::Other("DELETE clause requires a preceding MATCH clause".to_string())
                    })?;
                    plan = Some(PhysicalPlan::Delete(DeleteNode {
                        input: Box::new(current_plan),
                        detach: delete_clause.detach,
                        expressions: delete_clause.expressions,
                    }));
                }
                Clause::Where(w) => {
                    where_clause = Some(w);
                }
                Clause::Return(r) => {
                    return_clause = Some(r);
                }
                Clause::With(with_clause) => {
                    // WITH acts as a pipeline: project current results, then continue
                    let current_plan = plan.ok_or_else(|| {
                        Error::Other("WITH clause requires a preceding MATCH clause".to_string())
                    })?;

                    // Apply any pending WHERE first
                    let mut with_plan = if let Some(w) = where_clause.take() {
                        PhysicalPlan::Filter(FilterNode {
                            input: Box::new(current_plan),
                            predicate: w.expression,
                        })
                    } else {
                        current_plan
                    };

                    // Project the WITH items
                    let projections: Vec<(Expression, String)> = with_clause
                        .items
                        .into_iter()
                        .map(|item| {
                            let alias = item
                                .alias
                                .unwrap_or_else(|| Self::infer_alias(&item.expression));
                            (item.expression, alias)
                        })
                        .collect();

                    with_plan = PhysicalPlan::Project(ProjectNode {
                        input: Box::new(with_plan),
                        projections,
                    });

                    // Apply WITH's WHERE clause
                    if let Some(w) = with_clause.where_clause {
                        with_plan = PhysicalPlan::Filter(FilterNode {
                            input: Box::new(with_plan),
                            predicate: w.expression,
                        });
                    }

                    // Apply ORDER BY
                    if let Some(order_by) = with_clause.order_by {
                        let order_items = order_by
                            .items
                            .into_iter()
                            .map(|item| (item.expression, item.direction))
                            .collect();
                        with_plan = PhysicalPlan::Sort(SortNode {
                            input: Box::new(with_plan),
                            order_by: order_items,
                        });
                    }

                    // Apply SKIP
                    if let Some(skip) = with_clause.skip {
                        with_plan = PhysicalPlan::Skip(SkipNode {
                            input: Box::new(with_plan),
                            skip,
                        });
                    }

                    // Apply LIMIT
                    if let Some(limit) = with_clause.limit {
                        with_plan = PhysicalPlan::Limit(LimitNode {
                            input: Box::new(with_plan),
                            limit,
                        });
                    }

                    plan = Some(with_plan);
                }
            }
        }

        let mut final_plan =
            plan.ok_or_else(|| Error::Other("No MATCH or CREATE clause found".to_string()))?;

        if let Some(w) = where_clause {
            final_plan = PhysicalPlan::Filter(FilterNode {
                input: Box::new(final_plan),
                predicate: w.expression,
            });
        }

        if let Some(r) = return_clause {
            // Check if any return item contains aggregate functions
            let has_aggregates = r
                .items
                .iter()
                .any(|item| Self::contains_aggregate(&item.expression));

            if has_aggregates {
                // Extract aggregations and non-aggregate expressions
                let mut aggregations = Vec::new();
                let mut projections = Vec::new();

                for item in r.items {
                    let alias = item
                        .alias
                        .clone()
                        .unwrap_or_else(|| Self::infer_alias(&item.expression));

                    if let Some(agg_func) = Self::extract_aggregate(&item.expression) {
                        aggregations.push((agg_func, alias));
                    } else {
                        projections.push((item.expression, alias));
                    }
                }

                // Add aggregate node
                final_plan = PhysicalPlan::Aggregate(AggregateNode {
                    input: Box::new(final_plan),
                    aggregations,
                    group_by: projections.iter().map(|(e, _)| e.clone()).collect(),
                });

                // Project the results
                if !projections.is_empty() {
                    final_plan = PhysicalPlan::Project(ProjectNode {
                        input: Box::new(final_plan),
                        projections,
                    });
                }
            } else {
                let projections = r
                    .items
                    .into_iter()
                    .map(|item| {
                        let alias = item
                            .alias
                            .unwrap_or_else(|| Self::infer_alias(&item.expression));
                        (item.expression, alias)
                    })
                    .collect();

                final_plan = PhysicalPlan::Project(ProjectNode {
                    input: Box::new(final_plan),
                    projections,
                });
            }

            // ORDER BY must come before SKIP and LIMIT
            if let Some(order_by) = r.order_by {
                let order_items = order_by
                    .items
                    .into_iter()
                    .map(|item| (item.expression, item.direction))
                    .collect();
                final_plan = PhysicalPlan::Sort(SortNode {
                    input: Box::new(final_plan),
                    order_by: order_items,
                });
            }

            // SKIP comes after ORDER BY
            if let Some(skip) = r.skip {
                final_plan = PhysicalPlan::Skip(SkipNode {
                    input: Box::new(final_plan),
                    skip,
                });
            }

            // LIMIT comes last
            if let Some(limit) = r.limit {
                final_plan = PhysicalPlan::Limit(LimitNode {
                    input: Box::new(final_plan),
                    limit,
                });
            }
        }

        Ok(final_plan)
    }

    /// Check if expression contains aggregate function
    fn contains_aggregate(expr: &Expression) -> bool {
        match expr {
            Expression::FunctionCall(fc) => {
                matches!(
                    fc.name.to_uppercase().as_str(),
                    "COUNT" | "SUM" | "AVG" | "MIN" | "MAX" | "COLLECT"
                )
            }
            _ => false,
        }
    }

    /// Extract aggregate function from expression
    fn extract_aggregate(expr: &Expression) -> Option<AggregateFunction> {
        if let Expression::FunctionCall(fc) = expr {
            match fc.name.to_uppercase().as_str() {
                "COUNT" => Some(AggregateFunction::Count(fc.arguments.first().cloned())),
                "SUM" => fc.arguments.first().cloned().map(AggregateFunction::Sum),
                "AVG" => fc.arguments.first().cloned().map(AggregateFunction::Avg),
                "MIN" => fc.arguments.first().cloned().map(AggregateFunction::Min),
                "MAX" => fc.arguments.first().cloned().map(AggregateFunction::Max),
                "COLLECT" => fc
                    .arguments
                    .first()
                    .cloned()
                    .map(AggregateFunction::Collect),
                _ => None,
            }
        } else {
            None
        }
    }

    fn plan_match(&self, match_clause: MatchClause) -> Result<PhysicalPlan, Error> {
        let mut plan: Option<PhysicalPlan> = None;
        let mut last_node_alias: Option<String> = None;
        let mut elements = match_clause.pattern.elements.into_iter();
        let mut anon_idx = 0;

        while let Some(element) = elements.next() {
            match element {
                PathElement::Node(node) => {
                    let alias = node.variable.unwrap_or_else(|| {
                        anon_idx += 1;
                        format!("_anon{}", anon_idx)
                    });

                    if let Some(current_plan) = plan {
                        // If we already have a plan, this node should have been handled by an Expand.
                        // If we are here, it means we have a disconnected node (Cartesian product).
                        // Or maybe the previous element was NOT a relationship (impossible in valid Cypher path?).
                        // Valid path: Node - Rel - Node - Rel - Node

                        // For now, assume Cartesian product if not connected via Expand.
                        let scan = PhysicalPlan::Scan(ScanNode {
                            alias: alias.clone(),
                            labels: node.labels,
                        });

                        plan = Some(PhysicalPlan::NestedLoopJoin(NestedLoopJoinNode {
                            left: Box::new(current_plan),
                            right: Box::new(scan),
                            predicate: None,
                        }));
                    } else {
                        // First node
                        plan = Some(PhysicalPlan::Scan(ScanNode {
                            alias: alias.clone(),
                            labels: node.labels,
                        }));
                    }
                    last_node_alias = Some(alias);
                }
                PathElement::Relationship(rel) => {
                    let RelationshipPattern {
                        variable,
                        types,
                        direction,
                        properties,
                        variable_length,
                    } = rel;

                    // Expect next element to be a Node
                    if let Some(PathElement::Node(next_node)) = elements.next() {
                        let start_alias = last_node_alias.ok_or_else(|| {
                            Error::Other("Relationship without start node".to_string())
                        })?;
                        let end_alias = next_node.variable.unwrap_or_else(|| {
                            anon_idx += 1;
                            format!("_anon{}", anon_idx)
                        });

                        let current_plan = plan.ok_or_else(|| {
                            Error::Other("Relationship without start node plan".to_string())
                        })?;

                        plan = if let Some(var_len) = variable_length {
                            if variable.is_some() {
                                return Err(Error::NotImplemented(
                                    "variable-length relationship variables",
                                ));
                            }
                            if properties.is_some() {
                                return Err(Error::NotImplemented(
                                    "variable-length relationship properties",
                                ));
                            }

                            let min_hops = var_len.min.unwrap_or(1);
                            let Some(max_hops) = var_len.max else {
                                return Err(Error::NotImplemented(
                                    "variable-length relationships without max",
                                ));
                            };

                            Some(PhysicalPlan::ExpandVarLength(ExpandVarLengthNode {
                                input: Box::new(current_plan),
                                start_node_alias: start_alias,
                                end_node_alias: end_alias.clone(),
                                direction,
                                rel_type: types.first().cloned(), // TODO: Handle multiple types
                                min_hops,
                                max_hops,
                            }))
                        } else {
                            let rel_alias = variable.unwrap_or_else(|| "rel".to_string());
                            Some(PhysicalPlan::Expand(ExpandNode {
                                input: Box::new(current_plan),
                                start_node_alias: start_alias,
                                rel_alias,
                                end_node_alias: end_alias.clone(),
                                direction,
                                rel_type: types.first().cloned(), // TODO: Handle multiple types
                            }))
                        };

                        last_node_alias = Some(end_alias);
                    } else {
                        return Err(Error::Other(
                            "Relationship must be followed by a Node".to_string(),
                        ));
                    }
                }
            }
        }

        plan.ok_or_else(|| Error::Other("Empty pattern".to_string()))
    }

    fn infer_alias(expr: &Expression) -> String {
        match expr {
            Expression::Variable(name) => name.clone(),
            Expression::PropertyAccess(pa) => format!("{}.{}", pa.variable, pa.property),
            _ => "col".to_string(),
        }
    }

    fn extract_pattern_aliases(pattern: &Pattern) -> HashSet<String> {
        let mut aliases = HashSet::new();
        let mut anon_idx = 0;

        for element in &pattern.elements {
            match element {
                PathElement::Node(node) => {
                    let alias = node.variable.clone().unwrap_or_else(|| {
                        anon_idx += 1;
                        format!("_anon{}", anon_idx)
                    });
                    aliases.insert(alias);
                }
                PathElement::Relationship(rel) => {
                    let alias = rel.variable.clone().unwrap_or_else(|| "rel".to_string());
                    aliases.insert(alias);
                }
            }
        }

        aliases
    }

    fn extract_plan_output_aliases(plan: &PhysicalPlan) -> HashSet<String> {
        match plan {
            PhysicalPlan::Scan(node) => HashSet::from([node.alias.clone()]),
            PhysicalPlan::Filter(node) => Self::extract_plan_output_aliases(&node.input),
            PhysicalPlan::Project(node) => node
                .projections
                .iter()
                .map(|(_, alias)| alias.clone())
                .collect(),
            PhysicalPlan::Limit(node) => Self::extract_plan_output_aliases(&node.input),
            PhysicalPlan::Skip(node) => Self::extract_plan_output_aliases(&node.input),
            PhysicalPlan::Sort(node) => Self::extract_plan_output_aliases(&node.input),
            PhysicalPlan::Aggregate(node) => {
                let mut out: HashSet<String> = node
                    .aggregations
                    .iter()
                    .map(|(_, alias)| alias.clone())
                    .collect();
                for expr in &node.group_by {
                    if let Expression::Variable(name) = expr {
                        out.insert(name.clone());
                    }
                }
                out
            }
            PhysicalPlan::NestedLoopJoin(node) => {
                let mut out = Self::extract_plan_output_aliases(&node.left);
                out.extend(Self::extract_plan_output_aliases(&node.right));
                out
            }
            PhysicalPlan::LeftOuterJoin(node) => {
                let mut out = Self::extract_plan_output_aliases(&node.left);
                out.extend(Self::extract_plan_output_aliases(&node.right));
                out
            }
            PhysicalPlan::Expand(node) => {
                let mut out = Self::extract_plan_output_aliases(&node.input);
                out.insert(node.start_node_alias.clone());
                out.insert(node.rel_alias.clone());
                out.insert(node.end_node_alias.clone());
                out
            }
            PhysicalPlan::ExpandVarLength(node) => {
                let mut out = Self::extract_plan_output_aliases(&node.input);
                out.insert(node.start_node_alias.clone());
                out.insert(node.end_node_alias.clone());
                out
            }
            PhysicalPlan::Create(_) | PhysicalPlan::Set(_) | PhysicalPlan::Delete(_) => {
                HashSet::new()
            }
        }
    }
}

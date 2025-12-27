use crate::ast::*;
use crate::error::Error;
use std::collections::HashSet;

// NOTE: This is copied from v1 as a starting point. T51 will replace/trim nodes
// to match the GraphSnapshot operator set.

#[derive(Debug, Clone)]
pub enum PhysicalPlan {
    SingleRow(SingleRowNode),
    Scan(ScanNode),
    Filter(FilterNode),
    Project(ProjectNode),
    Aggregate(AggregateNode),
    Limit(LimitNode),
    Skip(SkipNode),
    Sort(SortNode),
    Distinct(DistinctNode),
    NestedLoopJoin(NestedLoopJoinNode),
    LeftOuterJoin(LeftOuterJoinNode),
    Expand(ExpandNode),
    ExpandVariable(ExpandVariableNode),
    Unwind(UnwindNode),
    Create(CreateNode),
    Set(SetNode),
    Delete(DeleteNode),
    // v1-only nodes intentionally dropped for v2 M3 (FTS/Vector/VarLength...)
}

#[derive(Debug, Clone)]
pub struct SingleRowNode;

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
pub struct AggregateNode {
    pub input: Box<PhysicalPlan>,
    pub group_by: Vec<String>, // Variables to group by
    pub aggregates: Vec<(AggregateFunction, String)>, // (Function, Alias)
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
pub struct DistinctNode {
    pub input: Box<PhysicalPlan>,
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
pub struct ExpandVariableNode {
    pub input: Box<PhysicalPlan>,
    pub start_node_alias: String,
    pub rel_alias: String,
    pub end_node_alias: String,
    pub direction: RelationshipDirection,
    pub rel_type: Option<String>,
    pub min_hops: u32,
    pub max_hops: Option<u32>, // None = unbounded
}

#[derive(Debug, Clone)]
pub struct UnwindNode {
    pub input: Box<PhysicalPlan>,
    pub expression: Expression,
    pub alias: String,
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
                                pattern_aliases
                                    .into_iter()
                                    .filter(|a| !left_aliases.contains(a))
                                    .collect()
                            }
                            None => Vec::new(),
                        }
                    } else {
                        Vec::new()
                    };

                    let match_plan = self.plan_pattern(&match_clause.pattern)?;
                    plan = Some(if is_optional {
                        match plan {
                            Some(current) => PhysicalPlan::LeftOuterJoin(LeftOuterJoinNode {
                                left: Box::new(current),
                                right: Box::new(match_plan),
                                right_aliases,
                            }),
                            None => match_plan,
                        }
                    } else {
                        match plan {
                            Some(current) => PhysicalPlan::NestedLoopJoin(NestedLoopJoinNode {
                                left: Box::new(current),
                                right: Box::new(match_plan),
                                predicate: None,
                            }),
                            None => match_plan,
                        }
                    });
                }
                Clause::Where(w) => where_clause = Some(w),
                Clause::Return(r) => return_clause = Some(r),
                other => {
                    return Err(Error::NotImplemented(match other {
                        Clause::Create(_) => "CREATE",
                        Clause::Merge(_) => "MERGE",
                        Clause::Unwind(_) => "UNWIND",
                        Clause::Call(_) => "CALL",
                        Clause::With(_) => "WITH",
                        Clause::Set(_) => "SET",
                        Clause::Delete(_) => "DELETE",
                        Clause::Union(_) => "UNION",
                        Clause::Match(_) | Clause::Where(_) | Clause::Return(_) => unreachable!(),
                    }));
                }
            }
        }

        let mut plan = plan.unwrap_or(PhysicalPlan::SingleRow(SingleRowNode));

        if let Some(w) = where_clause {
            plan = PhysicalPlan::Filter(FilterNode {
                input: Box::new(plan),
                predicate: w.expression,
            });
        }

        if let Some(r) = return_clause {
            // Check for aggregation
            let has_aggregate = r
                .items
                .iter()
                .any(|item| Self::contains_aggregate(&item.expression));

            if has_aggregate {
                // Extract group_by and aggregates
                let mut group_by = Vec::<String>::new();
                let mut aggregates = Vec::<(AggregateFunction, String)>::new();

                for item in &r.items {
                    let alias = item
                        .alias
                        .clone()
                        .unwrap_or_else(|| Self::expr_to_alias(&item.expression));

                    if let Some((func, _)) = Self::extract_aggregate(&item.expression) {
                        aggregates.push((func, alias));
                    } else if let Expression::Variable(var) = &item.expression {
                        // Non-aggregate: treat as group_by key
                        if !group_by.contains(var) {
                            group_by.push(var.clone());
                        }
                    } else if let Expression::PropertyAccess(prop) = &item.expression {
                        // Property access without aggregate: add to group_by
                        if !group_by.contains(&prop.variable) {
                            group_by.push(prop.variable.clone());
                        }
                    }
                }

                // Create aggregate node
                plan = PhysicalPlan::Aggregate(AggregateNode {
                    input: Box::new(plan),
                    group_by,
                    aggregates,
                });
            }

            // Always add project for column aliases
            plan = PhysicalPlan::Project(ProjectNode {
                input: Box::new(plan),
                projections: r
                    .items
                    .into_iter()
                    .map(|item| {
                        let alias = item
                            .alias
                            .unwrap_or_else(|| Self::expr_to_alias(&item.expression));
                        (item.expression, alias)
                    })
                    .collect(),
            });

            if let Some(limit) = r.limit {
                plan = PhysicalPlan::Limit(LimitNode {
                    input: Box::new(plan),
                    limit,
                });
            }
        }

        Ok(plan)
    }

    fn plan_pattern(&self, pattern: &Pattern) -> Result<PhysicalPlan, Error> {
        // Handle pattern elements (nodes and relationships)
        // Pattern: (a)-[:TYPE]->(b) or (a)-[:TYPE*1..3]->(b)

        if pattern.elements.is_empty() {
            return Ok(PhysicalPlan::SingleRow(SingleRowNode));
        }

        // Process elements pairwise: Node -> Relationship -> Node -> ...
        let mut current_plan: Option<PhysicalPlan> = None;
        let mut elements = pattern.elements.iter().peekable();

        while let Some(element) = elements.next() {
            match element {
                PathElement::Node(node) => {
                    // If there's no relationship after this node, it's a standalone node scan
                    if elements.peek().is_none() {
                        let alias = node.variable.clone().unwrap_or_else(|| "_".to_string());
                        current_plan = Some(PhysicalPlan::Scan(ScanNode {
                            alias,
                            labels: node.labels.clone(),
                        }));
                    }
                    // If next is relationship, we'll handle it in Relationship case
                }
                PathElement::Relationship(rel) => {
                    // Get the next node (must exist after relationship)
                    let next_element = elements.next();
                    let end_node = match next_element {
                        Some(PathElement::Node(n)) => n,
                        _ => {
                            return Err(Error::NotImplemented(
                                "malformed pattern: relationship must be followed by node",
                            ));
                        }
                    };

                    // Extract relationship info
                    let rel_type = rel.types.first().cloned();
                    let direction = rel.direction.clone();

                    // Extract node aliases
                    // We need to find the start node alias from previous state or from this relationship's context
                    // For now, use placeholders that will be set during execution
                    let start_node_alias =
                        rel.variable.clone().unwrap_or_else(|| "_start".to_string());
                    let rel_alias = rel.variable.clone().unwrap_or_else(|| "_rel".to_string());
                    let end_node_alias = end_node
                        .variable
                        .clone()
                        .unwrap_or_else(|| "_end".to_string());

                    // Check for variable-length pattern
                    let next_plan = if let Some(var_len) = &rel.variable_length {
                        let min = var_len.min.unwrap_or(1);
                        let max = var_len.max;
                        PhysicalPlan::ExpandVariable(ExpandVariableNode {
                            input: Box::new(
                                current_plan
                                    .clone()
                                    .unwrap_or(PhysicalPlan::SingleRow(SingleRowNode)),
                            ),
                            start_node_alias: start_node_alias.clone(),
                            rel_alias: rel_alias.clone(),
                            end_node_alias: end_node_alias.clone(),
                            direction: direction.clone(),
                            rel_type,
                            min_hops: min,
                            max_hops: max,
                        })
                    } else {
                        // Single-hop expand
                        PhysicalPlan::Expand(ExpandNode {
                            input: Box::new(
                                current_plan
                                    .clone()
                                    .unwrap_or(PhysicalPlan::SingleRow(SingleRowNode)),
                            ),
                            start_node_alias: start_node_alias.clone(),
                            rel_alias: rel_alias.clone(),
                            end_node_alias: end_node_alias.clone(),
                            direction: direction.clone(),
                            rel_type,
                        })
                    };

                    current_plan = Some(next_plan);
                }
            }
        }

        // If no elements produced a plan, return single row
        Ok(current_plan.unwrap_or(PhysicalPlan::SingleRow(SingleRowNode)))
    }

    fn extract_pattern_aliases(pattern: &Pattern) -> Vec<String> {
        let mut out = Vec::new();
        for el in &pattern.elements {
            match el {
                PathElement::Node(n) => {
                    if let Some(v) = &n.variable {
                        out.push(v.clone());
                    }
                }
                PathElement::Relationship(r) => {
                    if let Some(v) = &r.variable {
                        out.push(v.clone());
                    }
                }
            }
        }
        out
    }

    fn extract_plan_output_aliases(plan: &PhysicalPlan) -> HashSet<String> {
        let mut out = HashSet::new();
        Self::collect_output_aliases(plan, &mut out);
        out
    }

    fn collect_output_aliases(plan: &PhysicalPlan, out: &mut HashSet<String>) {
        match plan {
            PhysicalPlan::SingleRow(_) => {}
            PhysicalPlan::Scan(n) => {
                out.insert(n.alias.clone());
            }
            PhysicalPlan::Filter(n) => Self::collect_output_aliases(&n.input, out),
            PhysicalPlan::Project(n) => {
                for (_, alias) in &n.projections {
                    out.insert(alias.clone());
                }
                Self::collect_output_aliases(&n.input, out);
            }
            PhysicalPlan::Aggregate(n) => {
                for (_, alias) in &n.aggregates {
                    out.insert(alias.clone());
                }
                for group_var in &n.group_by {
                    out.insert(group_var.clone());
                }
                Self::collect_output_aliases(&n.input, out);
            }
            PhysicalPlan::Limit(n) => Self::collect_output_aliases(&n.input, out),
            PhysicalPlan::Skip(n) => Self::collect_output_aliases(&n.input, out),
            PhysicalPlan::Sort(n) => Self::collect_output_aliases(&n.input, out),
            PhysicalPlan::Distinct(n) => Self::collect_output_aliases(&n.input, out),
            PhysicalPlan::NestedLoopJoin(n) => {
                Self::collect_output_aliases(&n.left, out);
                Self::collect_output_aliases(&n.right, out);
            }
            PhysicalPlan::LeftOuterJoin(n) => {
                Self::collect_output_aliases(&n.left, out);
                Self::collect_output_aliases(&n.right, out);
            }
            PhysicalPlan::Expand(n) => {
                out.insert(n.start_node_alias.clone());
                out.insert(n.rel_alias.clone());
                out.insert(n.end_node_alias.clone());
                Self::collect_output_aliases(&n.input, out);
            }
            PhysicalPlan::ExpandVariable(n) => {
                out.insert(n.start_node_alias.clone());
                out.insert(n.rel_alias.clone());
                out.insert(n.end_node_alias.clone());
                Self::collect_output_aliases(&n.input, out);
            }
            PhysicalPlan::Unwind(n) => {
                out.insert(n.alias.clone());
                Self::collect_output_aliases(&n.input, out);
            }
            PhysicalPlan::Create(_) | PhysicalPlan::Set(_) | PhysicalPlan::Delete(_) => {}
        }
    }

    fn expr_to_alias(expr: &Expression) -> String {
        match expr {
            Expression::Variable(v) => v.clone(),
            Expression::PropertyAccess(p) => format!("{}.{}", p.variable, p.property),
            _ => "expr".to_string(),
        }
    }

    fn contains_aggregate(expr: &Expression) -> bool {
        matches!(expr, Expression::FunctionCall(fc) if Self::is_aggregate_function(&fc.name))
    }

    fn is_aggregate_function(name: &str) -> bool {
        matches!(name.to_uppercase().as_str(), "COUNT" | "SUM" | "AVG")
    }

    fn extract_aggregate(expr: &Expression) -> Option<(AggregateFunction, Expression)> {
        if let Expression::FunctionCall(fc) = expr {
            if Self::is_aggregate_function(&fc.name) {
                match fc.name.to_uppercase().as_str() {
                    "COUNT" => {
                        let arg = fc.args.first().cloned();
                        Some((AggregateFunction::Count(arg), expr.clone()))
                    }
                    "SUM" => {
                        if let Some(arg) = fc.args.first().cloned() {
                            Some((AggregateFunction::Sum(arg), expr.clone()))
                        } else {
                            None
                        }
                    }
                    "AVG" => {
                        if let Some(arg) = fc.args.first().cloned() {
                            Some((AggregateFunction::Avg(arg), expr.clone()))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            } else {
                None
            }
        } else {
            None
        }
    }
}

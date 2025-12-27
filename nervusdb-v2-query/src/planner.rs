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
    Limit(LimitNode),
    Skip(SkipNode),
    Sort(SortNode),
    Distinct(DistinctNode),
    NestedLoopJoin(NestedLoopJoinNode),
    LeftOuterJoin(LeftOuterJoinNode),
    Expand(ExpandNode),
    Unwind(UnwindNode),
    Create(CreateNode),
    Set(SetNode),
    Delete(DeleteNode),
    // v1-only nodes intentionally dropped for v2 M3 (FTS/Vector/Aggregate/VarLength...)
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
        // T50: minimal implementation to keep the crate compiling.
        // T51 will replace this with GraphSnapshot-oriented operators.
        let aliases = Self::extract_pattern_aliases(pattern);
        let alias = aliases.first().cloned().unwrap_or_else(|| "_".to_string());
        Ok(PhysicalPlan::Scan(ScanNode {
            alias,
            labels: Vec::new(),
        }))
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
}

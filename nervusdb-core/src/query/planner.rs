use crate::error::Error;
use crate::query::ast::*;

#[derive(Debug, Clone)]
pub enum PhysicalPlan {
    Scan(ScanNode),
    Filter(FilterNode),
    Project(ProjectNode),
    Limit(LimitNode),
    NestedLoopJoin(NestedLoopJoinNode),
    Expand(ExpandNode),
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
pub struct NestedLoopJoinNode {
    pub left: Box<PhysicalPlan>,
    pub right: Box<PhysicalPlan>,
    pub predicate: Option<Expression>,
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
                    let match_plan = self.plan_match(match_clause)?;
                    if let Some(current_plan) = plan {
                        // Implicit join with previous match
                        plan = Some(PhysicalPlan::NestedLoopJoin(NestedLoopJoinNode {
                            left: Box::new(current_plan),
                            right: Box::new(match_plan),
                            predicate: None,
                        }));
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
                _ => return Err(Error::Other("Unsupported clause".to_string())),
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

            if let Some(limit) = r.limit {
                final_plan = PhysicalPlan::Limit(LimitNode {
                    input: Box::new(final_plan),
                    limit,
                });
            }
        }

        Ok(final_plan)
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
                    // Expect next element to be a Node
                    if let Some(PathElement::Node(next_node)) = elements.next() {
                        let start_alias = last_node_alias.ok_or_else(|| {
                            Error::Other("Relationship without start node".to_string())
                        })?;
                        let end_alias = next_node.variable.unwrap_or_else(|| {
                            anon_idx += 1;
                            format!("_anon{}", anon_idx)
                        });
                        let rel_alias = rel.variable.unwrap_or_else(|| "rel".to_string()); // Default alias if none

                        let current_plan = plan.ok_or_else(|| {
                            Error::Other("Relationship without start node plan".to_string())
                        })?;

                        plan = Some(PhysicalPlan::Expand(ExpandNode {
                            input: Box::new(current_plan),
                            start_node_alias: start_alias,
                            rel_alias,
                            end_node_alias: end_alias.clone(),
                            direction: rel.direction,
                            rel_type: rel.types.first().cloned(), // TODO: Handle multiple types
                        }));

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
}

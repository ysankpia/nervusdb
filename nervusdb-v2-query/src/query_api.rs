use crate::ast::{Clause, Expression, Literal, Query, RelationshipDirection};
use crate::error::{Error, Result};
use crate::executor::{Plan, Row, Value, execute_plan, parse_u32_identifier};
use nervusdb_v2_api::{GraphSnapshot, RelTypeId};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct Params {
    inner: BTreeMap<String, Value>,
}

impl Params {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: impl Into<String>, value: Value) {
        self.inner.insert(name.into(), value);
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.inner.get(name)
    }
}

#[derive(Debug, Clone)]
pub struct PreparedQuery {
    plan: Plan,
}

impl PreparedQuery {
    pub fn execute_streaming<'a, S: GraphSnapshot + 'a>(
        &'a self,
        snapshot: &'a S,
        params: &'a Params,
    ) -> impl Iterator<Item = Result<Row>> + 'a {
        execute_plan(snapshot, &self.plan, params)
    }
}

pub fn prepare(cypher: &str) -> Result<PreparedQuery> {
    let query = crate::parser::Parser::parse(cypher)?;
    let plan = compile_m3_plan(query)?;
    Ok(PreparedQuery { plan })
}

fn compile_m3_plan(query: Query) -> Result<Plan> {
    // Supported shapes (M3):
    // - RETURN 1
    // - MATCH (n)-[:<u32>]->(m) [WHERE ...] RETURN n,m [LIMIT k]
    let mut match_clause = None;
    let mut where_clause = None;
    let mut return_clause = None;

    for clause in query.clauses {
        match clause {
            Clause::Match(m) => match_clause = Some(m),
            Clause::Where(w) => where_clause = Some(w),
            Clause::Return(r) => return_clause = Some(r),
            Clause::With(_) => return Err(Error::NotImplemented("WITH in v2 M3")),
            Clause::Create(_) => return Err(Error::NotImplemented("CREATE in v2 M3")),
            Clause::Merge(_) => return Err(Error::NotImplemented("MERGE in v2 M3")),
            Clause::Unwind(_) => return Err(Error::NotImplemented("UNWIND in v2 M3")),
            Clause::Call(_) => return Err(Error::NotImplemented("CALL in v2 M3")),
            Clause::Set(_) => return Err(Error::NotImplemented("SET in v2 M3")),
            Clause::Delete(_) => return Err(Error::NotImplemented("DELETE in v2 M3")),
            Clause::Union(_) => return Err(Error::NotImplemented("UNION in v2 M3")),
        }
    }

    let Some(ret) = return_clause else {
        return Err(Error::NotImplemented("query without RETURN"));
    };

    if match_clause.is_none() {
        if ret.items.len() == 1
            && let Expression::Literal(Literal::Number(n)) = &ret.items[0].expression
            && (*n - 1.0).abs() < f64::EPSILON
        {
            return Ok(Plan::ReturnOne);
        }
        return Err(Error::NotImplemented("RETURN-only query (except RETURN 1)"));
    }

    let m = match_clause.unwrap();
    if m.optional {
        return Err(Error::NotImplemented("OPTIONAL MATCH in v2 M3"));
    }

    if m.pattern.elements.len() != 3 {
        return Err(Error::NotImplemented("only single-hop patterns in v2 M3"));
    }

    let src = match &m.pattern.elements[0] {
        crate::ast::PathElement::Node(n) => n,
        _ => return Err(Error::Other("pattern must start with node".into())),
    };
    let rel_pat = match &m.pattern.elements[1] {
        crate::ast::PathElement::Relationship(r) => r,
        _ => return Err(Error::Other("expected relationship in middle".into())),
    };
    let dst = match &m.pattern.elements[2] {
        crate::ast::PathElement::Node(n) => n,
        _ => return Err(Error::Other("pattern must end with node".into())),
    };

    if !src.labels.is_empty() || !dst.labels.is_empty() {
        return Err(Error::NotImplemented("labels in v2 M3"));
    }

    let src_alias = src
        .variable
        .as_deref()
        .ok_or(Error::NotImplemented("anonymous node"))?
        .to_string();
    let dst_alias = dst
        .variable
        .as_deref()
        .ok_or(Error::NotImplemented("anonymous node"))?
        .to_string();

    if !matches!(rel_pat.direction, RelationshipDirection::LeftToRight) {
        return Err(Error::NotImplemented("only -> direction in v2 M3"));
    }

    let rel: Option<RelTypeId> = match rel_pat.types.as_slice() {
        [] => None,
        [t] => Some(parse_u32_identifier(t)?),
        _ => return Err(Error::NotImplemented("multiple rel types in v2 M3")),
    };

    let edge_alias = rel_pat.variable.clone();

    let mut project: Vec<String> = Vec::new();
    for item in &ret.items {
        let name = match &item.expression {
            Expression::Variable(v) => v.clone(),
            _ => return Err(Error::NotImplemented("only variable projection in v2 M3")),
        };
        project.push(item.alias.clone().unwrap_or(name));
    }

    let mut plan = Plan::MatchOut {
        src_alias,
        rel,
        edge_alias,
        dst_alias,
        limit: ret.limit,
        project: project.clone(),
        project_external: false,
    };

    // Add WHERE filter if present
    if let Some(w) = where_clause {
        plan = Plan::Filter {
            input: Box::new(plan),
            predicate: w.expression,
        };
    }

    // Add projection after filtering (to preserve columns for WHERE evaluation)
    plan = Plan::Project {
        input: Box::new(plan),
        columns: project,
    };

    Ok(plan)
}

use crate::error::{Error, Result};
use nervusdb_v2_api::{EdgeKey, ExternalId, GraphSnapshot, InternalNodeId, RelTypeId};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    NodeId(InternalNodeId),
    ExternalId(ExternalId),
    EdgeKey(EdgeKey),
    Int(i64),
    String(String),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Row {
    // Small row: linear search is fine for MVP.
    cols: Vec<(String, Value)>,
}

impl Row {
    pub fn with(mut self, name: impl Into<String>, value: Value) -> Self {
        let name = name.into();
        if let Some((_k, v)) = self.cols.iter_mut().find(|(k, _)| *k == name) {
            *v = value;
        } else {
            self.cols.push((name, value));
        }
        self
    }

    pub fn get_node(&self, name: &str) -> Option<InternalNodeId> {
        self.cols.iter().find_map(|(k, v)| {
            if k == name {
                match v {
                    Value::NodeId(iid) => Some(*iid),
                    _ => None,
                }
            } else {
                None
            }
        })
    }

    pub fn project(&self, names: &[&str]) -> Row {
        let mut out = Row::default();
        for &name in names {
            if let Some((k, v)) = self.cols.iter().find(|(k, _)| k == name) {
                out.cols.push((k.clone(), v.clone()));
            } else {
                out.cols.push((name.to_string(), Value::Null));
            }
        }
        out
    }

    pub fn columns(&self) -> &[(String, Value)] {
        &self.cols
    }
}

#[derive(Debug, Clone)]
pub enum Plan {
    /// `RETURN 1`
    ReturnOne,
    /// `MATCH (a)-[:rel]->(b) RETURN ...`
    MatchOut {
        src_alias: String,
        rel: Option<RelTypeId>,
        edge_alias: Option<String>,
        dst_alias: String,
        limit: Option<u32>,
        // Note: project is kept for backward compatibility but projection
        // should happen after filtering (see Plan::Project)
        project: Vec<String>,
        project_external: bool,
    },
    /// `MATCH ... WHERE ... RETURN ...` (with filter)
    Filter {
        input: Box<Plan>,
        predicate: crate::ast::Expression,
    },
    /// Project columns from input row (runs after filtering)
    Project {
        input: Box<Plan>,
        columns: Vec<String>,
    },
}

pub fn execute_plan<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    plan: &'a Plan,
    params: &'a crate::query_api::Params,
) -> Box<dyn Iterator<Item = Result<Row>> + 'a> {
    match plan {
        Plan::ReturnOne => Box::new(std::iter::once(Ok(Row::default().with("1", Value::Int(1))))),
        Plan::MatchOut {
            src_alias,
            rel,
            edge_alias,
            dst_alias,
            limit,
            project: _,
            project_external: _,
        } => {
            let base =
                MatchOutIter::new(snapshot, src_alias, *rel, edge_alias.as_deref(), dst_alias);
            if let Some(n) = limit {
                Box::new(base.take(*n as usize))
            } else {
                Box::new(base)
            }
        }
        Plan::Filter { input, predicate } => {
            let input_iter = execute_plan(snapshot, input, params);
            Box::new(input_iter.filter(move |result| {
                match result {
                    Ok(row) => {
                        crate::evaluator::evaluate_expression_bool(predicate, row, snapshot, params)
                    }
                    Err(_) => true, // Pass through errors
                }
            }))
        }
        Plan::Project { input, columns } => {
            let input_iter = execute_plan(snapshot, input, params);
            let names: Vec<&str> = columns.iter().map(|s| s.as_str()).collect();
            Box::new(input_iter.map(move |result| result.map(|row| row.project(&names))))
        }
    }
}

struct MatchOutIter<'a, S: GraphSnapshot + 'a> {
    snapshot: &'a S,
    src_alias: &'a str,
    rel: Option<RelTypeId>,
    edge_alias: Option<&'a str>,
    dst_alias: &'a str,
    node_iter: Box<dyn Iterator<Item = InternalNodeId> + 'a>,
    cur_src: Option<InternalNodeId>,
    cur_edges: Option<S::Neighbors<'a>>,
}

impl<'a, S: GraphSnapshot + 'a> MatchOutIter<'a, S> {
    fn new(
        snapshot: &'a S,
        src_alias: &'a str,
        rel: Option<RelTypeId>,
        edge_alias: Option<&'a str>,
        dst_alias: &'a str,
    ) -> Self {
        Self {
            snapshot,
            src_alias,
            rel,
            edge_alias,
            dst_alias,
            node_iter: snapshot.nodes(),
            cur_src: None,
            cur_edges: None,
        }
    }

    fn next_src(&mut self) -> Option<InternalNodeId> {
        for src in self.node_iter.by_ref() {
            if self.snapshot.is_tombstoned_node(src) {
                continue;
            }
            return Some(src);
        }
        None
    }
}

impl<'a, S: GraphSnapshot + 'a> Iterator for MatchOutIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.cur_edges.is_none() {
                let src = self.next_src()?;
                self.cur_src = Some(src);
                self.cur_edges = Some(self.snapshot.neighbors(src, self.rel));
            }

            let edges = self.cur_edges.as_mut().expect("cur_edges must exist");

            if let Some(edge) = edges.next() {
                let mut row = Row::default().with(self.src_alias, Value::NodeId(edge.src));
                if let Some(edge_alias) = self.edge_alias {
                    row = row.with(edge_alias, Value::EdgeKey(edge));
                }
                row = row.with(self.dst_alias, Value::NodeId(edge.dst));

                // Always return full row - projection happens in Plan::Project
                return Some(Ok(row));
            }

            self.cur_edges = None;
            self.cur_src = None;
        }
    }
}

pub fn parse_u32_identifier(name: &str) -> Result<u32> {
    name.parse::<u32>()
        .map_err(|_| Error::NotImplemented("non-numeric label/rel identifiers in M3"))
}

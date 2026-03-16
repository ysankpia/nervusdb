use super::{
    GraphSnapshot, InternalNodeId, LabelId, Plan, PlanIterator, Result, Row, Value, execute_plan,
};
use crate::ast::Expression;
use std::collections::HashSet;

pub struct NodeScanIter<'a, S: GraphSnapshot> {
    pub(super) snapshot: &'a S,
    pub(super) node_iter: Box<dyn Iterator<Item = InternalNodeId> + 'a>,
    pub(super) alias: &'a str,
    pub(super) label_id: Option<LabelId>,
}

impl<'a, S: GraphSnapshot> Iterator for NodeScanIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        for iid in self.node_iter.by_ref() {
            if self.snapshot.is_tombstoned_node(iid) {
                continue;
            }
            if let Some(lid) = self.label_id {
                let matches_label = self
                    .snapshot
                    .resolve_node_labels(iid)
                    .map(|labels| labels.contains(&lid))
                    .unwrap_or_else(|| self.snapshot.node_label(iid) == Some(lid));
                if !matches_label {
                    continue;
                }
            }
            return Some(Ok(Row::default().with(self.alias, Value::NodeId(iid))));
        }
        None
    }
}

pub struct FilterIter<'a, S: GraphSnapshot> {
    pub(super) snapshot: &'a S,
    pub(super) input: Box<PlanIterator<'a, S>>,
    pub(super) predicate: &'a Expression,
    pub(super) params: &'a crate::query_api::Params,
}

pub struct ProjectIter<'a, S: GraphSnapshot> {
    pub(super) snapshot: &'a S,
    pub(super) input: Box<PlanIterator<'a, S>>,
    pub(super) projections: &'a [(String, Expression)],
    pub(super) params: &'a crate::query_api::Params,
}

impl<'a, S: GraphSnapshot> Iterator for ProjectIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        let row = match self.input.next() {
            Some(Ok(row)) => row,
            Some(Err(err)) => return Some(Err(err)),
            None => return None,
        };
        let mut new_row = Row::default();
        for (alias, expr) in self.projections {
            if let Err(err) = super::plan_mid::ensure_runtime_expression_compatible(
                expr,
                &row,
                self.snapshot,
                self.params,
            ) {
                return Some(Err(err));
            }
            let val =
                crate::evaluator::evaluate_expression_value(expr, &row, self.snapshot, self.params);
            new_row = new_row.with(alias.clone(), val);
        }
        Some(Ok(new_row))
    }
}

pub struct DistinctIter<'a, S: GraphSnapshot> {
    pub(super) input: Box<PlanIterator<'a, S>>,
    pub(super) seen: HashSet<Vec<Value>>,
}

impl<'a, S: GraphSnapshot> Iterator for DistinctIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.input.next() {
                Some(Ok(row)) => {
                    if self.seen.insert(row.value_key()) {
                        return Some(Ok(row));
                    }
                }
                Some(Err(err)) => return Some(Err(err)),
                None => return None,
            }
        }
    }
}

pub struct UnionDistinctIter<'a, S: GraphSnapshot> {
    pub(super) input: std::iter::Chain<PlanIterator<'a, S>, PlanIterator<'a, S>>,
    pub(super) seen: HashSet<Vec<Value>>,
}

impl<'a, S: GraphSnapshot> Iterator for UnionDistinctIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.input.next() {
                Some(Ok(row)) => {
                    if self.seen.insert(row.value_key()) {
                        return Some(Ok(row));
                    }
                }
                Some(Err(err)) => return Some(Err(err)),
                None => return None,
            }
        }
    }
}

pub struct IndexSeekIter {
    pub(super) alias: String,
    pub(super) node_ids: std::vec::IntoIter<InternalNodeId>,
}

impl Iterator for IndexSeekIter {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        self.node_ids
            .next()
            .map(|iid| Ok(Row::default().with(self.alias.clone(), Value::NodeId(iid))))
    }
}

impl<'a, S: GraphSnapshot> Iterator for FilterIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.input.next() {
                Some(Ok(row)) => {
                    if let Err(err) = super::plan_mid::ensure_runtime_expression_compatible(
                        self.predicate,
                        &row,
                        self.snapshot,
                        self.params,
                    ) {
                        return Some(Err(err));
                    }
                    let pass = crate::evaluator::evaluate_expression_bool(
                        self.predicate,
                        &row,
                        self.snapshot,
                        self.params,
                    );
                    if pass {
                        return Some(Ok(row));
                    }
                }
                Some(Err(e)) => return Some(Err(e)),
                None => return None,
            }
        }
    }
}

pub struct CartesianProductIter<'a, S: GraphSnapshot> {
    pub(super) left_iter: Box<PlanIterator<'a, S>>,
    pub(super) right_plan: &'a Plan,
    pub(super) snapshot: &'a S,
    pub(super) params: &'a crate::query_api::Params,
    pub(super) current_left_row: Option<Row>,
    pub(super) current_right_iter: Option<Box<PlanIterator<'a, S>>>,
}

impl<'a, S: GraphSnapshot> Iterator for CartesianProductIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.current_left_row.is_none() {
                match self.left_iter.next() {
                    Some(Ok(row)) => {
                        self.current_left_row = Some(row);
                        self.current_right_iter = Some(Box::new(execute_plan(
                            self.snapshot,
                            self.right_plan,
                            self.params,
                        )));
                    }
                    Some(Err(e)) => return Some(Err(e)),
                    None => return None,
                }
            }

            if let Some(right_iter) = &mut self.current_right_iter {
                match right_iter.next() {
                    Some(Ok(right_row)) => {
                        let left_row = self.current_left_row.as_ref().expect("left row present");
                        return Some(Ok(left_row.join(&right_row)));
                    }
                    Some(Err(e)) => return Some(Err(e)),
                    None => {
                        self.current_left_row = None;
                        self.current_right_iter = None;
                    }
                }
            }
        }
    }
}

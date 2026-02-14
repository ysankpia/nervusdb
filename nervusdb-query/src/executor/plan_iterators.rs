use super::{
    GraphSnapshot, InternalNodeId, LabelId, Plan, PlanIterator, Result, Row, Value, execute_plan,
};
use crate::ast::Expression;

pub struct NodeScanIter<'a, S: GraphSnapshot> {
    pub(super) snapshot: &'a S,
    pub(super) node_iter: Box<dyn Iterator<Item = InternalNodeId> + 'a>,
    pub(super) alias: String,
    pub(super) label_id: Option<LabelId>,
}

impl<'a, S: GraphSnapshot> Iterator for NodeScanIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        for iid in self.node_iter.by_ref() {
            if self.snapshot.is_tombstoned_node(iid) {
                continue;
            }
            if let Some(lid) = self.label_id
                && self.snapshot.node_label(iid) != Some(lid)
            {
                continue;
            }
            return Some(Ok(
                Row::default().with(self.alias.clone(), Value::NodeId(iid))
            ));
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

impl<'a, S: GraphSnapshot> Iterator for FilterIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.input.next() {
                Some(Ok(row)) => {
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

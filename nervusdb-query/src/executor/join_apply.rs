use super::{
    ErasedSnapshot, Error, Plan, PlanIterator, Result, Row, execute_plan, get_procedure_registry,
    get_test_procedure_fixture,
};
use crate::ast::Expression;
use crate::evaluator::evaluate_expression_value;
use nervusdb_api::GraphSnapshot;

pub struct ApplyIter<'a, S: GraphSnapshot> {
    pub(super) input_iter: Box<PlanIterator<'a, S>>,
    pub(super) subquery_plan: &'a Plan,
    pub(super) snapshot: &'a S,
    pub(super) base_params: &'a crate::query_api::Params,
    pub(super) current_outer_row: Option<Row>,
    pub(super) current_results: std::vec::IntoIter<Row>,
}

impl<'a, S: GraphSnapshot> Iterator for ApplyIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // 1. Try to yield from current subquery results
            if let Some(inner_row) = self.current_results.next() {
                if let Some(outer) = &self.current_outer_row {
                    return Some(Ok(outer.join(&inner_row)));
                } else {
                    return Some(Err(Error::Other("Lost outer row in Apply".into())));
                }
            }

            // 2. Consume next outer row
            match self.input_iter.next() {
                Some(Ok(outer_row)) => {
                    self.current_outer_row = Some(outer_row.clone());

                    // Prepare params
                    // We need to merge base_params and outer_row
                    let mut extended_params = self.base_params.clone();
                    for (k, v) in &outer_row.cols {
                        extended_params.insert(k.clone(), v.clone());
                    }

                    // Execute subquery
                    // We must materialize to avoid lifetime issues with local extended_params
                    // Note: execute_plan returns an Iterator. We consume it immediately.
                    let iter = execute_plan(self.snapshot, self.subquery_plan, &extended_params);

                    let results: Vec<Row> = match iter.collect() {
                        Ok(rows) => rows,
                        Err(e) => return Some(Err(e)),
                    };

                    self.current_results = results.into_iter();
                    // Loop will continue and pick up the first result
                }
                Some(Err(e)) => return Some(Err(e)),
                None => return None, // Input exhausted
            }
        }
    }
}

pub struct ProcedureCallIter<'a, S: GraphSnapshot + 'a> {
    input_iter: Box<PlanIterator<'a, S>>,
    proc_name: String,
    args: &'a [Expression],
    yields: &'a [(String, Option<String>)],
    snapshot: &'a S,
    params: &'a crate::query_api::Params,
    current_outer_row: Option<Row>,
    current_results: std::vec::IntoIter<Row>,
}

impl<'a, S: GraphSnapshot + 'a> ProcedureCallIter<'a, S> {
    pub(super) fn new(
        input_iter: Box<PlanIterator<'a, S>>,
        proc_name: String,
        args: &'a [Expression],
        yields: &'a [(String, Option<String>)],
        snapshot: &'a S,
        params: &'a crate::query_api::Params,
    ) -> Self {
        Self {
            input_iter,
            proc_name,
            args,
            yields,
            snapshot,
            params,
            current_outer_row: None,
            current_results: Vec::new().into_iter(),
        }
    }
}

impl<'a, S: GraphSnapshot + 'a> Iterator for ProcedureCallIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // 1. Try to yield from current sub-results
            if let Some(proc_row) = self.current_results.next()
                && let Some(outer_row) = &self.current_outer_row
            {
                // Start with outer row
                let mut joined = outer_row.clone();
                // Merge proc_row into joined, applying YIELD aliases
                if self.yields.is_empty() {
                    // If no yields specified, just merge all?
                    // Actually in Cypher, if no YIELD is specified, it might be an error or return all.
                    // For NervusDB MVP: if yields is empty, assume we return everything from proc_row.
                    for (k, v) in proc_row.cols {
                        joined = joined.with(k, v);
                    }
                } else {
                    for (field, alias) in self.yields {
                        if let Some(val) = proc_row.get(field) {
                            joined = joined.with(alias.as_ref().unwrap_or(field), val.clone());
                        }
                    }
                }
                return Some(Ok(joined));
            }

            // 2. Fetch next outer row
            match self.input_iter.next() {
                Some(Ok(outer_row)) => {
                    // 3. Evaluate arguments
                    let mut eval_args = Vec::with_capacity(self.args.len());
                    for arg in self.args {
                        if let Err(err) = super::plan_mid::ensure_runtime_expression_compatible(
                            arg,
                            &outer_row,
                            self.snapshot,
                            self.params,
                        ) {
                            return Some(Err(err));
                        }
                        let v =
                            evaluate_expression_value(arg, &outer_row, self.snapshot, self.params);
                        eval_args.push(v);
                    }

                    if eval_args.is_empty()
                        && let Some(fixture) = get_test_procedure_fixture(&self.proc_name)
                        && !fixture.inputs.is_empty()
                    {
                        let mut implicit_args = Vec::with_capacity(fixture.inputs.len());
                        for field in fixture.inputs {
                            if let Some(value) = self.params.get(&field.name) {
                                implicit_args.push(value.clone());
                            } else {
                                return Some(Err(Error::Other(
                                    "syntax error: MissingParameter".to_string(),
                                )));
                            }
                        }
                        eval_args = implicit_args;
                    }

                    // 4. Call procedure
                    let registry = get_procedure_registry();
                    if let Some(proc) = registry.get(&self.proc_name) {
                        match proc.execute(self.snapshot as &dyn ErasedSnapshot, eval_args) {
                            Ok(mut results) => {
                                if results.is_empty()
                                    && self.yields.is_empty()
                                    && !outer_row.cols.is_empty()
                                    && get_test_procedure_fixture(&self.proc_name)
                                        .is_some_and(|fixture| fixture.outputs.is_empty())
                                {
                                    // Procedures that expose no output columns should keep
                                    // the incoming row cardinality in in-query CALL pipelines.
                                    results.push(Row::default());
                                }
                                self.current_outer_row = Some(outer_row);
                                self.current_results = results.into_iter();
                                // Loop continues to yield from current_results
                            }
                            Err(e) => return Some(Err(e)),
                        }
                    } else {
                        return Some(Err(Error::Other(format!(
                            "Procedure {} not found",
                            self.proc_name
                        ))));
                    }
                }
                Some(Err(e)) => return Some(Err(e)),
                None => return None,
            }
        }
    }
}

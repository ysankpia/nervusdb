use super::{
    Error, GraphSnapshot, Params, PreparedQuery, Result, Row, Value, WriteSemantics, execute_plan,
    execute_write, plan_contains_write,
};

impl PreparedQuery {
    fn should_clear_write_rows(plan: &crate::executor::Plan) -> bool {
        matches!(
            plan,
            crate::executor::Plan::Create { .. }
                | crate::executor::Plan::Delete { .. }
                | crate::executor::Plan::SetProperty { .. }
                | crate::executor::Plan::SetPropertiesFromMap { .. }
                | crate::executor::Plan::SetLabels { .. }
                | crate::executor::Plan::RemoveProperty { .. }
                | crate::executor::Plan::RemoveLabels { .. }
                | crate::executor::Plan::Foreach { .. }
        )
    }

    /// Executes a read query and returns a streaming iterator.
    ///
    /// The returned iterator yields `Result<Row>`, where each row
    /// represents a result record. Errors can occur during execution
    /// (e.g., type mismatches, missing variables).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let query = prepare("MATCH (n)-[:1]->(m) RETURN n, m LIMIT 10").unwrap();
    /// let rows: Vec<_> = query
    ///     .execute_streaming(&snapshot, &Params::new())
    ///     .collect::<Result<_>>()
    ///     .unwrap();
    /// ```
    pub fn execute_streaming<'a, S: GraphSnapshot + 'a>(
        &'a self,
        snapshot: &'a S,
        params: &'a Params,
    ) -> impl Iterator<Item = Result<Row>> + 'a {
        if let Some(plan) = &self.explain {
            let it: Box<dyn Iterator<Item = Result<Row>> + 'a> = Box::new(std::iter::once(Ok(
                Row::default().with("plan", Value::String(plan.clone())),
            )));
            return it;
        }
        params.begin_execution();
        Box::new(execute_plan(snapshot, &self.plan, params))
    }

    /// Executes a write query (CREATE/DELETE) with a write transaction.
    ///
    /// Returns the number of entities created/deleted.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let query = prepare("CREATE (n)").unwrap();
    /// let mut txn = db.begin_write();
    /// let count = query.execute_write(&snapshot, &mut txn, &Params::new()).unwrap();
    /// txn.commit().unwrap();
    /// ```
    pub fn execute_write<S: GraphSnapshot>(
        &self,
        snapshot: &S,
        txn: &mut impl crate::executor::WriteableGraph,
        params: &Params,
    ) -> Result<u32> {
        if self.explain.is_some() {
            return Err(Error::Other(
                "EXPLAIN cannot be executed as a write query".into(),
            ));
        }
        params.begin_execution();
        match self.write {
            WriteSemantics::Default => execute_write(&self.plan, snapshot, txn, params),
            WriteSemantics::Merge => Err(Error::Other(
                "MERGE not yet supported in 0.1 slim build".into(),
            )),
        }
    }

    pub fn execute_mixed<S: GraphSnapshot>(
        &self,
        snapshot: &S,
        txn: &mut impl crate::executor::WriteableGraph,
        params: &Params,
    ) -> Result<(
        Vec<std::collections::HashMap<String, crate::executor::Value>>,
        u32,
    )> {
        if self.explain.is_some() {
            return Err(Error::Other(
                "EXPLAIN cannot be executed as a mixed query".into(),
            ));
        }
        params.begin_execution();

        if plan_contains_write(&self.plan) {
            return Err(Error::Other(
                "mixed write+read queries not yet supported in 0.1 slim build".into(),
            ));
        }

        let rows: Vec<_> = crate::executor::execute_plan(snapshot, &self.plan, params).collect();
        let mut results = Vec::new();

        for row_res in rows {
            let row = row_res?;
            let mut map = std::collections::HashMap::new();
            for (k, v) in row.columns().iter().cloned() {
                map.insert(k, v);
            }
            results.push(map);
        }

        Ok((results, 0))
    }

    pub fn is_explain(&self) -> bool {
        self.explain.is_some()
    }

    /// Returns the explained plan string if this query was an EXPLAIN query.
    pub fn explain_string(&self) -> Option<&str> {
        self.explain.as_deref()
    }
}

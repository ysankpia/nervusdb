use super::Result;
use super::plan::logical::LogicalPlan;
use super::plan::physical::PhysicalPlan;

pub(super) fn build_logical(query: crate::query::ast::Query) -> LogicalPlan {
    LogicalPlan::new(query)
}

pub(super) fn build_physical(plan: LogicalPlan) -> Result<PhysicalPlan> {
    let LogicalPlan { query } = plan;

    let compiled = super::compile_m3_plan(query, None)?;
    Ok(compiled.into())
}

#[cfg(test)]
mod tests {
    use super::{build_logical, build_physical};
    use crate::query::query_api::plan::optimizer::optimize;

    #[test]
    fn planner_pipeline_compiles_read_query() {
        let query = crate::query::parser::Parser::parse("MATCH (n) RETURN n LIMIT 1")
            .expect("parse should succeed");
        let logical = build_logical(query);
        let physical = build_physical(optimize(logical)).expect("build physical should succeed");
        assert!(!matches!(
            physical.plan,
            crate::query::executor::Plan::ReturnOne
        ));
    }

    #[test]
    fn planner_pipeline_compiles_write_query() {
        let query = crate::query::parser::Parser::parse("CREATE (n:1 {name: 'x'})")
            .expect("parse should succeed");
        let logical = build_logical(query);
        let physical = build_physical(optimize(logical)).expect("build physical should succeed");
        assert!(matches!(
            physical.plan,
            crate::query::executor::Plan::Create { .. }
        ));
    }
}

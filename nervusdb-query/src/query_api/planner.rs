use super::plan::logical::LogicalPlan;
use super::plan::physical::PhysicalPlan;
use super::{Error, Result};

pub(super) fn build_logical(
    query: crate::ast::Query,
    merge_subclauses: std::collections::VecDeque<crate::parser::MergeSubclauses>,
) -> LogicalPlan {
    LogicalPlan::new(query, merge_subclauses)
}

pub(super) fn build_physical(plan: LogicalPlan) -> Result<PhysicalPlan> {
    let LogicalPlan {
        query,
        mut merge_subclauses,
    } = plan;

    let compiled = super::compile_m3_plan(query, &mut merge_subclauses, None)?;
    if !merge_subclauses.is_empty() {
        return Err(Error::Other(
            "internal error: unconsumed MERGE subclauses".into(),
        ));
    }

    Ok(compiled.into())
}

#[cfg(test)]
mod tests {
    use super::{build_logical, build_physical};
    use crate::query_api::plan::optimizer::optimize;
    use std::collections::VecDeque;

    #[test]
    fn planner_pipeline_compiles_read_query() {
        let (query, merge_subclauses) =
            crate::parser::Parser::parse_with_merge_subclauses("MATCH (n) RETURN n LIMIT 1")
                .expect("parse should succeed");
        let logical = build_logical(query, VecDeque::from(merge_subclauses));
        let physical = build_physical(optimize(logical)).expect("build physical should succeed");
        assert!(!matches!(physical.plan, crate::executor::Plan::ReturnOne));
    }

    #[test]
    fn planner_pipeline_compiles_write_query() {
        let (query, merge_subclauses) =
            crate::parser::Parser::parse_with_merge_subclauses("CREATE (n:1 {name: 'x'})")
                .expect("parse should succeed");
        let logical = build_logical(query, VecDeque::from(merge_subclauses));
        let physical = build_physical(optimize(logical)).expect("build physical should succeed");
        assert!(matches!(
            physical.plan,
            crate::executor::Plan::Create { .. }
        ));
    }
}

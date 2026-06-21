use super::{Error, PreparedQuery, Result, render_plan, strip_explain_prefix};

pub(super) fn prepare(cypher: &str) -> Result<PreparedQuery> {
    if let Some(inner) = strip_explain_prefix(cypher) {
        if inner.is_empty() {
            return Err(Error::Other("EXPLAIN requires a query".into()));
        }
        let query = crate::query::parser::Parser::parse(inner)?;
        let logical = super::planner::build_logical(query);
        let optimized = super::plan::optimizer::optimize(logical);
        let physical = super::planner::build_physical(optimized)?;
        let explain = Some(render_plan(&physical.plan));
        return Ok(PreparedQuery {
            plan: physical.plan,
            explain,
        });
    }

    let query = crate::query::parser::Parser::parse(cypher)?;
    let logical = super::planner::build_logical(query);
    let optimized = super::plan::optimizer::optimize(logical);
    let physical = super::planner::build_physical(optimized)?;
    Ok(PreparedQuery {
        plan: physical.plan,
        explain: None,
    })
}

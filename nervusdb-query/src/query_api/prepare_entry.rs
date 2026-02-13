use super::{Error, PreparedQuery, Result, VecDeque, render_plan, strip_explain_prefix};

pub(super) fn prepare(cypher: &str) -> Result<PreparedQuery> {
    if let Some(inner) = strip_explain_prefix(cypher) {
        if inner.is_empty() {
            return Err(Error::Other("EXPLAIN requires a query".into()));
        }
        let (query, merge_subclauses) = crate::parser::Parser::parse_with_merge_subclauses(inner)?;
        let logical = super::planner::build_logical(query, VecDeque::from(merge_subclauses));
        let optimized = super::plan::optimizer::optimize(logical);
        let physical = super::planner::build_physical(optimized)?;
        let explain = Some(render_plan(&physical.plan));
        return Ok(PreparedQuery {
            plan: physical.plan,
            explain,
            write: physical.write,
            merge_on_create_items: physical.merge_on_create_items,
            merge_on_match_items: physical.merge_on_match_items,
            merge_on_create_labels: physical.merge_on_create_labels,
            merge_on_match_labels: physical.merge_on_match_labels,
        });
    }

    let (query, merge_subclauses) = crate::parser::Parser::parse_with_merge_subclauses(cypher)?;
    let logical = super::planner::build_logical(query, VecDeque::from(merge_subclauses));
    let optimized = super::plan::optimizer::optimize(logical);
    let physical = super::planner::build_physical(optimized)?;
    Ok(PreparedQuery {
        plan: physical.plan,
        explain: None,
        write: physical.write,
        merge_on_create_items: physical.merge_on_create_items,
        merge_on_match_items: physical.merge_on_match_items,
        merge_on_create_labels: physical.merge_on_create_labels,
        merge_on_match_labels: physical.merge_on_match_labels,
    })
}

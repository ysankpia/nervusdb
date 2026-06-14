use super::{Error, Plan, Result};

pub(super) fn compile_foreach_plan(
    _input: Plan,
    _foreach: crate::ast::ForeachClause,
) -> Result<Plan> {
    Err(Error::Other(
        "syntax error: FOREACH not yet supported".to_string(),
    ))
}

use crate::ast::Query;

#[derive(Debug, Clone)]
pub(crate) struct LogicalPlan {
    pub(crate) query: Query,
}

impl LogicalPlan {
    pub(crate) fn new(query: Query) -> Self {
        Self { query }
    }
}

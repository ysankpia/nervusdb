use crate::ast::Query;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub(crate) struct LogicalPlan {
    pub(crate) query: Query,
    pub(crate) merge_subclauses: VecDeque<crate::parser::MergeSubclauses>,
}

impl LogicalPlan {
    pub(crate) fn new(
        query: Query,
        merge_subclauses: VecDeque<crate::parser::MergeSubclauses>,
    ) -> Self {
        Self {
            query,
            merge_subclauses,
        }
    }
}

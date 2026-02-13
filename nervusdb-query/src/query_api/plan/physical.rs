use super::super::{Expression, Plan, WriteSemantics};

#[derive(Debug, Clone)]
pub(crate) struct PhysicalPlan {
    pub(crate) plan: Plan,
    pub(crate) write: WriteSemantics,
    pub(crate) merge_on_create_items: Vec<(String, String, Expression)>,
    pub(crate) merge_on_match_items: Vec<(String, String, Expression)>,
    pub(crate) merge_on_create_labels: Vec<(String, Vec<String>)>,
    pub(crate) merge_on_match_labels: Vec<(String, Vec<String>)>,
}

impl From<super::super::compile_core::CompiledQuery> for PhysicalPlan {
    fn from(compiled: super::super::compile_core::CompiledQuery) -> Self {
        Self {
            plan: compiled.plan,
            write: compiled.write,
            merge_on_create_items: compiled.merge_on_create_items,
            merge_on_match_items: compiled.merge_on_match_items,
            merge_on_create_labels: compiled.merge_on_create_labels,
            merge_on_match_labels: compiled.merge_on_match_labels,
        }
    }
}

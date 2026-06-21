use super::super::Plan;

#[derive(Debug, Clone)]
pub(crate) struct PhysicalPlan {
    pub(crate) plan: Plan,
}

impl From<super::super::compile_core::CompiledQuery> for PhysicalPlan {
    fn from(compiled: super::super::compile_core::CompiledQuery) -> Self {
        Self {
            plan: compiled.plan,
        }
    }
}

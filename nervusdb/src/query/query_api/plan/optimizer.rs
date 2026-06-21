use super::logical::LogicalPlan;

/// Phase1c baseline optimizer: identity rewrite.
///
/// This keeps behavior stable while routing all queries through the
/// LogicalPlan -> Optimizer -> PhysicalPlan pipeline.
pub(crate) fn optimize(plan: LogicalPlan) -> LogicalPlan {
    plan
}

use super::{
    BTreeMap, Plan, Query, Result, Row, Value, VecDeque, compile_m3_plan, extract_output_var_kinds,
};

pub(super) fn compile_foreach_plan(
    input: Plan,
    foreach: crate::ast::ForeachClause,
    merge_subclauses: &mut VecDeque<crate::parser::MergeSubclauses>,
) -> Result<Plan> {
    // Compile updates sub-plan with a scoped placeholder input.
    // It must include both upstream bindings and FOREACH iteration variable,
    // otherwise CREATE property validation may incorrectly reject variables
    // referenced inside FOREACH bodies.
    let mut known_bindings = BTreeMap::new();
    extract_output_var_kinds(&input, &mut known_bindings);

    let mut seed_row = Row::default();
    for name in known_bindings.keys() {
        seed_row = seed_row.with(name.clone(), Value::Null);
    }
    seed_row = seed_row.with(foreach.variable.clone(), Value::Null);

    let initial_input = Some(Plan::Values {
        rows: vec![seed_row],
    });

    // Wrap updates in a Query structure for compilation
    let sub_query = Query {
        clauses: foreach.updates,
    };

    let compiled_sub = compile_m3_plan(sub_query, merge_subclauses, initial_input)?;

    Ok(Plan::Foreach {
        input: Box::new(input),
        variable: foreach.variable,
        list: foreach.list,
        sub_plan: Box::new(compiled_sub.plan),
    })
}

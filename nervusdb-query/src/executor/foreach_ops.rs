use super::{
    Error, Expression, GraphSnapshot, Plan, Result, Value, WriteableGraph,
    evaluate_expression_value, execute_plan, execute_write,
};

pub(super) fn execute_foreach<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
    txn: &mut dyn WriteableGraph,
    variable: &str,
    list: &Expression,
    sub_plan: &Plan,
    params: &crate::query_api::Params,
) -> Result<u32> {
    let mut total_mods = 0;

    for row in execute_plan(snapshot, input, params) {
        let row = row?;
        let list_val = evaluate_expression_value(list, &row, snapshot, params);

        let items = match list_val {
            Value::List(l) => l,
            _ => {
                return Err(Error::Other(format!(
                    "FOREACH expression must evaluate to a list, got {:?}",
                    list_val
                )));
            }
        };

        for item in items {
            let sub_row = row.clone().with(variable, item.clone());
            let mut current_sub_plan = sub_plan.clone();
            inject_rows(&mut current_sub_plan, vec![sub_row]);
            let mods = execute_write(&current_sub_plan, snapshot, txn, params)?;
            total_mods += mods;
        }
    }

    Ok(total_mods)
}

fn inject_rows(plan: &mut Plan, rows: Vec<super::Row>) {
    match plan {
        Plan::Values { rows: target_rows } => {
            *target_rows = rows;
        }
        Plan::Create { input, .. }
        | Plan::Delete { input, .. }
        | Plan::SetProperty { input, .. }
        | Plan::SetPropertiesFromMap { input, .. }
        | Plan::SetLabels { input, .. }
        | Plan::RemoveProperty { input, .. }
        | Plan::RemoveLabels { input, .. }
        | Plan::Foreach { input, .. }
        | Plan::Filter { input, .. }
        | Plan::Project { input, .. }
        | Plan::Limit { input, .. }
        | Plan::OptionalWhereFixup { outer: input, .. }
        | Plan::Skip { input, .. }
        | Plan::OrderBy { input, .. }
        | Plan::Distinct { input }
        | Plan::Unwind { input, .. }
        | Plan::Aggregate { input, .. } => inject_rows(input, rows),
        Plan::CartesianProduct { left, .. } | Plan::Union { left, .. } => inject_rows(left, rows),
        Plan::Apply { input, .. } => inject_rows(input, rows),
        _ => {}
    }
}

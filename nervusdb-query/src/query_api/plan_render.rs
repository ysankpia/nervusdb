use crate::executor::Plan;
use std::fmt::Write as _;

pub(super) fn render_plan(plan: &Plan) -> String {
    fn indent(n: usize) -> String {
        "  ".repeat(n)
    }

    fn go(out: &mut String, plan: &Plan, depth: usize) {
        let pad = indent(depth);
        match plan {
            Plan::ReturnOne => {
                let _ = writeln!(out, "{pad}ReturnOne");
            }
            Plan::Values { rows } => {
                let _ = writeln!(out, "{pad}Values(rows={})", rows.len());
            }
            Plan::Create {
                input,
                pattern,
                merge,
            } => {
                let _ = writeln!(out, "{pad}Create(merge={merge}, pattern={pattern:?})");
                go(out, input, depth + 1);
            }
            Plan::Foreach {
                input,
                variable,
                list,
                sub_plan,
            } => {
                let _ = writeln!(out, "{pad}Foreach(var={variable}, list={list:?})");
                go(out, input, depth + 1);
                let _ = writeln!(out, "{pad}  SubPlan:");
                go(out, sub_plan, depth + 2);
            }

            Plan::NodeScan {
                alias,
                label,
                optional,
            } => {
                let opt = if *optional { " OPTIONAL" } else { "" };
                let _ = writeln!(out, "{pad}NodeScan{opt}(alias={alias}, label={label:?})");
            }
            Plan::MatchOut {
                input: _,
                src_alias,
                rels,
                edge_alias,
                dst_alias,
                dst_labels: _,
                src_prebound: _,
                limit,
                project: _,
                project_external: _,
                optional,
                optional_unbind: _,
                path_alias,
            } => {
                let opt_str = if *optional { " OPTIONAL" } else { "" };
                let path_str = if let Some(p) = path_alias {
                    format!(" path={p}")
                } else {
                    "".to_string()
                };
                let _ = writeln!(
                    out,
                    "{pad}MatchOut{opt_str}(src={src_alias}, rels={rels:?}, edge={edge_alias:?}, dst={dst_alias}, limit={limit:?}{path_str})"
                );
            }
            Plan::MatchOutVarLen {
                input: _,
                src_alias,
                rels,
                edge_alias,
                dst_alias,
                dst_labels: _,
                src_prebound: _,
                direction,
                min_hops,
                max_hops,
                limit,
                project: _,
                project_external: _,
                optional,
                optional_unbind: _,
                path_alias,
            } => {
                let opt_str = if *optional { " OPTIONAL" } else { "" };
                let path_str = if let Some(p) = path_alias {
                    format!(" path={p}")
                } else {
                    "".to_string()
                };
                let _ = writeln!(
                    out,
                    "{pad}MatchOutVarLen{opt_str}(src={src_alias}, rels={rels:?}, edge={edge_alias:?}, dst={dst_alias}, dir={direction:?}, min={min_hops}, max={max_hops:?}, limit={limit:?}{path_str})"
                );
            }
            Plan::MatchIn {
                input,
                src_alias,
                rels,
                edge_alias,
                dst_alias,
                dst_labels: _,
                src_prebound: _,
                limit,
                optional,
                optional_unbind: _,
                path_alias,
            } => {
                let opt_str = if *optional { " OPTIONAL" } else { "" };
                let path_str = if let Some(p) = path_alias {
                    format!(" path={p}")
                } else {
                    "".to_string()
                };
                let _ = writeln!(
                    out,
                    "{pad}MatchIn{opt_str}(src={src_alias}, rels={rels:?}, edge={edge_alias:?}, dst={dst_alias}, limit={limit:?}{path_str})"
                );
                if let Some(p) = input {
                    go(out, p, depth + 1);
                }
            }
            Plan::MatchUndirected {
                input,
                src_alias,
                rels,
                edge_alias,
                dst_alias,
                dst_labels: _,
                src_prebound: _,
                limit,
                optional,
                optional_unbind: _,
                path_alias,
            } => {
                let opt_str = if *optional { " OPTIONAL" } else { "" };
                let path_str = if let Some(p) = path_alias {
                    format!(" path={p}")
                } else {
                    "".to_string()
                };
                let _ = writeln!(
                    out,
                    "{pad}MatchUndirected{opt_str}(src={src_alias}, rels={rels:?}, edge={edge_alias:?}, dst={dst_alias}, limit={limit:?}{path_str})"
                );
                if let Some(p) = input {
                    go(out, p, depth + 1);
                }
            }
            Plan::MatchBoundRel {
                input,
                rel_alias,
                src_alias,
                dst_alias,
                dst_labels: _,
                src_prebound: _,
                rels,
                direction,
                optional,
                optional_unbind: _,
                path_alias,
            } => {
                let opt_str = if *optional { " OPTIONAL" } else { "" };
                let path_str = if let Some(p) = path_alias {
                    format!(" path={p}")
                } else {
                    "".to_string()
                };
                let _ = writeln!(
                    out,
                    "{pad}MatchBoundRel{opt_str}(rel={rel_alias}, src={src_alias}, rels={rels:?}, dst={dst_alias}, dir={direction:?}{path_str})"
                );
                go(out, input, depth + 1);
            }
            Plan::Filter { input, predicate } => {
                let _ = writeln!(out, "{pad}Filter(predicate={predicate:?})");
                go(out, input, depth + 1);
            }
            Plan::OptionalWhereFixup {
                outer,
                filtered,
                null_aliases,
            } => {
                let _ = writeln!(
                    out,
                    "{pad}OptionalWhereFixup(null_aliases={null_aliases:?})"
                );
                let _ = writeln!(out, "{pad}  Outer:");
                go(out, outer, depth + 2);
                let _ = writeln!(out, "{pad}  Filtered:");
                go(out, filtered, depth + 2);
            }
            Plan::Project { input, projections } => {
                let _ = writeln!(out, "{pad}Project(len={})", projections.len());
                go(out, input, depth + 1);
            }
            Plan::Aggregate {
                input,
                group_by,
                aggregates,
            } => {
                let _ = writeln!(
                    out,
                    "{pad}Aggregate(group_by={group_by:?}, aggregates={aggregates:?})"
                );
                go(out, input, depth + 1);
            }
            Plan::OrderBy { input, items } => {
                let _ = writeln!(out, "{pad}OrderBy(items={items:?})");
                go(out, input, depth + 1);
            }
            Plan::Skip { input, skip } => {
                let _ = writeln!(out, "{pad}Skip(skip={skip:?})");
                go(out, input, depth + 1);
            }
            Plan::Limit { input, limit } => {
                let _ = writeln!(out, "{pad}Limit(limit={limit:?})");
                go(out, input, depth + 1);
            }
            Plan::CartesianProduct { left, right } => {
                let _ = writeln!(out, "{pad}CartesianProduct");
                go(out, left, depth + 1);
                go(out, right, depth + 1);
            }
            Plan::Apply {
                input,
                subquery,
                alias,
            } => {
                let _ = writeln!(out, "{pad}Apply(alias={alias:?})");
                go(out, input, depth + 1);
                let _ = writeln!(out, "{pad}  Subquery:");
                go(out, subquery, depth + 2);
            }
            Plan::ProcedureCall {
                input,
                name,
                args: _,
                yields,
            } => {
                let yields_str = yields
                    .iter()
                    .map(|(n, a)| {
                        format!(
                            "{n}{}",
                            a.as_ref()
                                .map(|ali| format!(" AS {ali}"))
                                .unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = writeln!(
                    out,
                    "{pad}ProcedureCall(name={}, yields=[{}])",
                    name.join("."),
                    yields_str
                );
                go(out, input, depth + 1);
            }
            Plan::Distinct { input } => {
                let _ = writeln!(out, "{pad}Distinct");
                go(out, input, depth + 1);
            }

            Plan::Delete {
                input,
                detach,
                expressions,
            } => {
                let _ = writeln!(
                    out,
                    "{pad}Delete(detach={detach}, expressions={expressions:?})"
                );
                go(out, input, depth + 1);
            }
            Plan::Unwind {
                input,
                expression,
                alias,
            } => {
                let _ = writeln!(out, "{pad}Unwind(alias={alias}, expression={expression:?})");
                go(out, input, depth + 1);
            }
            Plan::Union { left, right, all } => {
                let _ = writeln!(out, "{pad}Union(all={all})");
                go(out, left, depth + 1);
                go(out, right, depth + 1);
            }
            Plan::SetProperty { input, items } => {
                let _ = writeln!(out, "{pad}SetProperty(items={items:?})");
                go(out, input, depth + 1);
            }
            Plan::SetPropertiesFromMap { input, items } => {
                let _ = writeln!(out, "{pad}SetPropertiesFromMap(items={items:?})");
                go(out, input, depth + 1);
            }
            Plan::SetLabels { input, items } => {
                let _ = writeln!(out, "{pad}SetLabels(items={items:?})");
                go(out, input, depth + 1);
            }
            Plan::RemoveProperty { input, items } => {
                let _ = writeln!(out, "{pad}RemoveProperty(items={items:?})");
                go(out, input, depth + 1);
            }
            Plan::RemoveLabels { input, items } => {
                let _ = writeln!(out, "{pad}RemoveLabels(items={items:?})");
                go(out, input, depth + 1);
            }
            Plan::IndexSeek {
                alias,
                label,
                field,
                value_expr,
                fallback: _fallback,
            } => {
                let _ = writeln!(
                    out,
                    "{pad}IndexSeek(alias={alias}, label={label}, field={field}, value={value_expr:?})"
                );
                // We don't render fallback to avoid noise, as it's just the unoptimized plan
            }
        }
    }

    let mut out = String::new();
    go(&mut out, plan, 0);
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::render_plan;
    use crate::executor::Plan;

    #[test]
    fn render_plan_handles_return_one() {
        let out = render_plan(&Plan::ReturnOne);
        assert_eq!(out, "ReturnOne");
    }
}

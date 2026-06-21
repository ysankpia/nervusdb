use crate::query::executor::Plan;
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
            Plan::Project { input, projections } => {
                let _ = writeln!(out, "{pad}Project(len={})", projections.len());
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
            Plan::SetProperty { input, items } => {
                let _ = writeln!(out, "{pad}SetProperty(items={items:?})");
                go(out, input, depth + 1);
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
    use crate::query::executor::Plan;

    #[test]
    fn render_plan_handles_return_one() {
        let out = render_plan(&Plan::ReturnOne);
        assert_eq!(out, "ReturnOne");
    }
}

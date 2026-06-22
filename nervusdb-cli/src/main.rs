use clap::{Parser, Subcommand, ValueEnum};
use nervusdb::Db;
use nervusdb::GraphSnapshot;
use nervusdb::admin::{FsckIssue, FsckIssueKind, FsckOptions, FsckRepairKind, FsckReport};
use nervusdb::query::Value as V2Value;
use nervusdb::query::prepare;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "nervusdb", version, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    V2(V2Args),
}

#[derive(Parser)]
struct V2Args {
    #[command(subcommand)]
    command: V2Commands,
}

mod repl;

#[derive(Subcommand)]
enum V2Commands {
    Query(V2QueryArgs),
    Write(V2WriteArgs),
    Repl(V2ReplArgs),
    Fsck(V2FsckArgs),
}

#[derive(Parser)]
struct V2ReplArgs {
    /// Database base path
    #[arg(long)]
    db: PathBuf,
}

#[derive(Parser)]
struct V2FsckArgs {
    /// Local database directory
    #[arg(long)]
    db: PathBuf,

    /// Rebuild repairable derived indexes after checking
    #[arg(long)]
    repair: bool,

    /// Emit a machine-readable JSON report
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Ndjson,
}

#[derive(Parser)]
struct V2QueryArgs {
    /// Local database directory
    #[arg(long)]
    db: PathBuf,

    /// Mini-Cypher query string supported by the 0.1 surface
    #[arg(long, conflicts_with = "file")]
    cypher: Option<String>,

    /// Read Cypher query from file
    #[arg(long)]
    file: Option<PathBuf>,

    /// Parameters as a JSON object
    #[arg(long)]
    params_json: Option<String>,

    #[arg(long, value_enum, default_value = "ndjson")]
    format: OutputFormat,
}

#[derive(Parser)]
struct V2WriteArgs {
    /// Local database directory
    #[arg(long)]
    db: PathBuf,

    /// Mini-Cypher write string supported by the 0.1 surface
    #[arg(long, conflicts_with = "file")]
    cypher: Option<String>,

    /// Read Cypher query from file
    #[arg(long)]
    file: Option<PathBuf>,

    /// Parameters as a JSON object with scalar values
    #[arg(long)]
    params_json: Option<String>,
}

fn value_to_json_v2<S: GraphSnapshot>(snapshot: &S, value: &V2Value) -> serde_json::Value {
    match value {
        V2Value::NodeId(iid) => {
            if let Some(external) = snapshot.resolve_external(*iid) {
                serde_json::json!({ "internal_node_id": iid, "external_id": external })
            } else {
                serde_json::json!({ "internal_node_id": iid })
            }
        }
        V2Value::ExternalId(id) => serde_json::json!(id),
        V2Value::EdgeKey(e) => serde_json::json!({ "src": e.src, "rel": e.rel, "dst": e.dst }),
        V2Value::Int(i) => serde_json::json!(i),
        V2Value::Float(f) => serde_json::json!(f),
        V2Value::String(s) => serde_json::Value::String(s.clone()),
        V2Value::Bool(b) => serde_json::Value::Bool(*b),
        V2Value::Null => serde_json::Value::Null,
        V2Value::DateTime(i) => serde_json::json!({ "datetime": i }),
        V2Value::Blob(_) => serde_json::json!({ "blob": "<binary data>" }),
        V2Value::Map(m) => {
            let obj = m
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json_v2(snapshot, v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        V2Value::List(list) => {
            let arr: Vec<serde_json::Value> =
                list.iter().map(|v| value_to_json_v2(snapshot, v)).collect();
            serde_json::Value::Array(arr)
        }
        V2Value::Path(path_value) => {
            serde_json::json!({
                "nodes": path_value.nodes,
                "edges": path_value.edges.iter().map(|e| serde_json::json!({
                    "src": e.src,
                    "rel": e.rel,
                    "dst": e.dst
                })).collect::<Vec<_>>()
            })
        }
        V2Value::Node(node) => serde_json::json!({
            "id": node.id,
            "labels": node.labels,
            "properties": node.properties
        }),
        V2Value::Relationship(rel) => serde_json::json!({
            "src": rel.key.src,
            "rel": rel.key.rel,
            "dst": rel.key.dst,
            "properties": rel.properties
        }),
        V2Value::ReifiedPath(path) => serde_json::json!({
            "nodes": path.nodes,
            "relationships": path.relationships
        }),
    }
}

fn parse_params_json_v2(raw: Option<String>) -> Result<nervusdb::query::Params, String> {
    let Some(raw) = raw else {
        return Ok(nervusdb::query::Params::new());
    };
    if raw.trim().is_empty() {
        return Ok(nervusdb::query::Params::new());
    }
    let parsed: HashMap<String, serde_json::Value> = serde_json::from_str(&raw)
        .map_err(|e| format!("params_json must be a JSON object: {e}"))?;
    let mut out = nervusdb::query::Params::new();
    for (k, v) in parsed {
        let vv = match v {
            serde_json::Value::Null => V2Value::Null,
            serde_json::Value::Bool(b) => V2Value::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    V2Value::Int(i)
                } else {
                    return Err("v2 params_json only supports integer numbers in M3".to_string());
                }
            }
            serde_json::Value::String(s) => V2Value::String(s),
            _ => return Err("v2 params_json only supports scalar values in M3".to_string()),
        };
        out.insert(k, vv);
    }
    Ok(out)
}

fn read_query(cypher: Option<&String>, file: Option<&PathBuf>) -> Result<String, String> {
    if let Some(query) = cypher {
        return Ok(query.clone());
    }
    let Some(path) = file else {
        return Err("either --cypher or --file is required".to_string());
    };
    std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read query file {}: {e}", path.display()))
}

fn run_v2_query(args: V2QueryArgs) -> Result<(), String> {
    let query = read_query(args.cypher.as_ref(), args.file.as_ref())?;
    let params = parse_params_json_v2(args.params_json)?;

    let db = Db::open(&args.db).map_err(|e| e.to_string())?;
    let graph_snap = db.snapshot();

    let prepared = prepare(query.as_str()).map_err(|e| e.to_string())?;

    let mut stdout = std::io::stdout().lock();
    match args.format {
        OutputFormat::Ndjson => {
            for row in prepared.execute_streaming(&graph_snap, &params) {
                let row = row.map_err(|e| e.to_string())?;
                let mut map = serde_json::Map::with_capacity(row.columns().len());
                for (k, v) in row.columns() {
                    map.insert(k.clone(), value_to_json_v2(&graph_snap, v));
                }
                serde_json::to_writer(&mut stdout, &serde_json::Value::Object(map))
                    .map_err(|e| e.to_string())?;
                stdout.write_all(b"\n").map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(())
}

fn run_v2_write(args: V2WriteArgs) -> Result<(), String> {
    let query = read_query(args.cypher.as_ref(), args.file.as_ref())?;
    let params = parse_params_json_v2(args.params_json)?;

    let db = Db::open(&args.db).map_err(|e| e.to_string())?;
    let graph_snap = db.snapshot();

    let prepared = prepare(query.as_str()).map_err(|e| e.to_string())?;

    let mut txn = db.begin_write();
    let count = prepared
        .execute_write(&graph_snap, &mut txn, &params)
        .map_err(|e| e.to_string())?;
    txn.commit().map_err(|e| e.to_string())?;

    // Output the count as JSON
    println!(r#"{{"count":{}}}"#, count);

    Ok(())
}

fn run_v2_fsck(args: V2FsckArgs) -> CliExit {
    match nervusdb::admin::fsck(
        &args.db,
        FsckOptions {
            repair: args.repair,
        },
    ) {
        Ok(report) => match write_fsck_report(&report, args.json) {
            Ok(()) if report.ok => CliExit::Ok,
            Ok(()) => CliExit::Issues,
            Err(message) => CliExit::Error(message),
        },
        Err(err) => CliExit::Error(err.to_string()),
    }
}

fn write_fsck_report(report: &FsckReport, json: bool) -> Result<(), String> {
    if json {
        serde_json::to_writer_pretty(std::io::stdout().lock(), report)
            .map_err(|e| e.to_string())?;
        println!();
        return Ok(());
    }

    println!("fsck: {}", if report.ok { "ok" } else { "failed" });
    println!(
        "checked: nodes={} node_labels={} label_nodes={} node_props={} idx_node_props={} adj_out={} adj_in={} edge_props={}",
        report.checked.nodes,
        report.checked.node_labels,
        report.checked.label_nodes,
        report.checked.node_props,
        report.checked.idx_node_props,
        report.checked.adj_out,
        report.checked.adj_in,
        report.checked.edge_props
    );
    println!("issues: {}", report.issues.len());
    for issue in &report.issues {
        println!("- {}", format_fsck_issue(issue));
    }
    println!("repairs: {}", report.repairs.len());
    for repair in &report.repairs {
        println!(
            "- {} removed={} inserted={}",
            format_fsck_repair_kind(repair.kind),
            repair.removed,
            repair.inserted
        );
    }
    Ok(())
}

fn format_fsck_issue(issue: &FsckIssue) -> String {
    let mut out = format_fsck_issue_kind(issue.kind).to_string();
    if let Some(node) = issue.node {
        out.push_str(&format!(" node={node}"));
    }
    if let Some(label) = issue.label {
        out.push_str(&format!(" label={label}"));
    }
    if let Some(rel) = issue.rel {
        out.push_str(&format!(" rel={rel}"));
    }
    if let Some(dst) = issue.dst {
        out.push_str(&format!(" dst={dst}"));
    }
    if let Some(key) = &issue.property_key {
        out.push_str(&format!(" key={key}"));
    }
    out
}

fn format_fsck_issue_kind(kind: FsckIssueKind) -> &'static str {
    match kind {
        FsckIssueKind::MissingLabelNodeIndex => "missing_label_node_index",
        FsckIssueKind::StaleLabelNodeIndex => "stale_label_node_index",
        FsckIssueKind::MissingNodePropertyIndex => "missing_node_property_index",
        FsckIssueKind::StaleNodePropertyIndex => "stale_node_property_index",
        FsckIssueKind::AdjacencyMismatch => "adjacency_mismatch",
        FsckIssueKind::OrphanEdgeProperty => "orphan_edge_property",
        FsckIssueKind::OrphanNodeProperty => "orphan_node_property",
        FsckIssueKind::OrphanNodeLabel => "orphan_node_label",
        FsckIssueKind::MalformedNode => "malformed_node",
        FsckIssueKind::MalformedNodeLabel => "malformed_node_label",
        FsckIssueKind::MalformedLabelNode => "malformed_label_node",
        FsckIssueKind::MalformedNodeProperty => "malformed_node_property",
        FsckIssueKind::MalformedNodePropertyIndex => "malformed_node_property_index",
        FsckIssueKind::MalformedAdjOut => "malformed_adj_out",
        FsckIssueKind::MalformedAdjIn => "malformed_adj_in",
        FsckIssueKind::MalformedEdgeProperty => "malformed_edge_property",
    }
}

fn format_fsck_repair_kind(kind: FsckRepairKind) -> &'static str {
    match kind {
        FsckRepairKind::RebuiltLabelNodes => "rebuilt_label_nodes",
        FsckRepairKind::RebuiltNodePropertyIndex => "rebuilt_node_property_index",
    }
}

enum CliExit {
    Ok,
    Command(Result<(), String>),
    Issues,
    Error(String),
}

fn main() {
    let cli = Cli::parse();
    let exit = match cli.command {
        Commands::V2(args) => match args.command {
            V2Commands::Query(args) => CliExit::Command(run_v2_query(args)),
            V2Commands::Write(args) => CliExit::Command(run_v2_write(args)),
            V2Commands::Repl(args) => CliExit::Command(repl::run_repl(&args.db)),
            V2Commands::Fsck(args) => run_v2_fsck(args),
        },
    };

    match exit {
        CliExit::Ok => {}
        CliExit::Command(Ok(())) => {}
        CliExit::Command(Err(message)) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
        CliExit::Issues => std::process::exit(1),
        CliExit::Error(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nervusdb::admin::{FsckIssueKind, FsckRepairKind};

    #[test]
    fn fsck_text_kinds_are_snake_case() {
        let issue = FsckIssue {
            kind: FsckIssueKind::MissingNodePropertyIndex,
            node: Some(42),
            label: Some(1),
            rel: None,
            dst: None,
            property_key: Some("name".to_string()),
        };

        assert_eq!(
            format_fsck_issue(&issue),
            "missing_node_property_index node=42 label=1 key=name"
        );
        assert_eq!(
            format_fsck_repair_kind(FsckRepairKind::RebuiltNodePropertyIndex),
            "rebuilt_node_property_index"
        );
    }
}

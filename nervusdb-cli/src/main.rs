use clap::{Parser, Subcommand, ValueEnum};
use nervusdb_v2::Db;
use nervusdb_v2_api::{GraphSnapshot, GraphStore};
use nervusdb_v2_query::Value as V2Value;
use nervusdb_v2_query::prepare;
use nervusdb_v2_storage::engine::GraphEngine;
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
}

#[derive(Parser)]
struct V2ReplArgs {
    /// Database base path
    #[arg(long)]
    db: PathBuf,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Ndjson,
}

#[derive(Parser)]
struct V2QueryArgs {
    /// Database base path (v2 will derive `<path>.ndb`/`<path>.wal` if needed)
    #[arg(long)]
    db: PathBuf,

    /// Cypher query string (v2 M3 supports only a minimal subset)
    #[arg(long, conflicts_with = "file")]
    cypher: Option<String>,

    /// Read Cypher query from file
    #[arg(long)]
    file: Option<PathBuf>,

    /// Parameters as a JSON object (M3: parsed but currently ignored by supported queries)
    #[arg(long)]
    params_json: Option<String>,

    #[arg(long, value_enum, default_value = "ndjson")]
    format: OutputFormat,
}

#[derive(Parser)]
struct V2WriteArgs {
    /// Database base path (v2 will derive `<path>.ndb`/`<path>.wal` if needed)
    #[arg(long)]
    db: PathBuf,

    /// Cypher CREATE or DELETE query string (v2 M3)
    #[arg(long, conflicts_with = "file")]
    cypher: Option<String>,

    /// Read Cypher query from file
    #[arg(long)]
    file: Option<PathBuf>,

    /// Parameters as a JSON object (M3: supports scalar values)
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
        V2Value::List(list) => {
            let arr: Vec<serde_json::Value> =
                list.iter().map(|v| value_to_json_v2(snapshot, v)).collect();
            serde_json::Value::Array(arr)
        }
    }
}

fn parse_params_json_v2(raw: Option<String>) -> Result<nervusdb_v2_query::Params, String> {
    let Some(raw) = raw else {
        return Ok(nervusdb_v2_query::Params::new());
    };
    if raw.trim().is_empty() {
        return Ok(nervusdb_v2_query::Params::new());
    }
    let parsed: HashMap<String, serde_json::Value> = serde_json::from_str(&raw)
        .map_err(|e| format!("params_json must be a JSON object: {e}"))?;
    let mut out = nervusdb_v2_query::Params::new();
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
    let engine = GraphEngine::open(db.ndb_path(), db.wal_path()).map_err(|e| e.to_string())?;
    let graph_snap = engine.snapshot();

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
    let engine = GraphEngine::open(db.ndb_path(), db.wal_path()).map_err(|e| e.to_string())?;
    let graph_snap = engine.snapshot();

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

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::V2(args) => match args.command {
            V2Commands::Query(args) => run_v2_query(args),
            V2Commands::Write(args) => run_v2_write(args),
            V2Commands::Repl(args) => repl::run_repl(&args.db),
        },
    };

    if let Err(message) = result {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

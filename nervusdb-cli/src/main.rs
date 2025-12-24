use clap::{Parser, Subcommand, ValueEnum};
use nervusdb_core::query::executor::Value;
use nervusdb_core::query::parser::Parser as CypherParser;
use nervusdb_core::query::planner::QueryPlanner;
use nervusdb_core::{Database, Options};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "nervusdb", version, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Query(QueryArgs),
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Ndjson,
}

#[derive(Parser)]
struct QueryArgs {
    /// Database base path (core will use `<path>.redb`)
    #[arg(long)]
    db: PathBuf,

    /// Cypher query string
    #[arg(long, conflicts_with = "file")]
    cypher: Option<String>,

    /// Read Cypher query from file
    #[arg(long)]
    file: Option<PathBuf>,

    /// Parameters as a JSON object (e.g. '{\"name\":\"alice\"}')
    #[arg(long)]
    params_json: Option<String>,

    #[arg(long, value_enum, default_value = "ndjson")]
    format: OutputFormat,
}

fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Float(f) => serde_json::json!(f),
        Value::Boolean(b) => serde_json::Value::Bool(*b),
        Value::Null => serde_json::Value::Null,
        Value::Vector(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|f| {
                    serde_json::Number::from_f64(*f as f64)
                        .map(serde_json::Value::Number)
                        .unwrap_or(serde_json::Value::Null)
                })
                .collect(),
        ),
        Value::Node(id) => serde_json::json!({ "node_id": id }),
        Value::Relationship(triple) => serde_json::json!({
            "subject_id": triple.subject_id,
            "predicate_id": triple.predicate_id,
            "object_id": triple.object_id,
        }),
    }
}

fn parse_params_json(raw: Option<String>) -> Result<HashMap<String, Value>, String> {
    let Some(raw) = raw else {
        return Ok(HashMap::new());
    };
    if raw.trim().is_empty() {
        return Ok(HashMap::new());
    }
    let parsed: HashMap<String, serde_json::Value> = serde_json::from_str(&raw)
        .map_err(|e| format!("params_json must be a JSON object: {e}"))?;
    Ok(parsed
        .into_iter()
        .map(|(k, v)| (k, Database::serde_value_to_executor_value(v)))
        .collect())
}

fn read_query(args: &QueryArgs) -> Result<String, String> {
    if let Some(query) = args.cypher.as_ref() {
        return Ok(query.clone());
    }
    let Some(path) = args.file.as_ref() else {
        return Err("either --cypher or --file is required".to_string());
    };
    std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read query file {}: {e}", path.display()))
}

fn run_query(args: QueryArgs) -> Result<(), String> {
    let query = read_query(&args)?;
    let params = parse_params_json(args.params_json)?;

    #[allow(clippy::arc_with_non_send_sync)]
    let db = Arc::new(Database::open(Options::new(&args.db)).map_err(|e| e.to_string())?);
    let ast = CypherParser::parse(query.as_str()).map_err(|e| e.to_string())?;
    let plan = QueryPlanner::new().plan(ast).map_err(|e| e.to_string())?;

    #[allow(clippy::arc_with_non_send_sync)]
    let ctx = Arc::new(nervusdb_core::query::executor::ArcExecutionContext::new(
        Arc::clone(&db),
        params,
    ));
    let mut stdout = std::io::stdout().lock();

    match args.format {
        OutputFormat::Ndjson => {
            let iter = plan.execute_streaming(ctx).map_err(|e| e.to_string())?;
            for item in iter {
                let record = item.map_err(|e| e.to_string())?;
                let mut map = serde_json::Map::with_capacity(record.values.len());
                for (k, v) in &record.values {
                    map.insert(k.clone(), value_to_json(v));
                }
                serde_json::to_writer(&mut stdout, &serde_json::Value::Object(map))
                    .map_err(|e| e.to_string())?;
                stdout.write_all(b"\n").map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(())
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Query(args) => run_query(args),
    };

    if let Err(message) = result {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

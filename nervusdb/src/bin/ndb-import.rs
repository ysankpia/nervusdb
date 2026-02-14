use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use nervusdb_storage::bulkload::{BulkEdge, BulkLoader, BulkNode};
use nervusdb_storage::property::PropertyValue;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Bulk import tool for NervusDB V2
#[derive(Parser)]
#[command(name = "ndb-import")]
#[command(about = "Bulk imports data into a new NervusDB database", long_about = None)]
struct Cli {
    /// Path to the output database directory (must not exist)
    #[arg(long, short)]
    output: PathBuf,

    /// Node files to import
    #[arg(long)]
    nodes: Vec<PathBuf>,

    /// Edge files to import
    #[arg(long)]
    edges: Vec<PathBuf>,

    /// Input format
    #[arg(long, value_enum, default_value_t = Format::Csv)]
    format: Format,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Csv,
    Jsonl,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.output.exists() {
        anyhow::bail!("Output database path already exists: {:?}", cli.output);
    }

    println!("🚀 Starting bulk import into {:?}", cli.output);
    let mut loader = BulkLoader::new(cli.output.clone())?;

    // 1. Process Nodes
    for path in &cli.nodes {
        println!("  Processing nodes from {:?}", path);
        match cli.format {
            Format::Csv => process_nodes_csv(&mut loader, path)?,
            Format::Jsonl => process_nodes_jsonl(&mut loader, path)?,
        }
    }

    // 2. Process Edges
    for path in &cli.edges {
        println!("  Processing edges from {:?}", path);
        match cli.format {
            Format::Csv => process_edges_csv(&mut loader, path)?,
            Format::Jsonl => process_edges_jsonl(&mut loader, path)?,
        }
    }

    // 3. Commit
    println!("💾 Committing data to disk...");
    loader.commit()?;
    println!("✅ Import complete!");

    Ok(())
}

fn process_nodes_csv(loader: &mut BulkLoader, path: &Path) -> Result<()> {
    let file = File::open(path).context("Failed to open node file")?;
    let mut rdr = csv::Reader::from_reader(file);
    let headers = rdr.headers()?.clone();

    // Parse headers to identify special columns and property types
    let mut id_col = None;
    let mut label_col = None;
    let mut properties = Vec::new();

    for (i, header) in headers.iter().enumerate() {
        if header == "id:ID" || header == ":ID" {
            id_col = Some(i);
        } else if header == ":LABEL" {
            label_col = Some(i);
        } else {
            // Property column: "name", "age:int", "score:float"
            let (name, prop_type) = parse_header_type(header);
            properties.push((i, name, prop_type));
        }
    }

    let id_idx = id_col.context("Missing required column: id:ID or :ID")?;
    let label_idx = label_col.context("Missing required column: :LABEL")?;

    for result in rdr.records() {
        let record = result?;

        let external_id: u64 = record[id_idx].parse().context("Invalid ID format")?;
        let label = record[label_idx].to_string();

        let mut props = BTreeMap::new();
        for &(idx, ref name, ref ptype) in &properties {
            let val_str = &record[idx];
            if val_str.is_empty() {
                continue; // Skip empty/null values
            }
            let val = parse_value(val_str, ptype)?;
            props.insert(name.clone(), val);
        }

        loader.add_node(BulkNode {
            external_id,
            label,
            properties: props,
        })?;
    }

    Ok(())
}

fn process_edges_csv(loader: &mut BulkLoader, path: &Path) -> Result<()> {
    let file = File::open(path).context("Failed to open edge file")?;
    let mut rdr = csv::Reader::from_reader(file);
    let headers = rdr.headers()?.clone();

    let mut start_col = None;
    let mut end_col = None;
    let mut type_col = None;
    let mut properties = Vec::new();

    for (i, header) in headers.iter().enumerate() {
        if header == ":START_ID" {
            start_col = Some(i);
        } else if header == ":END_ID" {
            end_col = Some(i);
        } else if header == ":TYPE" {
            type_col = Some(i);
        } else {
            let (name, prop_type) = parse_header_type(header);
            properties.push((i, name, prop_type));
        }
    }

    let start_idx = start_col.context("Missing :START_ID column")?;
    let end_idx = end_col.context("Missing :END_ID column")?;
    let type_idx = type_col.context("Missing :TYPE column")?;

    for result in rdr.records() {
        let record = result?;

        let src_external_id: u64 = record[start_idx].parse().context("Invalid START_ID")?;
        let dst_external_id: u64 = record[end_idx].parse().context("Invalid END_ID")?;
        let rel_type = record[type_idx].to_string();

        let mut props = BTreeMap::new();
        for &(idx, ref name, ref ptype) in &properties {
            let val_str = &record[idx];
            if val_str.is_empty() {
                continue;
            }
            let val = parse_value(val_str, ptype)?;
            props.insert(name.clone(), val);
        }

        loader.add_edge(BulkEdge {
            src_external_id,
            rel_type,
            dst_external_id,
            properties: props,
        })?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
enum PropType {
    String,
    Int,
    Float,
    Bool,
}

fn parse_header_type(header: &str) -> (String, PropType) {
    if let Some(name) = header.strip_suffix(":int") {
        (name.to_string(), PropType::Int)
    } else if let Some(name) = header.strip_suffix(":float") {
        (name.to_string(), PropType::Float)
    } else if let Some(name) = header.strip_suffix(":bool") {
        (name.to_string(), PropType::Bool)
    } else if let Some(name) = header.strip_suffix(":string") {
        (name.to_string(), PropType::String)
    } else {
        (header.to_string(), PropType::String)
    }
}

fn parse_value(s: &str, ptype: &PropType) -> Result<PropertyValue> {
    match ptype {
        PropType::String => Ok(PropertyValue::String(s.to_string())),
        PropType::Int => {
            let v: i64 = s
                .parse()
                .with_context(|| format!("Invalid integer: {}", s))?;
            Ok(PropertyValue::Int(v))
        }
        PropType::Float => {
            let v: f64 = s.parse().with_context(|| format!("Invalid float: {}", s))?;
            Ok(PropertyValue::Float(v))
        }
        PropType::Bool => {
            let v: bool = s.parse().or_else(|_| match s.to_lowercase().as_str() {
                "true" | "t" | "yes" | "y" | "1" => Ok(true),
                "false" | "f" | "no" | "n" | "0" => Ok(false),
                _ => anyhow::bail!("Invalid boolean: {}", s),
            })?;
            Ok(PropertyValue::Bool(v))
        }
    }
}

fn process_nodes_jsonl(loader: &mut BulkLoader, path: &Path) -> Result<()> {
    let file = File::open(path).context("Failed to open node JSONL file")?;
    let reader = BufReader::new(file);

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let json: serde_json::Value = serde_json::from_str(&line)
            .with_context(|| format!("Malformed JSON at line {}", line_num + 1))?;

        let external_id = json["id"]
            .as_u64()
            .context("Missing or invalid 'id' (must be u64)")?;

        let label = json["label"]
            .as_str()
            .context("Missing or invalid 'label' (must be string)")?
            .to_string();

        let mut properties = BTreeMap::new();
        if let Some(props_obj) = json["properties"].as_object() {
            for (key, val) in props_obj {
                properties.insert(key.clone(), json_to_property_value(val)?);
            }
        }

        loader.add_node(BulkNode {
            external_id,
            label,
            properties,
        })?;
    }

    Ok(())
}

fn process_edges_jsonl(loader: &mut BulkLoader, path: &Path) -> Result<()> {
    let file = File::open(path).context("Failed to open edge JSONL file")?;
    let reader = BufReader::new(file);

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let json: serde_json::Value = serde_json::from_str(&line)
            .with_context(|| format!("Malformed JSON at line {}", line_num + 1))?;

        let src_external_id = json["src"]
            .as_u64()
            .context("Missing or invalid 'src' (must be u64)")?;

        let dst_external_id = json["dst"]
            .as_u64()
            .context("Missing or invalid 'dst' (must be u64)")?;

        let rel_type = json["type"]
            .as_str()
            .context("Missing or invalid 'type' (must be string)")?
            .to_string();

        let mut properties = BTreeMap::new();
        if let Some(props_obj) = json["properties"].as_object() {
            for (key, val) in props_obj {
                properties.insert(key.clone(), json_to_property_value(val)?);
            }
        }

        loader.add_edge(BulkEdge {
            src_external_id,
            rel_type,
            dst_external_id,
            properties,
        })?;
    }

    Ok(())
}

fn json_to_property_value(v: &serde_json::Value) -> Result<PropertyValue> {
    match v {
        serde_json::Value::String(s) => Ok(PropertyValue::String(s.clone())),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(PropertyValue::Int(i))
            } else if let Some(f) = n.as_f64() {
                Ok(PropertyValue::Float(f))
            } else {
                anyhow::bail!("Unsupported number format in JSON: {}", n)
            }
        }
        serde_json::Value::Bool(b) => Ok(PropertyValue::Bool(*b)),
        serde_json::Value::Null => anyhow::bail!("Null properties are not supported"),
        _ => anyhow::bail!(
            "Complex JSON types (objects/arrays) are not supported as property values yet"
        ),
    }
}

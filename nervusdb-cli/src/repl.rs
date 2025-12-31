use nervusdb_v2::Db;
use nervusdb_v2_api::GraphStore;
use nervusdb_v2_query::Value;
use nervusdb_v2_query::prepare;
use nervusdb_v2_storage::engine::GraphEngine;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

fn parse_param_value(input: &str) -> Result<Value, String> {
    let s = input.trim();
    if s.eq_ignore_ascii_case("null") {
        return Ok(Value::Null);
    }
    if s.eq_ignore_ascii_case("true") {
        return Ok(Value::Bool(true));
    }
    if s.eq_ignore_ascii_case("false") {
        return Ok(Value::Bool(false));
    }

    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        return Ok(Value::String(s[1..s.len() - 1].to_string()));
    }

    if let Ok(i) = s.parse::<i64>() {
        return Ok(Value::Int(i));
    }
    if let Ok(f) = s.parse::<f64>() {
        return Ok(Value::Float(f));
    }

    Err("Unsupported param value (use null/true/false/int/float/'string')".to_string())
}

pub fn run_repl(db_path: &Path) -> Result<(), String> {
    println!("💊 NervusDB REPL v{}", env!("CARGO_PKG_VERSION"));
    println!("Type .help for instructions, .exit to quit.\n");

    let db = Db::open(db_path).map_err(|e| e.to_string())?;
    // We open the engine directly to get a snapshot factory
    let engine = GraphEngine::open(db.ndb_path(), db.wal_path()).map_err(|e| e.to_string())?;

    let mut rl = DefaultEditor::new().map_err(|e| e.to_string())?;

    let history_path = db_path.with_extension("repl_history");
    let _ = rl.load_history(&history_path);

    let mut param_store: BTreeMap<String, Value> = BTreeMap::new();

    loop {
        let readline = rl.readline("nervusdb> ");
        match readline {
            Ok(line) => {
                let line = line.trim();
                let _ = rl.add_history_entry(line);

                if line.is_empty() {
                    continue;
                }

                if line.starts_with('.') {
                    if let Some(rest) = line.strip_prefix(".param") {
                        let rest = rest.trim();
                        let mut it = rest.splitn(2, char::is_whitespace);
                        let Some(key) = it.next().map(str::trim).filter(|s| !s.is_empty()) else {
                            println!("Usage: .param <name> <value>");
                            continue;
                        };
                        let Some(raw) = it.next().map(str::trim).filter(|s| !s.is_empty()) else {
                            println!("Usage: .param <name> <value>");
                            continue;
                        };
                        match parse_param_value(raw) {
                            Ok(v) => {
                                param_store.insert(key.to_string(), v);
                                println!("OK");
                            }
                            Err(e) => println!("Error: {e}"),
                        }
                        continue;
                    }

                    if let Some(rest) = line.strip_prefix(".unparam") {
                        let key = rest.trim();
                        if key.is_empty() {
                            println!("Usage: .unparam <name>");
                            continue;
                        }
                        param_store.remove(key);
                        println!("OK");
                        continue;
                    }

                    match line {
                        ".exit" | ".quit" => {
                            let _ = rl.save_history(&history_path);
                            println!("Bye!");
                            break;
                        }
                        ".help" => {
                            println!("Commands:");
                            println!("  .exit, .quit  Exit the REPL");
                            println!("  .help         Show this help message");
                            println!("  .param k v    Set query parameter ($k)");
                            println!("  .unparam k    Remove query parameter");
                            println!("  .params       Show all parameters");
                            println!("  .clearparams  Clear all parameters");
                            println!("  <cypher>      Execute a Cypher query");
                        }
                        ".params" => {
                            if param_store.is_empty() {
                                println!("(no params)");
                            } else {
                                for (k, v) in &param_store {
                                    println!("{k} = {v:?}");
                                }
                            }
                        }
                        ".clearparams" => {
                            param_store.clear();
                            println!("OK");
                        }
                        _ => println!("Unknown command: {}", line),
                    }
                    continue;
                }

                // Execute Query
                let start = Instant::now();
                let snapshot = engine.snapshot();

                match prepare(line) {
                    Ok(query) => {
                        // For MVP REPL we just dump as JSON-like lines or a simple table
                        // Reusing the logic from main.rs basically, but simpler output for now
                        let mut params = nervusdb_v2_query::Params::new();
                        for (k, v) in &param_store {
                            params.insert(k.clone(), v.clone());
                        }

                        let iter = query.execute_streaming(&snapshot, &params);
                        let mut count = 0;

                        for row in iter {
                            match row {
                                Ok(r) => {
                                    count += 1;
                                    // Simple debug print for row content
                                    println!("{:?}", r.columns());
                                }
                                Err(e) => println!("Error executing row: {}", e),
                            }
                        }

                        let duration = start.elapsed();
                        println!("({} rows, {:.4}s)", count, duration.as_secs_f64());
                    }
                    Err(e) => println!("Error preparing query: {}", e),
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }
            Err(ReadlineError::Eof) => {
                let _ = rl.save_history(&history_path);
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    Ok(())
}

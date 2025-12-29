use nervusdb_v2::Db;
use nervusdb_v2_api::GraphStore;
use nervusdb_v2_query::prepare;
use nervusdb_v2_storage::engine::GraphEngine;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::path::Path;
use std::time::Instant;

pub fn run_repl(db_path: &Path) -> Result<(), String> {
    println!("💊 NervusDB REPL v{}", env!("CARGO_PKG_VERSION"));
    println!("Type .help for instructions, .exit to quit.\n");

    let db = Db::open(db_path).map_err(|e| e.to_string())?;
    // We open the engine directly to get a snapshot factory
    let engine = GraphEngine::open(db.ndb_path(), db.wal_path()).map_err(|e| e.to_string())?;

    let mut rl = DefaultEditor::new().map_err(|e| e.to_string())?;

    // TODO: Load history if we want to persist it
    // if rl.load_history("history.txt").is_err() {
    //     println!("No previous history.");
    // }

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
                    match line {
                        ".exit" | ".quit" => {
                            println!("Bye!");
                            break;
                        }
                        ".help" => {
                            println!("Commands:");
                            println!("  .exit, .quit  Exit the REPL");
                            println!("  .help         Show this help message");
                            println!("  <cypher>      Execute a Cypher query");
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
                        let params = nervusdb_v2_query::Params::new(); // TODO: Interactive params?

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

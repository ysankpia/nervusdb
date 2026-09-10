use graphlite::{GraphError, GraphLite};
use std::env;
use std::io::{self, BufRead, Write};
use std::time::Instant;

fn print_ascii_table(columns: &[String], rows: &[Vec<String>]) {
    if columns.is_empty() {
        return;
    }

    let col_count = columns.len();
    let mut widths = vec![0; col_count];

    for (i, col) in columns.iter().enumerate() {
        widths[i] = widths[i].max(col.len());
    }

    for row in rows {
        for (i, val) in row.iter().enumerate() {
            if i < col_count {
                widths[i] = widths[i].max(val.len());
            }
        }
    }

    // 打印边框分隔线
    let print_separator = |w: &[usize]| {
        print!("+");
        for width in w {
            print!("-{}-+", "-".repeat(*width));
        }
        println!();
    };

    print_separator(&widths);

    // 打印表头
    print!("|");
    for (i, col) in columns.iter().enumerate() {
        print!(" {:<width$} |", col, width = widths[i]);
    }
    println!();

    print_separator(&widths);

    // 打印数据行
    for row in rows {
        print!("|");
        for (i, width) in widths.iter().enumerate() {
            let val = row.get(i).map(|s| s.as_str()).unwrap_or("");
            print!(" {:<width$} |", val, width = *width);
        }
        println!();
    }

    print_separator(&widths);
}

fn main() -> Result<(), GraphError> {
    let args: Vec<String> = env::args().collect();
    let db_path = if args.len() > 1 {
        &args[1]
    } else {
        "graphlite.db"
    };

    println!("============================================================");
    println!("       GraphLite-RS Interactive Shell (SQLite 3.0 Edition)  ");
    println!("       Connected to: {}", db_path);
    println!("       Enter '.help' for usage hints. Terminate queries with ';'.");
    println!("============================================================");

    let db = GraphLite::open(db_path)?;

    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut input_buffer = String::new();

    loop {
        if input_buffer.trim().is_empty() {
            print!("graphlite> ");
        } else {
            print!("   ...> ");
        }
        io::stdout().flush().unwrap();

        let mut line = String::new();
        if reader.read_line(&mut line).unwrap() == 0 {
            // EOF
            println!("\nBye!");
            break;
        }

        let trimmed = line.trim();

        // 检查点命令
        if input_buffer.trim().is_empty() && trimmed.starts_with('.') {
            match trimmed {
                ".quit" | ".exit" => {
                    println!("Bye!");
                    break;
                }
                ".help" => {
                    println!("\nAvailable Dot Commands:");
                    println!("  .schema       Show graph labels, edge types, indices, and counts");
                    println!(
                        "  .stats        Show Buffer Pool hit rate, disk I/O, and page metrics"
                    );
                    println!(
                        "  .checkpoint   Trigger checkpoint flush to single file and truncate WAL"
                    );
                    println!("  .quit / .exit Exit this interactive shell\n");
                }
                ".schema" => {
                    println!("\n--- Database Schema & Statistics ---");
                    println!("  Total Nodes:       {}", db.node_count());
                    println!("  Total Edges:       {}", db.edge_count());
                    println!("  Indexed Labels:    {:?}", db.index_labels());
                    println!("  Property Indices:  {:?}", db.index_properties());
                    println!("  Database File:     {:?}\n", db.db_path());
                }
                ".stats" => {
                    let stats = db.buffer_stats();
                    println!("\n--- 4KB Buffer Pool Metrics & Disk Stats ---");
                    println!(
                        "  Pool Capacity:     {} frames ({} KB)",
                        stats.capacity_frames,
                        stats.capacity_frames * 4
                    );
                    println!("  Used Frames:       {}", stats.used_frames);
                    println!("  Dirty Frames:      {}", stats.dirty_frames);
                    println!("  Cache Hits:        {}", stats.cache_hits);
                    println!("  Cache Misses:      {}", stats.cache_misses);
                    println!("  Cache Hit Rate:    {:.2}%", stats.hit_rate_percentage);
                    println!("  Physical Reads:    {} pages", stats.disk_reads);
                    println!("  Physical Writes:   {} pages", stats.disk_writes);
                    println!(
                        "  File Disk Size:    {} bytes ({:.2} KB)\n",
                        stats.file_size_bytes,
                        stats.file_size_bytes as f64 / 1024.0
                    );
                }
                ".checkpoint" => {
                    let start = Instant::now();
                    db.checkpoint()?;
                    println!("Checkpoint completed in {:.2?}\n", start.elapsed());
                }
                _ => {
                    println!("Unknown dot command: '{}'. Type '.help' for help.", trimmed);
                }
            }
            continue;
        }

        input_buffer.push_str(&line);

        // 如果包含分号，执行完整语句
        if input_buffer.contains(';') {
            let full_query = input_buffer.trim().to_string();
            input_buffer.clear();

            let query_without_semicolon = full_query.trim_end_matches(';').trim();
            if query_without_semicolon.is_empty() {
                continue;
            }

            let start = Instant::now();
            let is_match = query_without_semicolon.to_uppercase().starts_with("MATCH");

            if is_match {
                match db.query_cypher(query_without_semicolon) {
                    Ok(result_set) => {
                        let rows_str: Vec<Vec<String>> = result_set
                            .rows
                            .iter()
                            .map(|r| r.values.iter().map(|v| v.to_string()).collect())
                            .collect();

                        print_ascii_table(&result_set.columns, &rows_str);
                        println!(
                            "{} row(s) in set ({:.2?})\n",
                            result_set.row_count(),
                            start.elapsed()
                        );
                    }
                    Err(e) => {
                        eprintln!("Error executing query: {}\n", e);
                    }
                }
            } else {
                match db.execute(query_without_semicolon) {
                    Ok(stats) => {
                        println!("Query OK, {} ({:.2?})\n", stats.message, start.elapsed());
                    }
                    Err(e) => {
                        eprintln!("Error executing statement: {}\n", e);
                    }
                }
            }
        }
    }

    Ok(())
}

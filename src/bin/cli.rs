//! GraphLite-RS 交互式终端 (graphlite-cli)
//!
//! 媲美 sqlite3 的单文件图数据库命令行工具：
//! - 多行输入，以分号 `;` 终结一条语句（引号内的分号不终结）；
//! - 整齐对齐的 ASCII 表格渲染与执行耗时统计；
//! - 内置点命令：`.schema` / `.stats` / `.checkpoint` / `.dump <file>` / `.history` / `.help` / `.quit`；
//! - 通过 `GRAPH_LITE_HISTORY` 环境变量指定历史记录文件（默认 `~/.graphlite_history`）。

use graphlite::{GraphError, GraphLite, Value};
use std::env;
use std::fs::OpenOptions;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::Instant;

/// 渲染整齐对齐的 ASCII 表格
fn print_ascii_table(columns: &[String], rows: &[Vec<String>]) {
    if columns.is_empty() {
        return;
    }

    let col_count = columns.len();
    let mut widths = vec![0usize; col_count];

    for (i, col) in columns.iter().enumerate() {
        widths[i] = widths[i].max(display_width(col));
    }

    for row in rows {
        for (i, val) in row.iter().enumerate() {
            if i < col_count {
                widths[i] = widths[i].max(display_width(val));
            }
        }
    }

    let print_separator = |w: &[usize]| {
        let mut line = String::from("+");
        for width in w {
            line.push_str(&"-".repeat(width + 2));
            line.push('+');
        }
        println!("{}", line);
    };

    print_separator(&widths);

    let mut header = String::from("|");
    for (i, col) in columns.iter().enumerate() {
        header.push_str(&format!(" {} |", pad_right(col, widths[i])));
    }
    println!("{}", header);

    print_separator(&widths);

    for row in rows {
        let mut line = String::from("|");
        for (i, width) in widths.iter().enumerate() {
            let val = row.get(i).map(|s| s.as_str()).unwrap_or("");
            line.push_str(&format!(" {} |", pad_right(val, *width)));
        }
        println!("{}", line);
    }

    print_separator(&widths);
}

/// 以字符数（而非字节数）衡量显示宽度，保证中文等多字节内容对齐全
fn display_width(s: &str) -> usize {
    s.chars().count()
}

fn pad_right(s: &str, width: usize) -> String {
    let len = display_width(s);
    if len >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - len))
    }
}

/// 渲染单个值（字符串加引号，与 Cypher 字面量风格一致）
fn render_value(value: &Value) -> String {
    match value {
        Value::String(s) => format!("'{}'", s),
        other => other.to_string(),
    }
}

/// 判定缓冲区内容是否已构成完整语句：分号必须出现在引号之外
fn is_statement_complete(buffer: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for ch in buffer.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_single || in_double => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            ';' if !in_single && !in_double => return true,
            _ => {}
        }
    }

    false
}

fn history_path() -> PathBuf {
    if let Ok(path) = env::var("GRAPH_LITE_HISTORY") {
        return PathBuf::from(path);
    }
    match env::var("HOME") {
        Ok(home) => PathBuf::from(home).join(".graphlite_history"),
        Err(_) => PathBuf::from(".graphlite_history"),
    }
}

fn append_history(input: &str) {
    if input.trim().is_empty() {
        return;
    }
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(history_path())
    {
        let single_line = input.replace('\n', " ");
        let _ = writeln!(file, "{}", single_line.trim());
    }
}

fn print_help() {
    println!();
    println!("GraphLite-RS Interactive Shell Commands:");
    println!("  <cypher>;              Execute a Cypher statement (terminate with ';')");
    println!("                         Statements may span multiple lines.");
    println!();
    println!("Available Dot Commands:");
    println!("  .schema                Show labels, edge types, indices and entity counts");
    println!("  .stats                 Show Buffer Pool hit rate, disk I/O, WAL and file size");
    println!("  .checkpoint            Flush WAL to the main file and truncate the WAL");
    println!("  .dump <file>           Export the graph as a replayable Cypher script");
    println!("  .history               Show recent command history");
    println!("  .help                  Show this help message");
    println!("  .quit / .exit          Exit this interactive shell");
    println!();
}

fn cmd_schema(db: &GraphLite) {
    println!();
    println!("--- Graph Schema & Statistics ---");
    let labels = db.index_labels();
    let edge_types = db.edge_types();
    let props = db.index_properties();

    println!("  Nodes:              {}", db.node_count());
    println!("  Edges:              {}", db.edge_count());
    println!(
        "  Labels ({}):         {}",
        labels.len(),
        if labels.is_empty() {
            "-".to_string()
        } else {
            labels.join(", ")
        }
    );
    println!(
        "  Edge Types ({}):     {}",
        edge_types.len(),
        if edge_types.is_empty() {
            "-".to_string()
        } else {
            edge_types.join(", ")
        }
    );
    if props.is_empty() {
        println!("  Property Indices:   -");
    } else {
        println!("  Property Indices ({}):", props.len());
        for (label, key) in &props {
            println!("    - ({}).{}", label, key);
        }
    }
    println!("  Database File:      {}", db.db_path().display());
    println!();
}

fn cmd_stats(db: &GraphLite) {
    let stats = db.buffer_stats();
    println!();
    println!("--- 4KB Buffer Pool Metrics & Disk Stats ---");
    println!(
        "  Pool Capacity:      {} frames ({} KB)",
        stats.capacity_frames,
        stats.capacity_frames * 4
    );
    println!("  Used Frames:        {}", stats.used_frames);
    println!("  Dirty Frames:       {}", stats.dirty_frames);
    println!("  Cache Hits:         {}", stats.cache_hits);
    println!("  Cache Misses:       {}", stats.cache_misses);
    println!("  Cache Hit Rate:     {:.2}%", stats.hit_rate_percentage);
    println!("  Physical Reads:     {} pages", stats.disk_reads);
    println!("  Physical Writes:    {} pages", stats.disk_writes);
    println!(
        "  Main File Size:     {} bytes ({:.2} KB)",
        stats.file_size_bytes,
        stats.file_size_bytes as f64 / 1024.0
    );
    println!(
        "  WAL Size:           {} bytes ({:.2} KB)",
        stats.wal_size_bytes,
        stats.wal_size_bytes as f64 / 1024.0
    );
    println!("  WAL Resident Pages: {}", stats.wal_page_count);
    println!("  Spill (STEAL) Ops:  {}", stats.spill_count);
    println!("  WAL fsync Count:    {}", stats.wal_fsync_count);
    println!("  WAL Frames Written: {}", stats.wal_frames_written);
    println!();
}

fn cmd_dump(db: &GraphLite, args: &str) {
    let target = args.trim();
    if target.is_empty() {
        println!("Usage: .dump <file>    (use '-' to write to stdout)");
        return;
    }

    if target == "-" {
        let stdout = io::stdout();
        let mut lock = stdout.lock();
        match db.dump_cypher(&mut lock) {
            Ok(()) => {
                let _ = lock.flush();
                println!("-- Dump complete.");
            }
            Err(e) => eprintln!("Dump failed: {}", e),
        }
        return;
    }

    match std::fs::File::create(target) {
        Ok(mut file) => match db.dump_cypher(&mut file) {
            Ok(()) => match file.flush() {
                Ok(()) => println!(
                    "Dumped {} nodes and {} edges to '{}'.",
                    db.node_count(),
                    db.edge_count(),
                    target
                ),
                Err(e) => eprintln!("Failed to flush '{}': {}", target, e),
            },
            Err(e) => eprintln!("Dump failed: {}", e),
        },
        Err(e) => eprintln!("Cannot create '{}': {}", target, e),
    }
}

fn cmd_history() {
    match std::fs::read_to_string(history_path()) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(20);
            println!();
            for (idx, line) in lines[start..].iter().enumerate() {
                println!("  {:>3}  {}", start + idx + 1, line);
            }
            println!();
        }
        Err(_) => println!("No history recorded yet."),
    }
}

fn main() -> Result<(), GraphError> {
    let args: Vec<String> = env::args().collect();
    let db_path = if args.len() > 1 {
        args[1].as_str()
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
    let mut buffer = String::new();

    loop {
        if buffer.trim().is_empty() {
            print!("graphlite> ");
        } else {
            print!("    ...> ");
        }
        io::stdout().flush()?;

        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            println!();
            break;
        }

        // 缓冲区为空时的点命令处理（不进入多行语句缓冲）
        if buffer.trim().is_empty() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix('.') {
                append_history(trimmed);
                let mut parts = rest.splitn(2, char::is_whitespace);
                let command = parts.next().unwrap_or("");
                let argument = parts.next().unwrap_or("");

                match command {
                    "quit" | "exit" => break,
                    "help" => print_help(),
                    "schema" => cmd_schema(&db),
                    "stats" => cmd_stats(&db),
                    "checkpoint" => {
                        let start = Instant::now();
                        db.checkpoint()?;
                        println!("Checkpoint completed in {:.2?}", start.elapsed());
                    }
                    "dump" => cmd_dump(&db, argument),
                    "history" => cmd_history(),
                    other => println!(
                        "Unknown dot command: '.{}'. Type '.help' for available commands.",
                        other
                    ),
                }
                continue;
            }
        }

        buffer.push_str(&line);

        if is_statement_complete(&buffer) {
            let statement = buffer.trim().to_string();
            buffer.clear();
            append_history(&statement);

            let query = statement.trim_end_matches(';').trim();
            if query.is_empty() {
                continue;
            }

            let start = Instant::now();
            match db.run_cypher(query) {
                Ok(result_set) => {
                    // 只读查询渲染结果表格；纯写语句展示执行摘要
                    let is_status_only =
                        result_set.columns.len() == 1 && result_set.columns[0] == "Status";
                    if !result_set.columns.is_empty() && !is_status_only {
                        let rows_str: Vec<Vec<String>> = result_set
                            .rows
                            .iter()
                            .map(|r| r.values.iter().map(render_value).collect())
                            .collect();
                        print_ascii_table(&result_set.columns, &rows_str);
                        println!(
                            "{} row(s) in set ({:.2?})",
                            result_set.row_count(),
                            start.elapsed()
                        );
                    } else {
                        println!(
                            "Query OK, {} ({:.2?})",
                            result_set.stats.message,
                            start.elapsed()
                        );
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }
    }

    db.checkpoint()?;
    println!("Bye!");
    Ok(())
}

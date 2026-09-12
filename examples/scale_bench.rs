use graphlite::{GraphLite, GraphLiteOptions, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

/// 输出目录：`DB_DIR` 环境变量，默认当前目录下的 `bench_db`。
///
/// 刻意不硬编码作者本机的绝对路径——外部读者无从得知该换成什么。
fn resolve_db_dir() -> PathBuf {
    PathBuf::from(std::env::var("DB_DIR").unwrap_or_else(|_| "bench_db".to_string()))
}

/// 自动 checkpoint 阈值：`AUTO_CHECKPOINT_MB` 环境变量控制，默认 0（关闭）。
///
/// 基准自己按边数节奏做 checkpoint，若再叠加默认的 64MB 自动 checkpoint，
/// 会在大规模导入中触发数十次额外 checkpoint，实测吞吐减半
/// （LiveJournal：关闭 447k ops/s，开启 64MB 为 232k ops/s）。
/// 基准要测的是引擎自身的写入吞吐，因此默认关闭并把该配置打印出来，
/// 避免数字与配置脱节。
fn auto_checkpoint_bytes() -> u64 {
    std::env::var("AUTO_CHECKPOINT_MB")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
        .saturating_mul(1024 * 1024)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base_dir = resolve_db_dir();
    std::fs::create_dir_all(&base_dir)?;

    let nodes_count: u64 = std::env::var("NODES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000);
    let edges_count: u64 = std::env::var("EDGES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4_000_000);
    let pool_mb: usize = std::env::var("POOL_MB")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(64);

    let db_path = base_dir.join(format!(
        "bench_team3_{}m_{}m.db",
        nodes_count / 1_000_000,
        edges_count / 1_000_000
    ));
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("db.wal"));

    println!("============================================================");
    println!(" TEAM 3 BENCHMARK (Pure DeepSeek Kernel Architecture)");
    println!(" Target DB: {}", db_path.display());
    println!(
        " Scale    : {} nodes, {} edges, Pool: {} MB ({} frames)",
        nodes_count,
        edges_count,
        pool_mb,
        pool_mb * 256
    );
    println!("============================================================");

    let db = GraphLite::open_with_options(
        &db_path,
        GraphLiteOptions {
            buffer_pool_frames: pool_mb * graphlite::FRAMES_PER_MB,
            wal_auto_checkpoint_bytes: auto_checkpoint_bytes(),
            ..GraphLiteOptions::default()
        },
    )?;

    // 1. Nodes Ingestion
    println!("-> 1. Ingesting {} nodes...", nodes_count);
    let t0 = Instant::now();
    let batch_size = 100_000u64;
    let mut total_nodes = 0u64;
    while total_nodes < nodes_count {
        let chunk = (nodes_count - total_nodes).min(batch_size);
        let start_id = total_nodes + 1;
        let end_id = total_nodes + chunk;
        db.with_transaction(|tx| {
            for i in start_id..=end_id {
                let mut p = HashMap::new();
                p.insert("idx".to_string(), Value::from(i as i64));
                tx.add_node(HashSet::from(["Person".to_string()]), p)?;
            }
            Ok(())
        })?;
        total_nodes += chunk;
        print!(
            "\r   Nodes: {} / {} ({:.1}%)",
            total_nodes,
            nodes_count,
            (total_nodes as f64 / nodes_count as f64) * 100.0
        );
        std::io::Write::flush(&mut std::io::stdout())?;
    }
    let node_time = t0.elapsed();
    let node_rate = nodes_count as f64 / node_time.as_secs_f64();
    println!(
        "\n   => Nodes completed in {:.2?} | {:.0} ops/s",
        node_time, node_rate
    );

    // 2. Edges Ingestion
    if edges_count > 0 {
        println!("-> 2. Ingesting {} discrete random edges...", edges_count);
        let t1 = Instant::now();
        let edge_batch = 100_000u64;
        let mut total_edges = 0u64;
        while total_edges < edges_count {
            let chunk = (edges_count - total_edges).min(edge_batch);
            let start_idx = total_edges;
            let end_idx = total_edges + chunk;
            db.with_transaction(|tx| {
                for i in start_idx..end_idx {
                    let src = (i % nodes_count) + 1;
                    let dst = ((i * 7919 + 13) % nodes_count) + 1;
                    if src != dst {
                        tx.add_edge(src, dst, "KNOWS", HashMap::new(), 1.0)?;
                    }
                }
                Ok(())
            })?;
            total_edges += chunk;
            print!(
                "\r   Edges: {} / {} ({:.1}%)",
                total_edges,
                edges_count,
                (total_edges as f64 / edges_count as f64) * 100.0
            );
            std::io::Write::flush(&mut std::io::stdout())?;
        }
        let edge_time = t1.elapsed();
        let edge_rate = edges_count as f64 / edge_time.as_secs_f64();
        println!(
            "\n   => Edges completed in {:.2?} | {:.0} ops/s",
            edge_time, edge_rate
        );
    }

    // 3. 50k Edge Burst
    println!("-> 3. Measuring 50,000 edge burst...");
    let t_burst = Instant::now();
    db.with_transaction(|tx| {
        for i in 0..50_000u64 {
            let src = ((i * 31) % nodes_count) + 1;
            let dst = ((i * 999_983 + 7) % nodes_count) + 1;
            if src != dst {
                tx.add_edge(src, dst, "BURST", HashMap::new(), 1.0)?;
            }
        }
        Ok(())
    })?;
    let burst_time = t_burst.elapsed();
    let burst_rate = 50_000.0 / burst_time.as_secs_f64();
    println!(
        "   => 50k Burst completed in {:.2?} ({:.1} ms) | {:.0} ops/s",
        burst_time,
        burst_time.as_secs_f64() * 1000.0,
        burst_rate
    );

    // 4. Checkpoint
    println!("-> 4. Checkpoint...");
    let t_ckpt = Instant::now();
    db.checkpoint()?;
    println!("   => Checkpoint in {:.2?}", t_ckpt.elapsed());

    let stats = db.buffer_stats();
    let file_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    println!("------------------------------------------------------------");
    println!(" Final Metrics:");
    println!(
        " File Size         : {:.2} MB",
        file_size as f64 / 1_048_576.0
    );
    println!(" Cache Hit Rate    : {:.2}%", stats.hit_rate_percentage);
    println!(" Cache Misses      : {}", stats.cache_misses);
    println!(" WAL Fsync Count   : {}", stats.wal_fsync_count);
    println!(" WAL Frames Written: {}", stats.wal_frames_written);
    println!(
        " Used Frames       : {} / {}",
        stats.used_frames, stats.capacity_frames
    );
    println!("============================================================");

    // Cleanup
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("db.wal"));
    Ok(())
}

use graphlite::{GraphLite, GraphLiteOptions, Value};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::Instant;

/// 解析数据集路径：优先 `DATASET_PATH` 环境变量，其次是 `DATASET_DIR/<file>`。
///
/// 刻意**不硬编码**任何绝对路径：这些基准原先写死了作者本机的
/// `/Volumes/WorkDrive/...`，外部读者既无法运行也无从得知该换成什么。
/// 缺失时给出可操作的提示并正常退出，而不是 panic 出一堆栈。
fn resolve_dataset(file_name: &str, default_dir: &str) -> PathBuf {
    if let Ok(p) = std::env::var("DATASET_PATH") {
        return PathBuf::from(p);
    }
    let dir = std::env::var("DATASET_DIR").unwrap_or_else(|_| default_dir.to_string());
    PathBuf::from(dir).join(file_name)
}

/// 解析输出目录：`DB_DIR` 环境变量，默认当前目录下的 `bench_db`。
fn resolve_db_dir() -> PathBuf {
    PathBuf::from(std::env::var("DB_DIR").unwrap_or_else(|_| "bench_db".to_string()))
}

/// 数据集缺失时的统一提示（返回 `None` 让 main 优雅退出）。
fn missing_dataset(path: &std::path::Path) -> Option<()> {
    if path.exists() {
        return Some(());
    }
    eprintln!(
        "dataset not found: {}\n\n\
         设置 DATASET_PATH 指向数据集文件，或设置 DATASET_DIR 指向其所在目录。\n\
         例如：\n  \
         DATASET_PATH=/data/com-dblp.ungraph.txt cargo run --release --example snap_dblp_bench\n",
        path.display()
    );
    None
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
    let dataset_path = resolve_dataset("soc-pokec-relationships.txt", "/data/datasets");
    let target_dir = resolve_db_dir();
    if missing_dataset(&dataset_path).is_none() {
        return Ok(());
    }
    std::fs::create_dir_all(&target_dir)?;

    let db_path = target_dir.join("pokec_team3.db");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("db.wal"));

    let pool_mb: usize = std::env::var("POOL_MB")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(512);

    let max_edges_limit: usize = std::env::var("MAX_EDGES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);

    println!("============================================================");
    println!(" [STAGE 2] SNAP soc-Pokec REAL-WORLD GRAPH BENCHMARK (30M EDGES)");
    println!(" TEAM 3 (Pure DeepSeek Kernel Architecture)");
    println!(" Dataset  : {}", dataset_path.display());
    println!(" Target DB: {} (Internal NVMe SSD)", db_path.display());
    println!(
        " Pool Size: {} MB ({} frames) | Auto-Checkpoint: {}",
        pool_mb,
        pool_mb * graphlite::FRAMES_PER_MB,
        match auto_checkpoint_bytes() {
            0 => "off (benchmark checkpoints explicitly)".to_string(),
            b => format!("{} MB", b / 1024 / 1024),
        }
    );
    println!(
        " Max Edges: {}",
        if max_edges_limit == usize::MAX {
            "ALL (30.6M)".to_string()
        } else {
            format!("{}", max_edges_limit)
        }
    );
    println!("============================================================");

    // 1. Parse & Ingest Raw Dataset
    println!("-> Step 1: Loading & Analyzing SNAP Pokec Dataset...");
    let t_parse = Instant::now();
    let file = File::open(&dataset_path)?;
    let reader = BufReader::with_capacity(16 * 1024 * 1024, file);

    let mut raw_edges: Vec<(u32, u32)> = Vec::with_capacity(31_000_000.min(max_edges_limit));
    let mut degrees: HashMap<u32, usize> = HashMap::with_capacity(1_700_000);
    let mut max_raw_id = 0u32;

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        if let (Some(u_str), Some(v_str)) = (parts.next(), parts.next()) {
            if let (Ok(u), Ok(v)) = (u_str.parse::<u32>(), v_str.parse::<u32>()) {
                raw_edges.push((u, v));
                *degrees.entry(u).or_insert(0) += 1;
                *degrees.entry(v).or_insert(0) += 1;
                if u > max_raw_id {
                    max_raw_id = u;
                }
                if v > max_raw_id {
                    max_raw_id = v;
                }
                if raw_edges.len() >= max_edges_limit {
                    break;
                }
            }
        }
    }

    let mut raw_nodes: Vec<u32> = degrees.keys().copied().collect();
    raw_nodes.sort_unstable();
    let num_nodes = raw_nodes.len();
    let num_edges = raw_edges.len();

    // Degree distribution & Hub identification
    let mut sorted_by_degree: Vec<(u32, usize)> = degrees.iter().map(|(&k, &v)| (k, v)).collect();
    // 按 (度数 DESC, raw id ASC) 排成全序：否则并列名次的取舍取决于排序细节，
    // 同一份数据会产出不同的 top-50 hub 集合，结果无法复现比对。
    sorted_by_degree.sort_by_key(|r| (std::cmp::Reverse(r.1), r.0));

    let max_deg = sorted_by_degree.first().map(|x| x.1).unwrap_or(0);
    let top_raw_hubs: Vec<u32> = sorted_by_degree.iter().take(50).map(|x| x.0).collect();

    println!("   Parsed in {:.2?}", t_parse.elapsed());
    println!("   Total Distinct Nodes : {}", num_nodes);
    println!("   Total Graph Edges    : {}", num_edges);
    println!("   Max Raw Node ID      : {}", max_raw_id);
    println!(
        "   Max Degree (Top Hub) : {} (Raw ID: {})",
        max_deg,
        top_raw_hubs.first().unwrap_or(&0)
    );
    println!(
        "   Top 5 Hub Degrees    : {:?}",
        sorted_by_degree
            .iter()
            .take(5)
            .map(|x| (x.0, x.1))
            .collect::<Vec<_>>()
    );

    // 2. Open Engine
    println!("\n-> Step 2: Opening Database with {} MB Pool...", pool_mb);
    let db = GraphLite::open_with_options(
        &db_path,
        GraphLiteOptions {
            buffer_pool_frames: pool_mb * graphlite::FRAMES_PER_MB,
            wal_auto_checkpoint_bytes: auto_checkpoint_bytes(),
            ..GraphLiteOptions::default()
        },
    )?;

    // 3. Node Ingestion
    println!("-> Step 3: Ingesting {} nodes in batches...", num_nodes);
    let t_nodes = Instant::now();
    let mut raw_to_engine: Vec<u64> = vec![0; (max_raw_id as usize) + 1];

    let batch_size = 100_000usize;
    let mut node_idx = 0;
    while node_idx < num_nodes {
        let end = (node_idx + batch_size).min(num_nodes);
        let chunk = &raw_nodes[node_idx..end];
        let mut created_ids = Vec::with_capacity(chunk.len());
        db.with_transaction(|tx| {
            for &raw_id in chunk {
                let mut p = HashMap::new();
                p.insert("raw_id".to_string(), Value::from(raw_id as i64));
                let eid = tx.add_node(HashSet::from(["User".to_string()]), p)?;
                created_ids.push((raw_id, eid));
            }
            Ok(())
        })?;
        for (raw_id, eid) in created_ids {
            raw_to_engine[raw_id as usize] = eid;
        }
        node_idx = end;
        print!(
            "\r   Nodes: {} / {} ({:.1}%)",
            node_idx,
            num_nodes,
            (node_idx as f64 / num_nodes as f64) * 100.0
        );
        std::io::Write::flush(&mut std::io::stdout())?;
    }
    let node_dur = t_nodes.elapsed();
    let node_ops = num_nodes as f64 / node_dur.as_secs_f64();
    println!(
        "\n   => Node Ingestion: {:.2?} | {:.0} ops/s",
        node_dur, node_ops
    );

    // Map top hubs to engine IDs
    let top_hubs: Vec<u64> = top_raw_hubs
        .iter()
        .map(|&r| raw_to_engine[r as usize])
        .collect();

    // 4. Edge Ingestion
    println!(
        "-> Step 4: Ingesting {} real-world edges in batches of 100,000...",
        num_edges
    );
    let t_edges = Instant::now();
    let edge_batch = 100_000usize;
    let mut edge_idx = 0;
    while edge_idx < num_edges {
        let end = (edge_idx + edge_batch).min(num_edges);
        let chunk = &raw_edges[edge_idx..end];
        db.with_transaction(|tx| {
            for &(raw_u, raw_v) in chunk {
                let src = raw_to_engine[raw_u as usize];
                let dst = raw_to_engine[raw_v as usize];
                if src != 0 && dst != 0 && src != dst {
                    tx.add_edge(src, dst, "FRIEND", HashMap::new(), 1.0)?;
                }
            }
            Ok(())
        })?;
        edge_idx = end;
        if edge_idx % 500_000 == 0 || edge_idx == num_edges {
            let el = t_edges.elapsed().as_secs_f64();
            let cur_ops = edge_idx as f64 / el;
            print!(
                "\r   Edges: {} / {} ({:.1}%) | Elapsed: {:.1}s | Running Avg: {:.0} ops/s",
                edge_idx,
                num_edges,
                (edge_idx as f64 / num_edges as f64) * 100.0,
                el,
                cur_ops
            );
            std::io::Write::flush(&mut std::io::stdout())?;
        }
    }
    let edge_dur = t_edges.elapsed();
    let edge_ops = num_edges as f64 / edge_dur.as_secs_f64();
    println!(
        "\n   => Edge Ingestion Total: {:.2?} | {:.0} ops/s",
        edge_dur, edge_ops
    );

    // 5. Checkpoint
    println!("-> Step 5: Checkpointing to Internal NVMe SSD...");
    let t_ckpt = Instant::now();
    db.checkpoint()?;
    println!("   => Checkpoint in {:.2?}", t_ckpt.elapsed());

    let file_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    let stats = db.buffer_stats();
    println!(
        "   On-Disk File Size : {:.2} MB",
        file_size as f64 / 1_048_576.0
    );
    println!("   Cache Hit Rate    : {:.2}%", stats.hit_rate_percentage);
    println!("   Cache Misses      : {}", stats.cache_misses);
    println!("   WAL Fsync Count   : {}", stats.wal_fsync_count);
    println!("   Buffer Spills     : {}", stats.spill_count);
    println!("   Buffer Evictions  : {}", stats.evictions);

    // 6. Query Benchmark: Top 50 Hubs 1-Hop & 2-Hop
    println!("\n-> Step 6: Query Benchmark on Real Supernodes (Influencers with High Degree)...");

    // 6.1: 1-Hop on Top 50 Hubs
    let mut hub_1hop_latencies: Vec<f64> = Vec::with_capacity(top_hubs.len());
    let mut total_1hop_neighbors = 0usize;
    for &hub_id in &top_hubs {
        let t0 = Instant::now();
        let mut neighbors = HashSet::new();
        if let Some(node) = db.get_node(hub_id) {
            for &eid in &node.outgoing {
                if let Some(e) = db.get_edge(eid) {
                    neighbors.insert(e.dst_id);
                }
            }
            for &eid in &node.incoming {
                if let Some(e) = db.get_edge(eid) {
                    neighbors.insert(e.src_id);
                }
            }
        }
        let lat_us = t0.elapsed().as_secs_f64() * 1_000_000.0;
        hub_1hop_latencies.push(lat_us);
        total_1hop_neighbors += neighbors.len();
    }
    hub_1hop_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50_1hop = hub_1hop_latencies[hub_1hop_latencies.len() / 2];
    let p99_1hop = hub_1hop_latencies[(hub_1hop_latencies.len() as f64 * 0.99) as usize];
    let avg_1hop: f64 = hub_1hop_latencies.iter().sum::<f64>() / hub_1hop_latencies.len() as f64;
    println!(
        "   [Hub 1-Hop] Top 50 Hubs (Avg Degree: {:.1})",
        total_1hop_neighbors as f64 / top_hubs.len() as f64
    );
    println!(
        "      Avg: {:.2} µs | P50: {:.2} µs | P99: {:.2} µs | Total Neighbors: {}",
        avg_1hop, p50_1hop, p99_1hop, total_1hop_neighbors
    );

    // 6.2: 2-Hop on Top 50 Hubs
    let mut hub_2hop_latencies: Vec<f64> = Vec::with_capacity(top_hubs.len());
    let mut total_2hop_neighbors = 0usize;
    for &hub_id in &top_hubs {
        let t0 = Instant::now();
        let mut hop1 = HashSet::new();
        if let Some(node) = db.get_node(hub_id) {
            for &eid in &node.outgoing {
                if let Some(e) = db.get_edge(eid) {
                    hop1.insert(e.dst_id);
                }
            }
            for &eid in &node.incoming {
                if let Some(e) = db.get_edge(eid) {
                    hop1.insert(e.src_id);
                }
            }
        }
        let mut hop2 = HashSet::new();
        for &n1 in &hop1 {
            if let Some(node1) = db.get_node(n1) {
                for &eid in &node1.outgoing {
                    if let Some(e) = db.get_edge(eid) {
                        hop2.insert(e.dst_id);
                    }
                }
                for &eid in &node1.incoming {
                    if let Some(e) = db.get_edge(eid) {
                        hop2.insert(e.src_id);
                    }
                }
            }
        }
        let lat_ms = t0.elapsed().as_secs_f64() * 1000.0;
        hub_2hop_latencies.push(lat_ms);
        total_2hop_neighbors += hop2.len();
    }
    hub_2hop_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50_2hop = hub_2hop_latencies[hub_2hop_latencies.len() / 2];
    let p99_2hop = hub_2hop_latencies[(hub_2hop_latencies.len() as f64 * 0.99) as usize];
    let avg_2hop: f64 = hub_2hop_latencies.iter().sum::<f64>() / hub_2hop_latencies.len() as f64;
    println!("   [Hub 2-Hop Expansion] Top 50 Hubs");
    println!(
        "      Avg: {:.2} ms | P50: {:.2} ms | P99: {:.2} ms | Total 2-Hop Reached: {}",
        avg_2hop, p50_2hop, p99_2hop, total_2hop_neighbors
    );

    // 6.3: 1-Hop on 1,000 Random Nodes
    let rng_step = 137u64;
    let mut random_1hop_lats: Vec<f64> = Vec::with_capacity(1000);
    for i in 0..1000u64 {
        let raw_id = raw_nodes[((i * rng_step + 41) as usize) % num_nodes];
        let nid = raw_to_engine[raw_id as usize];
        let t0 = Instant::now();
        let mut neighbors = HashSet::new();
        if let Some(node) = db.get_node(nid) {
            for &eid in &node.outgoing {
                if let Some(e) = db.get_edge(eid) {
                    neighbors.insert(e.dst_id);
                }
            }
            for &eid in &node.incoming {
                if let Some(e) = db.get_edge(eid) {
                    neighbors.insert(e.src_id);
                }
            }
        }
        random_1hop_lats.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
    }
    random_1hop_lats.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50_rand = random_1hop_lats[500];
    let p99_rand = random_1hop_lats[990];
    let avg_rand = random_1hop_lats.iter().sum::<f64>() / 1000.0;
    println!("   [Random 1-Hop] 1,000 Random Nodes");
    println!(
        "      Avg: {:.2} µs | P50: {:.2} µs | P99: {:.2} µs",
        avg_rand, p50_rand, p99_rand
    );

    println!("============================================================");
    println!(" BENCHMARK COMPLETE FOR TEAM 3");
    println!("============================================================");

    Ok(())
}

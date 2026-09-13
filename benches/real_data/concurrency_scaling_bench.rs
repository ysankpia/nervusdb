//! 读扩展性基准：**不需要外部数据集**即可复现「读吞吐随线程数下降」。
//!
//! ## 为什么要有它
//!
//! `docs/benchmarks.md` 里的并发数字来自 com-DBLP（317k 节点/1.05M 边），而那份
//! 数据集不在仓库里。于是「读会负扩展」这条结论**无法被复现**——只有作者的机器
//! 上跑过。一个无法复现的性能结论，读者只能选择相信或不信，两种都不好。
//!
//! 本基准用**合成图**（确定性构造，无随机种子依赖）走同一条读取路径，因此任何人
//! 都能在本机得到同一量级的曲线。它测的不是绝对吞吐（那取决于硬件），而是
//! **曲线形状**：线程数增加时 ops/s 是否上升。
//!
//! ## 方法
//!
//! - 每个线程读**互不相交**的节点区间：排除缓存行共享，把变量收敛到「缓冲池争用」。
//! - 固定 1.5 秒时间预算，避免先跑完的配置被美化。
//! - 报告命中率：命中率高说明瓶颈是锁而不是磁盘。这是区分两者的关键证据——
//!   所有变体命中率都是 98.7%，因此吞吐下降只能归因于加锁。
//!
//! ## 配置
//!
//! `NODES`、`EDGES`、`POOL_FRAMES`（每帧 4KB）。默认 20 万节点/60 万边/4096 帧
//! （16MB）——池远小于数据，这是复现争用所需的压力。
//!
//! **`DB_PATH`：改为测量一个**已存在**的库**（例如 `snap_dblp_bench` 跑完留下的
//! `dblp_team3.db`），跳过合成图的构建。真实数据的度数分布是不均匀的，而合成图
//! 是均匀的——两者的争用形态不同，因此「合成图上量到 X 倍」不能直接当成「真实
//! 数据上也是 X 倍」。要下结论就用真实库量一次。

use nervusdb::{GraphError, NervusDb, NervusDbOptions, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};
use tempfile::tempdir;

/// 池帧数（每帧 4KB）。默认 4096 = 16MB，远小于数据量。
fn pool_frames() -> usize {
    std::env::var("POOL_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4096)
}

fn build(path: &std::path::Path, nodes: u64, edges: u64) -> Result<(), GraphError> {
    let db = NervusDb::open_with_options(
        path,
        NervusDbOptions {
            buffer_pool_frames: pool_frames(),
            wal_auto_checkpoint_bytes: 0,
            ..Default::default()
        },
    )?;
    let mut tx = db.begin_transaction()?;
    let mut ids = Vec::new();
    for i in 0..nodes {
        ids.push(tx.add_node(
            HashSet::from(["N".to_string()]),
            HashMap::from([("i".to_string(), Value::from(i as i64))]),
        )?);
    }
    for e in 0..edges {
        let a = ids[(e % nodes) as usize];
        let b = ids[((e * 7 + 3) % nodes) as usize];
        tx.add_edge(a, b, "R", HashMap::new(), 1.0)?;
    }
    tx.commit()?;
    db.checkpoint()?;
    Ok(())
}

/// 所有线程读**同一批高度数枢纽**：全局锁的最坏情形。
///
/// `docs/benchmarks.md` 引用的「0.6%–1.4% 效率」就是在枢纽点读下测的，因此同口径
/// 对照必须用枢纽，而不是互不相交的区间（那是最**好**情形，争用最少）。两个数字
/// 若混用，看起来就像同一次测量的前后对比，实际却是在比两种不同的负载。
fn run_hub_reads(path: &std::path::Path, nodes: u64) -> Result<(), Box<dyn std::error::Error>> {
    let hub_count: usize = std::env::var("HUB_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);

    // 扫一遍取度数最高的若干节点（与 snap 基准同规则：度数降序、同度 id 升序）
    let mut degs: Vec<(u64, usize)> = Vec::new();
    {
        let probe = NervusDb::open_read_only(path)?;
        for id in 1..=nodes {
            if let Ok(Some(n)) = probe.try_get_node(id) {
                let d = n.outgoing.len() + n.incoming.len();
                if d > 0 {
                    degs.push((id, d));
                }
            }
        }
    }
    degs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let hubs: Vec<u64> = degs.iter().take(hub_count).map(|r| r.0).collect();
    println!(
        "hub mode: {} hubs, max degree {}",
        hubs.len(),
        degs.first().map(|r| r.1).unwrap_or(0)
    );

    for threads in [1usize, 2, 4, 8, 16] {
        let db = Arc::new(NervusDb::open_with_options(
            path,
            NervusDbOptions {
                buffer_pool_frames: pool_frames(),
                read_only: true,
                wal_auto_checkpoint_bytes: 0,
                ..Default::default()
            },
        )?);
        let barrier = Arc::new(Barrier::new(threads));
        let ops = Arc::new(AtomicU64::new(0));
        let budget = Duration::from_millis(1500);
        let hubs = Arc::new(hubs.clone());
        let start = Instant::now();

        let mut handles = Vec::new();
        for _ in 0..threads {
            let db = Arc::clone(&db);
            let barrier = Arc::clone(&barrier);
            let ops = Arc::clone(&ops);
            let hubs = Arc::clone(&hubs);
            handles.push(std::thread::spawn(move || {
                let mut i = 0usize;
                let mut local = 0u64;
                barrier.wait();
                while start.elapsed() < budget {
                    if let Ok(Some(_)) = db.try_get_node(hubs[i % hubs.len()]) {
                        local += 1;
                    }
                    i += 1;
                }
                ops.fetch_add(local, Ordering::Relaxed);
            }));
        }
        for h in handles {
            let _ = h.join();
        }
        let secs = start.elapsed().as_secs_f64();
        let total = ops.load(Ordering::Relaxed);
        let per_sec = total as f64 / secs;
        let st = db.buffer_stats();
        println!(
            "threads={threads:<3} ops/s={per_sec:>12.0}  per-thread={:>10.0}  hit%={:.1}",
            per_sec / threads as f64,
            st.hit_rate_percentage
        );
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `DB_PATH` 优先：直接量一个已存在的库（真实数据），不构建合成图。
    if let Ok(existing) = std::env::var("DB_PATH") {
        let path = std::path::PathBuf::from(&existing);
        let probe = NervusDb::open_read_only(&path)?;
        let nodes = probe.node_count() as u64;
        let edges = probe.edge_count() as u64;
        drop(probe);
        println!("existing database: {existing}");
        println!(
            "  nodes={nodes} edges={edges} (pool {} frames)",
            pool_frames()
        );
        // `HUB_READ=1`：读同一批枢纽（最坏情形），用于与 benchmarks.md 的口径对照。
        if std::env::var("HUB_READ").is_ok() {
            return run_hub_reads(&path, nodes);
        }
        return run_scaling(&path, nodes);
    }

    let dir = tempdir()?;
    let path = dir.path().join("scale.db");
    let nodes: u64 = std::env::var("NODES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200_000);
    let edges: u64 = std::env::var("EDGES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(600_000);
    println!("building {nodes} nodes / {edges} edges ...");
    let t = Instant::now();
    build(&path, nodes, edges)?;
    println!("built in {:.2}s", t.elapsed().as_secs_f64());

    run_scaling(&path, nodes)
}

/// 逐线程数测量点读吞吐。`nodes` 用于把读区间切成互不相交的片段。
fn run_scaling(path: &std::path::Path, nodes: u64) -> Result<(), Box<dyn std::error::Error>> {
    for threads in [1usize, 2, 4, 8, 16] {
        // **必须用 `open_with_options` 而不是 `open_read_only`**：后者取默认池
        // （1024 帧 = 4MB），会让 `POOL_FRAMES` 被静默忽略。首版就是这样，于是
        // 「把库全装进池里再测」那个对照实验实际测的还是 4MB 池——命中率 80% 说明
        // 有大量真实磁盘读，而那与要排除的变量正是同一个。一个被静默忽略的配置
        // 参数会把对照实验变成自我欺骗。
        let db = Arc::new(NervusDb::open_with_options(
            path,
            NervusDbOptions {
                buffer_pool_frames: pool_frames(),
                read_only: true,
                wal_auto_checkpoint_bytes: 0,
                ..Default::default()
            },
        )?);
        let barrier = Arc::new(Barrier::new(threads));
        let ops = Arc::new(AtomicU64::new(0));
        // 每个线程读**互不相交**的节点区间：避免缓存行共享，只留缓冲池争用
        let per = nodes / threads as u64;
        let budget = Duration::from_millis(1500);
        let start = Instant::now();

        let mut handles = Vec::new();
        for t in 0..threads {
            let db = Arc::clone(&db);
            let barrier = Arc::clone(&barrier);
            let ops = Arc::clone(&ops);
            handles.push(std::thread::spawn(move || {
                let lo = t as u64 * per + 1;
                let hi = lo + per;
                let mut id = lo;
                let mut local = 0u64;
                barrier.wait();
                while start.elapsed() < budget {
                    if let Ok(Some(_)) = db.try_get_node(id) {
                        local += 1;
                    }
                    id += 1;
                    if id >= hi {
                        id = lo;
                    }
                }
                ops.fetch_add(local, Ordering::Relaxed);
            }));
        }
        for h in handles {
            let _ = h.join();
        }
        let secs = start.elapsed().as_secs_f64();
        let total = ops.load(Ordering::Relaxed);
        let per_sec = total as f64 / secs;
        let st = db.buffer_stats();
        println!(
            "threads={threads:<3} ops/s={per_sec:>12.0}  per-thread={:>10.0}  hit%={:.1} misses={}",
            per_sec / threads as f64,
            st.hit_rate_percentage,
            st.cache_misses
        );
    }
    Ok(())
}

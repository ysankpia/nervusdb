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

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    for threads in [1usize, 2, 4, 8] {
        let db = Arc::new(NervusDb::open_read_only(&path)?);
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

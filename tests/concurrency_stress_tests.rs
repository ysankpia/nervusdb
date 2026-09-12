//! 1.1.0 套件：高并发下的读写压力（可移植）。
//!
//! ## 为什么需要它
//!
//! 既有的并发用例固定 20 线程，而线程数应当跟着机器走：在 8 核机器上 20 线程
//! 已超出并行度，在 64 核机器上 20 线程根本压不到锁竞争。本套件按**可用并行度**
//! 决定线程数，因此换机器后仍然有意义。
//!
//! ## 断言的是「不死锁、不丢数据、不损坏」，不是吞吐量
//!
//! 吞吐量随机器剧烈变化，写进测试只会变成噪声（AGENTS.md §3.2 记过一次教训：
//! 一条「批量比逐条快 20 倍」的断言在云端磁盘上以 16 倍失败）。这里只断言在任何
//! 机器上都成立的性质：
//!
//! 1. 全部线程都能正常 join（无死锁）；
//! 2. 写入总量精确等于各线程报告之和（无丢写）；
//! 3. 结束后图结构自洽（无损坏）；
//! 4. 读线程在写入期间确实读到了数据（读者没有被饿死）。
//!
//! 线程数与耗时上限都放宽到「任何合理机器都能过」的程度。

use graphlite::{GraphError, GraphLite, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::tempdir;

/// 可用并行度：取核心数，钳在 [4, 32]。
///
/// 下限 4 保证在单核 CI 上仍能触发交错；上限 32 避免在超大机器上把测试拖长
/// （每线程都要做真实的磁盘写入）。
fn parallelism() -> usize {
    thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(4, 32)
}

// =========================================================================
// 1. 全并行拓扑写入 + 并发读
// =========================================================================

/// 每个线程在障碍点后同时开始写入，写入期间另一组线程持续读。
///
/// 断言：无死锁、写入数精确、结构自洽。
#[test]
fn test_full_parallelism_mixed_read_write() -> Result<(), GraphError> {
    let par = parallelism();
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join("stress_mixed.db"))?;

    // 每线程写入量随并行度收缩，使总耗时与机器规模基本无关
    let per_thread = 400 / par + 20;
    let writers = par;
    let readers = (par / 2).max(2);

    let barrier = Arc::new(Barrier::new(writers + readers));
    let stop = Arc::new(AtomicBool::new(false));
    let write_ops = Arc::new(AtomicUsize::new(0));
    let read_hits = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();

    for t in 0..writers {
        let db = db.clone();
        let barrier = Arc::clone(&barrier);
        let write_ops = Arc::clone(&write_ops);
        handles.push(thread::spawn(move || -> Result<(), GraphError> {
            barrier.wait();
            // 每个线程建自己的一条链，避免跨线程依赖
            let mut prev = db.add_node(HashSet::from(["Root".to_string()]), HashMap::new())?;
            write_ops.fetch_add(1, Ordering::Relaxed);
            for i in 0..per_thread {
                let mut labels = HashSet::new();
                labels.insert("Chain".to_string());
                labels.insert(format!("T{t}"));
                let mut props = HashMap::new();
                props.insert("t".to_string(), Value::from(t as i64));
                props.insert("i".to_string(), Value::from(i as i64));
                let id = db.add_node(labels, props)?;
                db.add_edge(prev, id, "NEXT", HashMap::new(), 1.0)?;
                prev = id;
                write_ops.fetch_add(2, Ordering::Relaxed);
            }
            Ok(())
        }));
    }

    for _ in 0..readers {
        let db = db.clone();
        let barrier = Arc::clone(&barrier);
        let stop = Arc::clone(&stop);
        let read_hits = Arc::clone(&read_hits);
        handles.push(thread::spawn(move || -> Result<(), GraphError> {
            barrier.wait();
            while !stop.load(Ordering::Relaxed) {
                let res = db.run_cypher("MATCH (c:Chain) RETURN count(*)")?;
                // 读到任何东西都算一次成功的读；读到 0 也可能是合法的早期状态
                let _ = &res.rows[0].values[0];
                read_hits.fetch_add(1, Ordering::Relaxed);
            }
            Ok(())
        }));
    }

    // 等写者全部完成
    let mut written = 0usize;
    for h in handles.drain(..writers) {
        h.join().expect("写线程 panic")?;
        written += 1;
    }
    stop.store(true, Ordering::Relaxed);
    for h in handles {
        h.join().expect("读线程 panic")?;
    }

    // 1) 无死锁：能走到这里就说明 join 全部成功
    // 2) 写入数精确：每个线程 1 个根 + per_thread 个节点 + per_thread 条边
    assert_eq!(written, writers, "全部写线程都应完成");
    assert!(write_ops.load(Ordering::Relaxed) > 0, "写操作应发生");

    let expected_nodes = writers * (1 + per_thread);
    assert_eq!(
        db.node_count(),
        expected_nodes,
        "节点数必须精确等于各线程写入之和（无丢写）"
    );

    let res = db.run_cypher("MATCH (c:Chain) RETURN count(*)")?;
    assert_eq!(
        res.rows[0].values[0],
        Value::Int((writers * per_thread) as i64),
        "索引计数同样必须精确"
    );

    // 3) 结构自洽
    let report = db.integrity_check()?;
    assert!(
        report.is_ok(),
        "高压写入后结构必须自洽，问题: {:?}",
        report.issues.first()
    );

    // 4) 读线程确实在跑
    assert!(
        read_hits.load(Ordering::Relaxed) > 0,
        "读线程未完成任何查询，测试无效"
    );

    Ok(())
}

// =========================================================================
// 2. 无死锁：写者在读者持续压力下仍能完成
// =========================================================================

/// 读者长时间持有读锁时，写者必须**最终**拿到锁。
///
/// `std::sync::RwLock` 的公平性没有跨平台保证：一个持续被读者占用的读锁可能让
/// 写者长期饥饿。这条测试把它变成可观察的断言——写者必须在有限时间内完成。
#[test]
fn test_writer_is_not_starved_by_readers() -> Result<(), GraphError> {
    let par = parallelism();
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join("starvation.db"))?;

    let stop = Arc::new(AtomicBool::new(false));
    let mut readers = Vec::new();
    for _ in 0..par {
        let db = db.clone();
        let stop = Arc::clone(&stop);
        readers.push(thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let _ = db.try_get_node(1);
            }
        }));
    }

    // 写者：在读者压力下完成若干次写入
    let t0 = std::time::Instant::now();
    for i in 0..50 {
        let mut props = HashMap::new();
        props.insert("i".to_string(), Value::from(i));
        db.add_node(HashSet::new(), props)?;
    }
    let elapsed = t0.elapsed();

    stop.store(true, Ordering::Relaxed);
    for r in readers {
        let _ = r.join();
    }

    assert_eq!(db.node_count(), 50, "写者必须完成全部写入");
    // 上限刻意宽松：只要求「没有无限期饥饿」，不要求任何具体速度
    assert!(
        elapsed < std::time::Duration::from_secs(60),
        "写者在读者压力下耗时 {elapsed:?}，疑似饥饿"
    );

    Ok(())
}

// =========================================================================
// 3. 并发只读：多读者之间不应相互阻塞或出错
// =========================================================================

#[test]
fn test_concurrent_readers_are_independent() -> Result<(), GraphError> {
    let par = parallelism();
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join("many_readers.db"))?;

    // 建一个够读的图：1 个起点 + 500 个循环节点 = 501 个 :N
    let mut prev = db.add_node(HashSet::from(["N".to_string()]), HashMap::new())?;
    for i in 0..500 {
        let mut props = HashMap::new();
        props.insert("i".to_string(), Value::from(i));
        let id = db.add_node(HashSet::from(["N".to_string()]), props)?;
        db.add_edge(prev, id, "L", HashMap::new(), 1.0)?;
        prev = id;
    }
    const EXPECTED: i64 = 501;
    assert_eq!(
        db.node_count(),
        EXPECTED as usize,
        "前置条件：图应有 {EXPECTED} 个节点"
    );

    let barrier = Arc::new(Barrier::new(par));
    let results = Arc::new(std::sync::Mutex::new(Vec::<i64>::new()));

    let mut handles = Vec::new();
    for _ in 0..par {
        let db = db.clone();
        let barrier = Arc::clone(&barrier);
        let results = Arc::clone(&results);
        handles.push(thread::spawn(move || -> Result<(), GraphError> {
            barrier.wait();
            for _ in 0..20 {
                let res = db.run_cypher("MATCH (n:N) RETURN count(*)")?;
                let v = match &res.rows[0].values[0] {
                    Value::Int(v) => *v,
                    other => panic!("count 必须是 Int，实际 {other:?}"),
                };
                results.lock().unwrap().push(v);
            }
            Ok(())
        }));
    }

    for h in handles {
        h.join().expect("读线程 panic")?;
    }

    // 无写入，所以每个读者每次都必须看到同一个数
    let results = results.lock().unwrap();
    assert_eq!(results.len(), par * 20, "每个读者都应完成全部查询");
    assert!(
        results.iter().all(|&v| v == EXPECTED),
        "无写入时所有读者的计数必须一致，实际出现: {:?}",
        results.iter().collect::<std::collections::BTreeSet<_>>()
    );

    Ok(())
}

//! 1.1.0 套件：并发正确性（快照隔离的验收标准）。
//!
//! ## 这套测试的定位
//!
//! 它先于实现存在。按用户要求「先写并发正确性测试再改实现」——把**期望的保证**
//! 写成可执行的断言，再让实现去满足它们。这样做的理由是：并发缺陷无法靠阅读代码
//! 发现，只能靠让它在压力下真的发生。若先改实现再补测试，测试会不自觉地围绕
//! 实现的实际行为书写，而不是围绕应有的行为。
//!
//! ## 这里断言的是「可观察的正确性」，不是「并发度」
//!
//! 单个操作是否够快、多少线程能同时进临界区，属于性能问题，由基准回答。
//! 这套测试只问一件事：**在任何交错下，读到的数据是否都是一个真实发生过的状态**。
//! 具体是三条：
//!
//! 1. **无撕裂邻接表**：读到的每条 `Node.outgoing/incoming` 里的边 ID，都必须
//!    能解析出一条 src/dst 与宿主节点相符的边记录。撕裂的邻接表会让遍历走进
//!    不存在的边，而这类故障在单线程测试里永远不出现。
//! 2. **提交原子可见**：一批已提交的写入，要么全部可见、要么全部不可见。
//!    读到「一半」的样子（例如边已存在但端点还没出现）就是脏读。
//! 3. **不丢写**：N 个写线程各写 M 条，最终必须精确等于 N×M。
//!
//! ## 失败信息要能定位
//!
//! 每个断言都带上具体是哪条边、哪个节点出的问题。并发测试的失败若只说
//! 「不一致」，排查成本会高到让人放弃这个测试。

use graphlite::{GraphError, GraphLite, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::tempdir;

/// 邻接表一致性 oracle：遍历所有节点的邻接链，逐条验证边记录存在且端点相符。
///
/// 返回 `Err(描述)` 而不是 `bool`：并发测试失败时必须能说清是哪条边、期望什么、
/// 实际什么，否则无法复现。
///
/// 走 `read_snapshot` 而不是逐个 `try_get_node` / `try_get_edge`：后者每次调用
/// 各自取放读锁，一次遍历会横跨多个时刻，从而**把两个不同时刻的状态拼在一起**，
/// 看到并发删除造成的表面不一致。那不是引擎内部撕裂，而是读者缺少一致性窗口
/// ——本套件最初正是这样失败的（首个复现是「节点 2 引用了不存在的边 1」），
/// `GraphLite::read_snapshot` 就是为此提供的。
fn check_adjacency_integrity(db: &GraphLite) -> Result<usize, String> {
    let snapshot = db.read_snapshot();
    let n = snapshot.node_count() as u64;
    let mut checked = 0usize;

    for nid in 1..=n {
        let node = match snapshot.get_node(nid) {
            Ok(Some(node)) => node,
            Ok(None) => continue, // 已被删除或从未存在
            Err(e) => return Err(format!("读取节点 {nid} 失败: {e}")),
        };

        for &eid in node.outgoing.iter().chain(node.incoming.iter()) {
            let edge = match snapshot.get_edge(eid) {
                Ok(Some(edge)) => edge,
                Ok(None) => {
                    return Err(format!(
                        "节点 {nid} 的邻接表引用了不存在的边 {eid}（同一快照内的悬空引用）"
                    ))
                }
                Err(e) => return Err(format!("读取边 {eid} 失败: {e}")),
            };

            // 这条边必须真的与该节点相邻
            if edge.src_id != nid && edge.dst_id != nid {
                return Err(format!(
                    "节点 {nid} 的邻接表引用了边 {eid}，但该边连接的是 {} -> {}",
                    edge.src_id, edge.dst_id
                ));
            }
            checked += 1;
        }
    }

    Ok(checked)
}

// =========================================================================
// 1. 无撕裂邻接表
// =========================================================================

/// 读线程持续遍历全图，同时写线程不断增删边。
///
/// 若读操作能看到「边已进邻接表、记录还没写」或反之的中间状态，邻接表就会
/// 引用一条解析不出来的边。断言每次检查都通过。
#[test]
fn test_reader_never_observes_torn_adjacency_under_writes() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join("torn.db"))?;

    // 一批稳定存在的节点，供写线程在其间连边
    let mut ids = Vec::new();
    for _ in 0..16 {
        ids.push(db.add_node(HashSet::new(), HashMap::new())?);
    }

    let stop = Arc::new(AtomicBool::new(false));
    let reads = Arc::new(AtomicUsize::new(0));
    let failures = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

    let mut handles = Vec::new();

    // 写线程：反复在既有节点间添加与删除边
    {
        let db = db.clone();
        let stop = Arc::clone(&stop);
        let ids = ids.clone();
        handles.push(thread::spawn(move || {
            let mut i = 0usize;
            while !stop.load(Ordering::Relaxed) {
                let src = ids[i % ids.len()];
                let dst = ids[(i * 7 + 3) % ids.len()];
                if src != dst {
                    if let Ok(eid) = db.add_edge(src, dst, "LINK", HashMap::new(), 1.0) {
                        let _ = db.remove_edge(eid);
                    }
                }
                i += 1;
            }
        }));
    }

    // 读线程：持续做全图邻接一致性检查
    {
        let db = db.clone();
        let stop = Arc::clone(&stop);
        let reads = Arc::clone(&reads);
        let failures = Arc::clone(&failures);
        handles.push(thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if let Err(msg) = check_adjacency_integrity(&db) {
                    failures.lock().unwrap().push(msg);
                }
                reads.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    // 跑一段时间
    thread::sleep(std::time::Duration::from_millis(2500));
    stop.store(true, Ordering::Relaxed);
    for h in handles {
        let _ = h.join();
    }

    let reads = reads.load(Ordering::Relaxed);
    assert!(reads > 0, "读线程未执行，测试无效");

    let failures = failures.lock().unwrap();
    assert!(
        failures.is_empty(),
        "读到 {} 次不一致状态（共 {reads} 次检查）。首个错误: {}",
        failures.len(),
        failures.first().map(String::as_str).unwrap_or("(无)")
    );

    Ok(())
}

// =========================================================================
// 2. 提交原子可见
// =========================================================================

/// 一个已提交的事务，其全部写入必须一次可见。
///
/// 构造：写线程在一个事务里创建「中心节点 + 一条指向它的边」，读线程检查
/// 「绝不出现只有边没有端点」的情形。这正是「读到半个事务」的可观察形式。
#[test]
fn test_committed_batch_is_visible_atomically() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join("atomic_visible.db"))?;

    let hub = db.add_node(HashSet::new(), HashMap::new())?;

    let stop = Arc::new(AtomicBool::new(false));
    let checks = Arc::new(AtomicUsize::new(0));
    let failures = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

    // 写线程：事务内一起写入「新节点 + 指向 hub 的边」
    let writer = {
        let db = db.clone();
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            let mut count = 0usize;
            while !stop.load(Ordering::Relaxed) {
                let result = db.with_transaction(|tx| {
                    let mut labels = HashSet::new();
                    labels.insert("Batch".to_string());
                    let mut props = HashMap::new();
                    props.insert("seq".to_string(), Value::from(count as i64));
                    let id = tx.add_node(labels, props)?;
                    tx.add_edge(id, hub, "POINTS", HashMap::new(), 1.0)?;
                    Ok(())
                });
                if result.is_ok() {
                    count += 1;
                }
            }
            count
        })
    };

    // 读线程：检查「边存在则其两端节点也必须存在」
    let reader = {
        let db = db.clone();
        let stop = Arc::clone(&stop);
        let checks = Arc::clone(&checks);
        let failures = Arc::clone(&failures);
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let total = db.edge_count() as u64;
                for eid in 1..=total {
                    let edge = match db.try_get_edge(eid) {
                        Ok(Some(e)) => e,
                        _ => continue,
                    };
                    if db.try_get_node(edge.src_id).ok().flatten().is_none() {
                        failures.lock().unwrap().push(format!(
                            "边 {eid} 已可见，但源节点 {} 不可见（读到了半个事务）",
                            edge.src_id
                        ));
                    }
                    if db.try_get_node(edge.dst_id).ok().flatten().is_none() {
                        failures.lock().unwrap().push(format!(
                            "边 {eid} 已可见，但目标节点 {} 不可见（读到了半个事务）",
                            edge.dst_id
                        ));
                    }
                }
                checks.fetch_add(1, Ordering::Relaxed);
            }
        })
    };

    thread::sleep(std::time::Duration::from_millis(2500));
    stop.store(true, Ordering::Relaxed);
    let _ = writer.join();
    let _ = reader.join();

    let checks = checks.load(Ordering::Relaxed);
    assert!(checks > 0, "读线程未执行，测试无效");

    let failures = failures.lock().unwrap();
    assert!(
        failures.is_empty(),
        "读到 {} 次非原子可见状态（共 {checks} 轮），首个: {}",
        failures.len(),
        failures.first().map(String::as_str).unwrap_or("(无)")
    );

    Ok(())
}

// =========================================================================
// 3. 不丢写
// =========================================================================

#[test]
fn test_concurrent_writers_do_not_lose_writes() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join("no_lost_writes.db"))?;

    const THREADS: usize = 8;
    const PER_THREAD: usize = 60;
    let barrier = Arc::new(Barrier::new(THREADS));

    let mut handles = Vec::new();
    for t in 0..THREADS {
        let db = db.clone();
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || -> Result<usize, GraphError> {
            barrier.wait();
            let mut written = 0usize;
            for i in 0..PER_THREAD {
                let mut labels = HashSet::new();
                labels.insert("W".to_string());
                let mut props = HashMap::new();
                props.insert("t".to_string(), Value::from(t as i64));
                props.insert("i".to_string(), Value::from(i as i64));
                db.add_node(labels, props)?;
                written += 1;
            }
            Ok(written)
        }));
    }

    let mut total = 0usize;
    for h in handles {
        total += h.join().expect("写线程 panic")?;
    }

    assert_eq!(total, THREADS * PER_THREAD, "写线程报告的写入数不对");
    assert_eq!(
        db.node_count(),
        THREADS * PER_THREAD,
        "最终节点数必须精确等于各线程写入之和（不丢写）"
    );

    let res = db.run_cypher("MATCH (w:W) RETURN count(*)")?;
    assert_eq!(
        res.rows[0].values[0],
        Value::Int((THREADS * PER_THREAD) as i64),
        "通过索引统计的节点数同样必须精确"
    );

    Ok(())
}

// =========================================================================
// 4. 读写可以并存（不互相饿死）
// =========================================================================

/// 读与写在时间上重叠时，两者都必须能推进。
///
/// 断言的是**进展**而不是速度：在 2 秒窗口内，读操作必须发生多次，写操作也必须
/// 发生多次。若读者与写者互相完全阻塞（或某一方长期拿不到锁），这个断言会失败。
/// 它不关心具体吞吐量——那是基准的事。
#[test]
fn test_readers_and_writers_both_make_progress() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join("progress.db"))?;

    let hub = db.add_node(HashSet::new(), HashMap::new())?;

    let stop = Arc::new(AtomicBool::new(false));
    let reads = Arc::new(AtomicUsize::new(0));
    let writes = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..4 {
        let db = db.clone();
        let stop = Arc::clone(&stop);
        let reads = Arc::clone(&reads);
        handles.push(thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if db.run_cypher("MATCH (n) RETURN count(*)").is_ok() {
                    reads.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }
    for _ in 0..2 {
        let db = db.clone();
        let stop = Arc::clone(&stop);
        let writes = Arc::clone(&writes);
        handles.push(thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if db
                    .add_node(HashSet::new(), HashMap::new())
                    .and_then(|id| db.add_edge(id, hub, "R", HashMap::new(), 1.0).map(|_| ()))
                    .is_ok()
                {
                    writes.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    thread::sleep(std::time::Duration::from_millis(2000));
    stop.store(true, Ordering::Relaxed);
    for h in handles {
        let _ = h.join();
    }

    let reads = reads.load(Ordering::Relaxed);
    let writes = writes.load(Ordering::Relaxed);
    assert!(reads > 10, "2 秒内只完成 {reads} 次读查询，读者被写者饿死");
    assert!(writes > 10, "2 秒内只完成 {writes} 次写，写者被读者饿死");

    Ok(())
}

// =========================================================================
// 5. 读到的图必须自洽（索引与数据一致）
// =========================================================================

/// 并发写入期间，标签索引与磁盘数据必须保持一致。
///
/// 索引是派生的：它必须最终与真实数据相等。若某次写入只更新了索引或只更新了
/// 数据，`count(*)` 与全表扫描给出的数字就会不同。
#[test]
fn test_index_and_scan_agree_under_concurrent_writes() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join("index_agree.db"))?;

    let stop = Arc::new(AtomicBool::new(false));
    let mismatches = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let checks = Arc::new(AtomicUsize::new(0));

    let writer = {
        let db = db.clone();
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            let mut i = 0i64;
            while !stop.load(Ordering::Relaxed) {
                let mut labels = HashSet::new();
                labels.insert("Idx".to_string());
                let mut props = HashMap::new();
                props.insert("v".to_string(), Value::from(i));
                let _ = db.add_node(labels, props);
                i += 1;
            }
        })
    };

    let reader = {
        let db = db.clone();
        let stop = Arc::clone(&stop);
        let mismatches = Arc::clone(&mismatches);
        let checks = Arc::clone(&checks);
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                // 两条查询必须在**同一个快照**内执行。
                //
                // 否则「索引读在前、扫描读在后」会横跨两个时刻：抽取到的
                // `(0, N)` 只是「读索引时还没人写、读扫描时已经写了」，
                // 属于读者自己拼接出的状态，不是索引缺陷。这条测试最初正是
                // 这样偶发失败的（约每几十次一轮），而偶发失败的断言没有价值。
                let (idx, scan) = {
                    let snapshot = db.read_snapshot();
                    let idx = match snapshot.query("MATCH (n:Idx) RETURN count(*)") {
                        Ok(r) => r.rows[0].values[0].clone(),
                        Err(_) => continue,
                    };
                    let scan = match snapshot.query("MATCH (n) WHERE n.v >= 0 RETURN count(*)") {
                        Ok(r) => r.rows[0].values[0].clone(),
                        Err(_) => continue,
                    };
                    (idx, scan)
                };
                if let (Value::Int(a), Value::Int(b)) = (idx, scan) {
                    // 同一快照内，索引计数不得小于它对应的标签集合中的行数：
                    // 那意味着索引丢了已提交的行。反过来（索引多于扫描）不会发生，
                    // 因为两者统计的是同一批节点。
                    if a < b {
                        mismatches.lock().unwrap().push(format!(
                            "同一快照内索引报告 {a} 行，少于扫描的 {b} 行（索引丢行）"
                        ));
                    }
                }
                checks.fetch_add(1, Ordering::Relaxed);
            }
        })
    };

    thread::sleep(std::time::Duration::from_millis(2000));
    stop.store(true, Ordering::Relaxed);
    let _ = writer.join();
    let _ = reader.join();

    // 静止后，两个路径必须完全一致：这是索引正确性的终态断言
    let idx = db.run_cypher("MATCH (n:Idx) RETURN count(*)")?;
    let scan = db.run_cypher("MATCH (n) WHERE n.v >= 0 RETURN count(*)")?;
    assert_eq!(
        idx.rows[0].values[0], scan.rows[0].values[0],
        "静止后索引计数与全表扫描不一致（索引与数据脱节）"
    );

    let mismatches = mismatches.lock().unwrap();
    assert!(
        mismatches.is_empty(),
        "并发期间索引与数据出现 {} 次不一致，首个: {}",
        mismatches.len(),
        mismatches.first().map(String::as_str).unwrap_or("(无)")
    );

    Ok(())
}

// =========================================================================
// 6. 快照契约
// =========================================================================

/// 快照内的多步遍历不能与并发删除交错。
///
/// ## 为什么这条测试必须是确定性的
///
/// 并发压力版（`test_reader_never_observes_torn_adjacency_under_writes`）确实能
/// 复现问题——它第一次运行就报了「节点 2 的邻接表引用了不存在的边 1」。但把修复
/// 移除后重跑，连续 5 次都没再复现：交错窗口取决于调度时机，压力测试只能证明
/// 「曾经坏过」，不足以作为回归防线。
///
/// 因此这里用精确编排代替碰运气：读线程**显式地**读了一半就停住，让写者去尝试
/// 删除。快照若真的持锁，写者必然被挡住；若有人把它改成不加锁（例如为了「提高
/// 并发度」），这里会 100% 失败而不是偶尔失败。这正是 AGENTS.md §3.3 要求的：
/// 能可靠失败的测试才有价值。
#[test]
fn test_snapshot_prevents_interleaved_deletion() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join("snapshot_interleave.db"))?;

    let a = db.add_node(HashSet::new(), HashMap::new())?;
    let b = db.add_node(HashSet::new(), HashMap::new())?;
    let e = db.add_edge(a, b, "LINK", HashMap::new(), 1.0)?;

    // 读了一半：邻接表已读到，边记录还没读
    let snapshot = db.read_snapshot();
    let node = snapshot.get_node(a)?.expect("节点 a 应存在");
    assert!(node.outgoing.contains(&e), "前置条件：a 的出边链应含边 {e}");

    // 写者试图在这个窗口里删掉那条边
    let writer_done = Arc::new(AtomicBool::new(false));
    let writer = {
        let db = db.clone();
        let done = Arc::clone(&writer_done);
        let eid = e;
        thread::spawn(move || {
            let result = db.remove_edge(eid);
            done.store(true, Ordering::SeqCst);
            result
        })
    };

    // 给它充分的机会去尝试；快照持读锁，它此刻必然拿不到写锁
    thread::sleep(std::time::Duration::from_millis(400));
    assert!(
        !writer_done.load(Ordering::SeqCst),
        "快照存续期间写者完成了删除——快照没有锁住状态，读写会交错"
    );

    // 读完后半：必须与前半看到同一个状态
    let edge = snapshot.get_edge(e)?;
    assert!(
        edge.is_some(),
        "同一快照内的两次读看到了不同状态：邻接表引用边 {e}，但边读不出来"
    );

    // 释放快照，写者随即应当能够完成
    drop(snapshot);
    writer.join().expect("写者线程 panic")?;
    assert!(
        writer_done.load(Ordering::SeqCst),
        "释放快照后写者应当能够完成删除"
    );

    // 终态：边确实被删掉了，且未留下悬空引用
    let snapshot = db.read_snapshot();
    let node = snapshot.get_node(a)?.expect("节点 a 应存在");
    assert!(
        !node.outgoing.contains(&e),
        "删除完成后邻接表不应再引用边 {e}"
    );
    drop(snapshot);

    Ok(())
}

/// 快照会被写者阻塞（当前架构的既有语义），且必须**能释放**。
///
/// 这条测试锁住「快照不会永久阻塞后续写入」：若 Drop 没释放读锁，写者会永远
/// 挂起，测试以超时失败。它同时把代价写清楚——快照不是免费的。
#[test]
fn test_snapshot_blocks_writer_until_released() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join("snapshot_blocks.db"))?;
    db.add_node(HashSet::new(), HashMap::new())?;

    let snapshot = db.read_snapshot();

    // 写者线程在快照存续期间应当被挡住
    let (tx, rx) = std::sync::mpsc::channel();
    let writer = {
        let db = db.clone();
        thread::spawn(move || {
            let result = db.add_node(HashSet::new(), HashMap::new());
            let _ = tx.send(result.is_ok());
        })
    };

    // 给写者足够机会去尝试；它此刻必然拿不到写锁
    assert!(
        rx.recv_timeout(std::time::Duration::from_millis(300))
            .is_err(),
        "快照存续期间写者不应完成写入"
    );

    // 释放快照后，写者必须能继续
    drop(snapshot);
    assert!(
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .expect("释放快照后写者应能完成写入"),
        "释放快照后写入应当成功"
    );
    let _ = writer.join();

    Ok(())
}

/// 快照只能读：经它执行写语句必须被拒绝。
///
/// 与只读句柄同源的理由——一个自洽的只读视图如果能被自己改写，它就不再自洽。
#[test]
fn test_snapshot_rejects_mutating_cypher() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join("snapshot_ro.db"))?;
    db.add_node(HashSet::new(), HashMap::new())?;

    let snapshot = db.read_snapshot();

    // 只读查询正常
    let res = snapshot.query("MATCH (n) RETURN count(*)")?;
    assert_eq!(res.row_count(), 1);

    // 写语句被拒绝
    let err = snapshot
        .query("CREATE (:X {v: 1})")
        .expect_err("快照不得执行写语句");
    assert!(
        err.to_string().contains("read-only"),
        "应说明这是只读视图，实际: {err}"
    );

    drop(snapshot);

    // 反向验证：经快照的写入确实没有发生
    let after = db.run_cypher("MATCH (n:X) RETURN count(*)")?;
    assert_eq!(
        after.rows[0].values[0],
        Value::Int(0),
        "被拒绝的写入不得留下任何数据"
    );

    Ok(())
}

/// 快照内的完整性检查在并发写入下必须始终健康。
///
/// `integrity_check` 在一次读锁内完成整个检查，因此它描述的是一个真实存在过的
/// 状态。这条测试把「引擎自身在压力下是自洽的」变成可执行的断言。
#[test]
fn test_integrity_check_stays_healthy_under_concurrent_writes() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join("integrity_load.db"))?;

    let hub = db.add_node(HashSet::new(), HashMap::new())?;

    let stop = Arc::new(AtomicBool::new(false));
    let rounds = Arc::new(AtomicUsize::new(0));
    let problems = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

    let writer = {
        let db = db.clone();
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if let Ok(id) = db.add_node(HashSet::new(), HashMap::new()) {
                    if let Ok(eid) = db.add_edge(id, hub, "R", HashMap::new(), 1.0) {
                        let _ = db.remove_edge(eid);
                    }
                }
            }
        })
    };

    let checker = {
        let db = db.clone();
        let stop = Arc::clone(&stop);
        let rounds = Arc::clone(&rounds);
        let problems = Arc::clone(&problems);
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let snapshot = db.read_snapshot();
                match snapshot.integrity_check() {
                    Ok(report) => {
                        if !report.is_ok() {
                            problems.lock().unwrap().push(format!(
                                "完整性检查发现 {} 个问题，首个: {:?}",
                                report.issues.len(),
                                report.issues.first()
                            ));
                        }
                    }
                    Err(e) => problems.lock().unwrap().push(format!("检查失败: {e}")),
                }
                rounds.fetch_add(1, Ordering::Relaxed);
            }
        })
    };

    thread::sleep(std::time::Duration::from_millis(3000));
    stop.store(true, Ordering::Relaxed);
    let _ = writer.join();
    let _ = checker.join();

    let rounds = rounds.load(Ordering::Relaxed);
    assert!(rounds > 0, "检查线程未执行，测试无效");

    let problems = problems.lock().unwrap();
    assert!(
        problems.is_empty(),
        "并发写入下完整性检查出现 {} 次问题（共 {rounds} 轮），首个: {}",
        problems.len(),
        problems.first().map(String::as_str).unwrap_or("(无)")
    );

    Ok(())
}

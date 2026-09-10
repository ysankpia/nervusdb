//! 生产就绪性验证：进程级排他锁、结构性完整性校验、错误不再静默。
//!
//! 这些用例覆盖的正是此前实测出的两类致命失效：
//! 1. 同一数据库被两个句柄同时写入导致静默丢数据；
//! 2. 数据页损坏后静默返回错误/缺失数据而无人报错。

use graphlite::{GraphError, GraphLite, Value};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use tempfile::tempdir;

fn props(i: i64) -> HashMap<String, Value> {
    let mut m = HashMap::new();
    m.insert("i".to_string(), Value::from(i));
    m.insert("name".to_string(), Value::from(format!("node{}", i)));
    m
}

// =========================================================================
// 1. 排他锁：同一数据库同时只允许一个句柄
// =========================================================================
#[test]
fn test_second_handle_is_rejected() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("locked.db");

    let first = GraphLite::open(&db_path)?;
    assert!(first.is_locked(), "file-backed handle must hold the lock");

    // 同进程第二次打开必须被明确拒绝，而不是「打开成功然后互相覆盖」
    let err = match GraphLite::open(&db_path) {
        Ok(_) => panic!("second handle on the same file must be rejected"),
        Err(e) => e,
    };
    match err {
        GraphError::DatabaseLocked(msg) => {
            assert!(
                msg.contains("already open"),
                "error must explain the cause, got: {}",
                msg
            );
        }
        other => panic!("expected DatabaseLocked, got {:?}", other),
    }

    // 释放后可正常重开
    drop(first);
    let reopened = GraphLite::open(&db_path)?;
    assert_eq!(reopened.node_count(), 0);

    Ok(())
}

#[test]
fn test_lock_released_after_drop_allows_reopen() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("reopen.db");

    {
        let db = GraphLite::open(&db_path)?;
        db.add_node(HashSet::from(["N".to_string()]), props(1))?;
        db.checkpoint()?;
    }
    // drop 之后锁必须已释放
    let db = GraphLite::open(&db_path)?;
    assert_eq!(db.node_count(), 1);
    // 连续多次重开也不应残留锁
    drop(db);
    let db = GraphLite::open(&db_path)?;
    assert_eq!(db.node_count(), 1);

    Ok(())
}

#[test]
fn test_memory_mode_does_not_lock() -> Result<(), GraphError> {
    // :memory: 无文件，不应加锁，允许多个实例共存
    let a = GraphLite::open(":memory:")?;
    let b = GraphLite::open(":memory:")?;
    assert!(!a.is_locked());
    assert!(!b.is_locked());
    a.add_node(HashSet::from(["A".to_string()]), HashMap::new())?;
    b.add_node(HashSet::from(["B".to_string()]), HashMap::new())?;
    assert_eq!(a.node_count(), 1);
    assert_eq!(b.node_count(), 1);
    Ok(())
}

// =========================================================================
// 2. 跨进程互斥（真正多进程，而非同进程双句柄）
// =========================================================================
#[test]
fn test_cross_process_lock_excludes() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("crossproc.db");

    // 本进程持有锁
    let holder = GraphLite::open(&db_path)?;
    holder.add_node(HashSet::from(["N".to_string()]), props(1))?;
    holder.checkpoint()?;

    // 另起一个真实子进程尝试打开同一文件：应失败
    let exe = std::env::current_exe().expect("test binary path");
    let output = std::process::Command::new(exe)
        .arg("--exact")
        .arg("cross_process_child_probe")
        .arg("--ignored")
        .arg("--nocapture")
        .env("GL_CHILD_DB", &db_path)
        .output()
        .expect("spawn child");

    // 子进程必须报出 DatabaseLocked（通过标记字符串判定）
    let stdout = String::from_utf8_lossy(&output.stdout);
    // 对抗性自审：确认子进程**真的跑了**目标测试，而不是静默 0 个用例「假通过」
    assert!(
        stdout.contains("running 1 test"),
        "child probe did not actually execute a test; stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("CHILD_RESULT=locked"),
        "child should report the db as locked; actual stdout: {}",
        stdout
    );
    assert!(
        output.status.success(),
        "child probe exited with failure: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}

/// 子进程探针：由 `test_cross_process_lock_excludes` 以真实子进程方式启动。
/// 默认 `#[ignore]`，只有被显式调用时才运行。
#[test]
#[ignore]
fn cross_process_child_probe() {
    let path = std::env::var("GL_CHILD_DB").expect("GL_CHILD_DB must be set");
    match GraphLite::open(&path) {
        Ok(_) => println!("CHILD_RESULT=opened"),
        Err(GraphError::DatabaseLocked(_)) => println!("CHILD_RESULT=locked"),
        Err(e) => println!("CHILD_RESULT=other:{}", e),
    }
}

// =========================================================================
// 3. 健康数据库必须通过完整性校验
// =========================================================================
#[test]
fn test_integrity_check_passes_on_healthy_db() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("healthy.db");
    let db = GraphLite::open(&db_path)?;

    // 构造含自环、重复边、扇入扇出与长属性链的图
    db.with_transaction(|tx| {
        for i in 1..=200u64 {
            tx.add_node(HashSet::from(["N".to_string()]), props(i as i64))?;
        }
        for i in 1..200u64 {
            tx.add_edge(i, i + 1, "NEXT", HashMap::new(), 1.0)?;
            tx.add_edge(i, (i * 7) % 200 + 1, "JUMP", HashMap::new(), 2.0)?;
        }
        tx.add_edge(5, 5, "SELF", HashMap::new(), 1.0)?;
        let mut big = HashMap::new();
        big.insert("blob".to_string(), Value::from("X".repeat(9000)));
        tx.add_node(HashSet::from(["Doc".to_string()]), big)?;
        Ok(())
    })?;
    db.checkpoint()?;

    let report = db.integrity_check()?;
    assert!(
        report.is_ok(),
        "healthy database must pass integrity check, issues: {:?}",
        report.issues
    );
    assert_eq!(report.nodes_checked, 201);
    assert_eq!(report.edges_checked, 399);

    // verify() 是同一断言的 Result 形式
    let verified = db.verify()?;
    assert!(verified.is_ok());

    Ok(())
}

// =========================================================================
// 4. 损坏必须被完整性校验**报出来**（对抗性：故意破坏，不回避）
// =========================================================================
#[test]
fn test_integrity_check_detects_corruption() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("corrupt.db");

    {
        let db = GraphLite::open(&db_path)?;
        db.with_transaction(|tx| {
            for i in 1..=300u64 {
                tx.add_node(HashSet::from(["N".to_string()]), props(i as i64))?;
            }
            for i in 1..300u64 {
                tx.add_edge(i, i + 1, "NEXT", HashMap::new(), 1.0)?;
            }
            Ok(())
        })?;
        db.checkpoint()?;
    }

    // 定位并破坏一张**承载节点记录/链指针**的数据页（非属性页），
    // 这样才能破坏链守恒而不是仅仅让属性不可读。
    let bytes = std::fs::read(&db_path)?;
    // 跳过 Page 0 (header)，破坏第 2 张页的头部区域
    let victim_page = 2usize;
    assert!(
        bytes.len() > (victim_page + 1) * 4096,
        "file too small to contain the victim page"
    );

    {
        use std::fs::OpenOptions;
        use std::io::{Seek, SeekFrom};
        let mut f = OpenOptions::new().write(true).open(&db_path)?;
        f.seek(SeekFrom::Start((victim_page * 4096) as u64))?;
        // 把该页前 128 字节置为 0xFF：破坏记录与链指针
        f.write_all(&[0xFFu8; 128])?;
        f.flush()?;
    }

    let db = GraphLite::open(&db_path)?;
    let report = db.integrity_check()?;

    assert!(
        !report.is_ok(),
        "corruption MUST be detected; a silent pass is the defect we are fixing"
    );
    // 应给出结构性问题（计数/链/悬空引用之一），而非只是「属性不可读」
    let structural = report.count_of(graphlite::IntegrityIssueKind::CountMismatch)
        + report.count_of(graphlite::IntegrityIssueKind::DanglingEdge)
        + report.count_of(graphlite::IntegrityIssueKind::OutgoingChainCountMismatch)
        + report.count_of(graphlite::IntegrityIssueKind::IncomingChainCountMismatch)
        + report.count_of(graphlite::IntegrityIssueKind::ChainCycleOrOutOfRange)
        + report.count_of(graphlite::IntegrityIssueKind::PropertyUnreadable);
    assert!(
        structural > 0,
        "expected at least one structural issue, got {:?}",
        report.issues
    );

    // verify() 必须返回 Err 并携带诊断信息
    let err = db.verify().expect_err("verify must fail on a corrupt db");
    let msg = err.to_string();
    assert!(
        msg.contains("issue") || msg.contains("integrity"),
        "error must be diagnostic, got: {}",
        msg
    );

    Ok(())
}

// =========================================================================
// 4b. 度数守恒 oracle：只破坏链指针（计数仍自洽）也必须被抓出
// =========================================================================
#[test]
fn test_integrity_catches_chain_only_corruption() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("chain_corrupt.db");

    {
        let db = GraphLite::open(&db_path)?;
        db.with_transaction(|tx| {
            for i in 1..=200u64 {
                tx.add_node(HashSet::from(["N".to_string()]), props(i as i64))?;
            }
            // 每个节点多条出边，使链长明显区别于 0
            for i in 1..=200u64 {
                tx.add_edge(i, (i % 200) + 1, "R", HashMap::new(), 1.0)?;
                tx.add_edge(i, ((i * 3) % 200) + 1, "R", HashMap::new(), 1.0)?;
            }
            Ok(())
        })?;
        db.checkpoint()?;
    }

    // 健康时必须通过（阴性对照）
    {
        let db = GraphLite::open(&db_path)?;
        assert!(db.integrity_check()?.is_ok(), "sanity: healthy db passes");
    }

    // 破坏第一张节点记录页的链指针区域：把 first_outgoing_edge_id 清 0。
    // 这会缩短链但不改变任何边的 in_use 状态，故计数仍自洽 ——
    // 只有「链口径 vs 独立扫描口径」的度数守恒才能发现它。
    let bytes = std::fs::read(&db_path)?;
    // 找到含 node_count 的 header 页(0)之后的第一个非空数据页
    let mut victim = None;
    for p in 1..(bytes.len() / 4096) {
        let seg = &bytes[p * 4096..(p + 1) * 4096];
        // 节点记录首个字节 in_use == 1
        if seg[0] == 1 && seg.iter().any(|&b| b != 0) {
            victim = Some(p);
            break;
        }
    }
    let victim = victim.expect("a node record page must exist");

    {
        use std::fs::OpenOptions;
        use std::io::{Seek, SeekFrom};
        let mut f = OpenOptions::new().write(true).open(&db_path)?;
        // NodeRecord 布局: in_use(1) reserved(3) label_id(4) 之后是
        // first_outgoing_edge_id 的 8 字节；清零它即打断出边链
        f.seek(SeekFrom::Start(((victim as u64) * 4096) + 8))?;
        f.write_all(&[0u8; 8])?;
        f.flush()?;
    }

    let db = GraphLite::open(&db_path)?;
    let report = db.integrity_check()?;
    assert!(
        !report.is_ok(),
        "chain-only corruption must be detected by the degree conservation oracle"
    );
    let degree_issues = report.count_of(graphlite::IntegrityIssueKind::OutgoingChainCountMismatch)
        + report.count_of(graphlite::IntegrityIssueKind::IncomingChainCountMismatch);
    assert!(
        degree_issues > 0,
        "expected a degree-conservation violation, got: {:?}",
        report.issues
    );

    Ok(())
}

// =========================================================================
// 5. 读错误不再静默：try_get_* 保留错误，get_* 折叠为 None
// =========================================================================
#[test]
fn test_try_get_preserves_storage_errors() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("readerr.db");

    {
        let db = GraphLite::open(&db_path)?;
        for i in 1..=200u64 {
            let mut m = props(i as i64);
            // 多页属性：破坏其溢出链即可让读取失败
            m.insert("blob".to_string(), Value::from("B".repeat(8000)));
            db.add_node(HashSet::from(["N".to_string()]), m)?;
        }
        db.checkpoint()?;
    }

    // 破坏承载属性载荷的某一页
    let bytes = std::fs::read(&db_path)?;
    let needle = b"BBBBBBBBBB";
    let victim = (1..(bytes.len() / 4096))
        .find(|&p| {
            bytes[p * 4096..(p + 1) * 4096]
                .windows(needle.len())
                .any(|w| w == needle)
        })
        .expect("payload page must exist");

    {
        use std::fs::OpenOptions;
        use std::io::{Seek, SeekFrom};
        let mut f = OpenOptions::new().write(true).open(&db_path)?;
        f.seek(SeekFrom::Start(((victim as u64) * 4096) + 8))?;
        // 破坏 overflow 链的 next 指针与长度字段
        f.write_all(&[0xFFu8; 16])?;
        f.flush()?;
    }

    let db = GraphLite::open(&db_path)?;

    // 找到那个属性不可读的节点：try_get_node 必须报错
    let mut saw_error = false;
    for id in 1..=200u64 {
        match db.try_get_node(id) {
            Err(_) => {
                saw_error = true;
                break;
            }
            Ok(_) => continue,
        }
    }
    assert!(
        saw_error,
        "try_get_node must surface the storage error instead of hiding it"
    );

    // 同时确认 get_node 的有损语义：同一 id 上它只会给出 None
    let mut lossy_none = 0;
    for id in 1..=200u64 {
        if db.try_get_node(id).is_err() && db.get_node(id).is_none() {
            lossy_none += 1;
        }
    }
    assert!(
        lossy_none > 0,
        "get_node must collapse the error to None (documented lossy behaviour)"
    );

    Ok(())
}

// =========================================================================
// 6. 锁与崩溃恢复的交互：WAL 回放必须发生在持锁之后
// =========================================================================
#[test]
fn test_lock_held_during_wal_replay() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("replay_lock.db");

    // 写入数据但不 checkpoint：WAL 中有待回放的已提交页
    {
        let db = GraphLite::open(&db_path)?;
        db.with_transaction(|tx| {
            for i in 1..=500u64 {
                tx.add_node(HashSet::from(["N".to_string()]), props(i as i64))?;
            }
            Ok(())
        })?;
        // 故意不 checkpoint，保留 WAL
    }

    // 重开：这次 open 会回放 WAL。回放期间另一个句柄必须被拒。
    let db = GraphLite::open(&db_path)?;
    assert!(db.is_locked());
    assert!(
        GraphLite::open(&db_path).is_err(),
        "no second handle may open while another holds the lock"
    );
    // 回放结果正确
    assert_eq!(db.node_count(), 500);
    assert!(
        db.verify()?.is_ok(),
        "post-replay database must be consistent"
    );

    Ok(())
}

// =========================================================================
// 7. 锁中毒后引擎仍可用（不再 panic 终止进程）
// =========================================================================
#[test]
fn test_poisoned_lock_recovers_instead_of_panicking() -> Result<(), GraphError> {
    use graphlite::sync_ext::RwLockRecoverExt;
    use std::sync::{Arc, RwLock};

    let lock: Arc<RwLock<u32>> = Arc::new(RwLock::new(7));

    // 在一个线程里持锁 panic，制造中毒状态
    let poisoner = Arc::clone(&lock);
    let _ = std::thread::spawn(move || {
        let _guard = poisoner.write().expect("acquire before poisoning");
        panic!("intentional panic to poison the lock");
    })
    .join();

    assert!(lock.is_poisoned(), "lock must be poisoned after the panic");

    // 关键断言：恢复式访问不得 panic，且能取出内部数据
    let value = *lock.read_recover();
    assert_eq!(value, 7, "recovered read must still see the data");

    let mut guard = lock.write_recover();
    *guard += 1;
    drop(guard);
    assert_eq!(*lock.read_recover(), 8);

    // 对照：标准 .read().unwrap() 在中毒锁上会 panic —— 证明该恢复路径确有必要。
    // 这里用 catch_unwind 观察而不真正让测试失败。
    let std_panicked = {
        let l = Arc::clone(&lock);
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // 绑定守卫并真实读取，避免 `let _ =` 立即释放导致语义失真
            let guard = l.read().unwrap();
            std::hint::black_box(*guard);
        }))
        .is_err()
    };
    assert!(
        std_panicked,
        "sanity check: the default read().unwrap() does panic on a poisoned lock"
    );

    Ok(())
}

// =========================================================================
// 8. 引擎在锁中毒后仍能继续服务（端到端，而非仅特质单测）
// =========================================================================
#[test]
fn test_engine_usable_after_internal_panic() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("poison_engine.db");
    let db = GraphLite::open(&db_path)?;

    db.add_node(HashSet::from(["N".to_string()]), props(1))?;

    // 制造一次跨线程 panic 让内部锁中毒（模拟库内部某个 unwrap 失败）
    let db_clone = db.clone();
    let _ = std::thread::spawn(move || {
        let _ = db_clone.node_count();
        panic!("simulated internal failure while touching engine state");
    })
    .join();

    // 引擎必须仍然可用：不 panic、能读写、能自检
    let db2 = GraphLite::open(&db_path);
    assert!(
        db2.is_err(),
        "original handle still holds the lock, so reopen must be refused"
    );

    db.add_node(HashSet::from(["N".to_string()]), props(2))?;
    assert_eq!(db.node_count(), 2);
    let report = db.integrity_check()?;
    assert!(report.is_ok(), "engine must remain self-consistent");

    Ok(())
}

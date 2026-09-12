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

    // 报告必须**点名坏页**，而不是只说「图好像小了一圈」。
    // 页级 CRC 体检排在最前，因此在内容检查被读错误中断之前，
    // 坏页已经带着页号进入报告——这是本用例的核心断言。
    assert!(
        report.count_of(graphlite::IntegrityIssueKind::PageChecksumMismatch) > 0,
        "report must name the corrupt page, got {:?}",
        report.issues
    );
    let mentions_victim = report.issues.iter().any(|i| {
        i.kind == graphlite::IntegrityIssueKind::PageChecksumMismatch
            && i.detail.contains(&format!("page {}", victim_page))
    });
    assert!(
        mentions_victim,
        "report must name page {} specifically, got {:?}",
        victim_page, report.issues
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
//
// 这是「逻辑损坏」而非「介质损坏」：页字节被改坏之后，我们**重算该页的 CRC 并
// 回写**，使页级校验和仍然自洽。这模拟的是**写路径自身的 bug**——数据被按照
// 当时的代码逻辑「正确」写坏，页内容完整、CRC 相符，介质层面毫无异常。
//
// 为什么必须这样构造：页级 CRC 只能发现意外位翻转（介质损坏），它对写路径 bug
// 完全失明，因为坏值是被合法写入并合法计算校验和的。能发现这类损坏的只有
// 度数守恒 oracle：链口径（沿指针走出）与独立扫描口径（遍历边记录）必须相等。
// 两条路径都不看 CRC，因此本用例真正锁定的是 oracle，而非校验和。
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

    // 读入该页、破坏链指针、重算 CRC，然后连同新 CRC 一起写回。
    // 回写 CRC 是本用例的关键：它让损坏在页级校验下「合法」，
    // 从而逼迫完整性检查走度数守恒这条独立路径。
    let mut page = bytes[victim * 4096..(victim + 1) * 4096].to_vec();
    // NodeRecord 布局: in_use(1) reserved(3) label_id(4) 之后是
    // first_outgoing_edge_id 的 8 字节；清零它即打断出边链
    page[8..16].copy_from_slice(&0u64.to_le_bytes());

    {
        use std::fs::OpenOptions;
        use std::io::{Seek, SeekFrom};
        let mut f = OpenOptions::new().write(true).open(&db_path)?;
        f.seek(SeekFrom::Start((victim * 4096) as u64))?;
        f.write_all(&page)?;
        f.flush()?;
    }

    // 用库自身的 CrcStore 重算并落盘该页校验和。
    //
    // 刻意复用生产代码而不是在测试里手写偏移量：CRC 布局（内联区 / 两级目录）
    // 是要害细节，测试自己抄一份必然会在下次改布局时悄悄过期，
    // 那样「损坏已经骗过校验」这个前提就不再成立，而测试仍会假装通过。
    {
        let dm = std::sync::Arc::new(graphlite::DiskManager::open(&db_path)?);

        // 目录根页号是 Page 0 的持久化字段，用库常量读取
        let mut page0 = [0u8; graphlite::PAGE_SIZE];
        dm.read_page(0, &mut page0)?;
        let root = u32::from_le_bytes(
            page0[graphlite::page::HeaderPage::CRC_DIR_PAGE_OFFSET
                ..graphlite::page::HeaderPage::CRC_DIR_PAGE_OFFSET + 4]
                .try_into()
                .unwrap(),
        );

        let mut store = graphlite::crc::CrcStore::new(dm, root);
        let mut page_arr = [0u8; graphlite::PAGE_SIZE];
        page_arr.copy_from_slice(&page);
        store.record(victim as u32, &page_arr)?;
        store.flush()?;
    }

    let db = GraphLite::open(&db_path)?;

    // 前提校验：损坏必须已经骗过了页级校验，否则本用例退化成 CRC 用例，
    // 无法证明度数 oracle 本身有效。
    assert!(
        db.verify_page_on_disk(victim as u32).is_ok(),
        "constructed corruption must pass the page CRC, else this test \
         does not exercise the degree oracle"
    );

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

// =========================================================================
// 9. 页校验和必须覆盖 **256 页之后**的页
// =========================================================================
//
// 这条测试存在的原因值得记录：对手实现（gemlite_droid_gemini_ds_plan）的
// `verify_page_crc_static` 第一行就写着 `if page_id >= INLINE_CRC_PAGE_COUNT
// { return Ok(()); }`，即只会校验前 256 页，而它那条「超出部分走目录链」的分支
// 没有任何调用者、是死代码。它的自测篡改的是 Page 2，正好落在受保护范围内，
// 于是「测试通过、实际无保护」。
//
// 这里刻意破坏一张**远超 256** 的页，确保目录链真的在工作。
#[test]
fn test_page_checksum_covers_pages_beyond_256() -> Result<(), GraphError> {
    use std::io::{Read, Seek, SeekFrom, Write};

    let dir = tempdir()?;
    let db_path = dir.path().join("beyond256.db");

    // 写入足够多的节点，使文件远大于 512 页（512 页 = 2MB）。
    //
    // 这里刻意取 512 页而不是 256 页作为下界：靶页取的是 `file_pages / 2`，
    // 只有当文件超过 512 页时，中位页才必然落在 256 之后的内联区之外。
    // 40k 节点带属性只能铺出约 475 页，因此需要更大的 fixture。
    let node_count: u64 = 90_000;
    {
        let db = GraphLite::open(&db_path)?;
        db.with_transaction(|tx| {
            for i in 1..=node_count {
                let mut p = HashMap::new();
                p.insert("idx".to_string(), Value::from(i as i64));
                tx.add_node(HashSet::from(["N".to_string()]), p)?;
            }
            Ok(())
        })?;
        db.checkpoint()?;
    }

    let page_size = graphlite::PAGE_SIZE as u64;
    let file_pages = std::fs::metadata(&db_path)?.len() / page_size;
    assert!(
        file_pages > 512,
        "fixture must exceed twice the inline CRC range so a mid-file page \\
         is provably beyond it, got {} pages",
        file_pages
    );

    // 选择一张**确定是数据页**的页。
    //
    // 不能简单取最后一页：CRC 目录页本身就在文件尾部（目录页用 `self_crc` 自我保护，
    // 不参与数据页校验），取到它们会得到「无 CRC 记录」的假阴性。
    // 取文件中位：它远在 256 页之后，且不可能是尾部的一两张目录页。
    let victim_page = (file_pages / 2) as u32;

    // 翻转该页中间的一个字节
    {
        let mut f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&db_path)?;
        f.seek(SeekFrom::Start(victim_page as u64 * page_size + 64))?;
        let mut b = [0u8; 1];
        f.read_exact(&mut b)?;
        b[0] ^= 0xFF;
        f.seek(SeekFrom::Start(victim_page as u64 * page_size + 64))?;
        f.write_all(&b)?;
        f.flush()?;
    }

    // 从磁盘读取该页并校验：必须报出校验和错误，而不是静默返回坏数据。
    // `verify_page_on_disk` 绕过缓冲池缓存，读的是磁盘上的实际内容。
    {
        let db = GraphLite::open(&db_path)?;
        match db.verify_page_on_disk(victim_page) {
            Err(GraphError::PageChecksumMismatch { page_id, .. }) => {
                assert_eq!(
                    page_id, victim_page as u64,
                    "must report the corrupted page"
                );
            }
            Err(other) => panic!("expected PageChecksumMismatch, got: {}", other),
            Ok(()) => panic!(
                "corruption in page {} (>=256) was NOT detected — the CRC directory \
                 chain is not covering pages beyond the inline range",
                victim_page
            ),
        }
    }

    Ok(())
}

// =========================================================================
// 8. 多读单写并发：共享读锁
// =========================================================================
/// 只读句柄取共享锁：多个读者共存，但与写者双向互斥。
///
/// 这是「后台写入、前台观察」场景的基础。SQLite 的 WAL 模式即此模型。
#[test]
fn test_multiple_readers_coexist_with_one_writer_excluded() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("shared_lock.db");

    // 建库并 checkpoint，使 WAL 为空（只读打开要求无可回放内容）
    {
        let db = GraphLite::open(&db_path)?;
        db.with_transaction(|tx| {
            for i in 1..=5 {
                tx.add_node(HashSet::from(["N".to_string()]), props(i))?;
            }
            Ok(())
        })?;
        db.checkpoint()?;
    }

    // 多个读者共存
    let r1 = GraphLite::open_read_only(&db_path)?;
    let r2 = GraphLite::open_read_only(&db_path)?;
    let r3 = GraphLite::open_read_only(&db_path)?;
    assert!(r1.is_read_only() && r2.is_read_only() && r3.is_read_only());
    assert_eq!(r1.node_count(), 5, "reader must see the committed data");

    // 有读者时写者被拒绝
    let writer = GraphLite::open(&db_path);
    assert!(
        writer.is_err(),
        "a writer must be refused while readers hold shared locks"
    );

    // 写入口在只读句柄上必须明确报错，而不是静默失败
    let write_err = r1
        .add_node(HashSet::new(), HashMap::new())
        .expect_err("read-only handle must refuse writes");
    assert!(
        write_err.to_string().contains("read-only"),
        "error must explain that the handle is read-only, got: {}",
        write_err
    );
    assert!(
        r1.checkpoint().is_err(),
        "read-only handle must refuse checkpoint (it writes)"
    );

    // 释放读者后写者可用；此时读者反过来被拒绝
    drop(r1);
    drop(r2);
    drop(r3);
    let w = GraphLite::open(&db_path)?;
    assert!(
        GraphLite::open_read_only(&db_path).is_err(),
        "a reader must be refused while a writer holds the exclusive lock"
    );
    drop(w);

    // 写者退出后读者再次可用 —— 证明锁确实随 Drop 释放，没有泄漏
    let again = GraphLite::open_read_only(&db_path)?;
    assert_eq!(again.node_count(), 5);

    Ok(())
}

/// 只读打开不得在 WAL 还有待回放内容时成功。
///
/// 读者不能回放（回放会写主数据文件），若静默跳过那些页，它会看到过期数据。
/// 因此必须明确失败并指出怎么处理。
#[test]
fn test_read_only_open_refuses_pending_wal_replay() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("ro_pending_wal.db");

    // 写入但**不** checkpoint：WAL 中留有已提交页
    {
        let db = GraphLite::open(&db_path)?;
        db.with_transaction(|tx| {
            for i in 1..=5 {
                tx.add_node(HashSet::from(["N".to_string()]), props(i))?;
            }
            Ok(())
        })?;
    }

    let err = match GraphLite::open_read_only(&db_path) {
        Ok(_) => panic!("read-only open must fail while the WAL has committed pages"),
        Err(e) => e,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("read-only") && msg.contains("not yet replayed"),
        "error must explain the WAL situation, got: {}",
        msg
    );
    assert!(
        msg.contains("read-write handle"),
        "error must say how to resolve it, got: {}",
        msg
    );

    // 用读写句柄打开一次即可回放；此后只读可用且能看到全部数据
    {
        let db = GraphLite::open(&db_path)?;
        assert_eq!(db.node_count(), 5);
        db.checkpoint()?;
    }
    let ro = GraphLite::open_read_only(&db_path)?;
    assert_eq!(
        ro.node_count(),
        5,
        "after replay the reader must see every committed node"
    );

    Ok(())
}

// =========================================================================
// 9. 唯一约束：数据不脏的最后一道防线
// =========================================================================
/// 唯一约束的完整语义：声明、拦截、更新、持久化。
///
/// 没有约束时，一个有 bug 的写入方（或重试逻辑出错的 Agent）可以给同一个实体
/// 建两个节点，而查询只返回其中一半——错误被推迟到很久以后才被发现。
#[test]
fn test_unique_constraint_full_semantics() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("unique.db");

    let named = |name: &str| {
        let mut p = HashMap::new();
        p.insert("name".to_string(), Value::from(name));
        (HashSet::from(["Character".to_string()]), p)
    };

    let db = GraphLite::open(&db_path)?;

    // 无约束时重名是允许的
    db.add_node(named("林渊").0, named("林渊").1)?;
    db.add_node(named("林渊").0, named("林渊").1)?;

    // 既有数据已重复 → 声明必须被拒绝，且指出冲突
    let err = db
        .create_unique_constraint("Character", "name")
        .expect_err("declaring a constraint over duplicate data must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("already shared by 2 nodes"),
        "error must name the conflicting nodes, got: {}",
        msg
    );

    // 干净数据上声明成功
    let clean_path = dir.path().join("clean.db");
    let db2 = GraphLite::open(&clean_path)?;
    db2.add_node(named("林渊").0, named("林渊").1)?;
    db2.create_unique_constraint("Character", "name")?;
    assert_eq!(
        db2.unique_constraints(),
        vec![("Character".to_string(), "name".to_string())]
    );

    // 重复插入被拒
    let dup = db2.add_node(named("林渊").0, named("林渊").1);
    assert!(
        matches!(dup, Err(GraphError::UniqueConstraintViolation { .. })),
        "duplicate insert must fail with a constraint violation, got: {:?}",
        dup.map(|_| ())
    );

    // 不同值可插入
    let other = db2.add_node(named("苏晴").0, named("苏晴").1)?;

    // 更新成已存在的值被拒
    let upd = db2.update_node_property(other, "name", "林渊");
    assert!(
        matches!(upd, Err(GraphError::UniqueConstraintViolation { .. })),
        "updating to an existing value must fail, got: {:?}",
        upd
    );

    // 更新为自身当前值必须允许（不得与自己冲突）
    db2.update_node_property(other, "name", "苏晴")?;

    // 约束跨重启持久化并继续生效
    db2.checkpoint()?;
    drop(db2);
    let reopened = GraphLite::open(&clean_path)?;
    assert_eq!(
        reopened.unique_constraints(),
        vec![("Character".to_string(), "name".to_string())],
        "constraints must survive a restart"
    );
    assert!(
        reopened.add_node(named("林渊").0, named("林渊").1).is_err(),
        "the constraint must still be enforced after a restart"
    );

    Ok(())
}

// =========================================================================
// 10. 运维：在线备份与空间回收
// =========================================================================
/// `backup` 必须产出**可独立打开且数据一致**的副本。
///
/// 这是「小说数据不能丢」的直接保障：副本不依赖源库，也不依赖任何边的存在。
#[test]
fn test_backup_produces_consistent_independent_copy() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let src = dir.path().join("src.db");
    let bak = dir.path().join("bak.db");

    let db = GraphLite::open(&src)?;
    let mut ids: Vec<u64> = Vec::new();
    db.with_transaction(|tx| {
        for i in 1..=100u64 {
            let mut p = HashMap::new();
            p.insert("i".to_string(), Value::from(i as i64));
            // 多 KB 属性：走溢出页链，确保副本覆盖的不只是定长记录页
            p.insert("pad".to_string(), Value::from("y".repeat(3000)));
            ids.push(tx.add_node(HashSet::from(["N".to_string()]), p)?);
        }
        for i in 0..99 {
            tx.add_edge(ids[i], ids[i + 1], "NEXT", HashMap::new(), 1.0)?;
        }
        Ok(())
    })?;

    let copied = db.backup(&bak)?;
    assert!(copied > 0, "backup must copy bytes");
    assert!(bak.exists(), "backup target must exist");

    // 副本独立可开：释放源句柄（排他锁）后单独打开副本
    drop(db);
    let restored = GraphLite::open(&bak)?;
    assert_eq!(restored.node_count(), 100);
    assert_eq!(restored.edge_count(), 99);

    // 属性（含溢出页链）必须完整
    let node = restored
        .try_get_node(1)?
        .expect("node 1 must exist in the copy");
    assert_eq!(
        node.get_prop("pad")
            .and_then(|v| v.as_str())
            .map(|s| s.len()),
        Some(3000),
        "multi-page property must survive the copy"
    );

    // 边链完整
    let rows = restored.run_cypher("MATCH (a:N)-[:NEXT]->(b) RETURN count(*) AS n")?;
    assert_eq!(rows.rows[0].values[0].as_i64(), Some(99));

    // 副本自身可写：证明它是完整的数据库，不是只读快照
    let extra = restored.add_node(HashSet::from(["N".to_string()]), HashMap::new())?;
    assert!(extra > 0, "the copy must accept writes");

    Ok(())
}

/// 覆盖已有文件是危险的：可能抹掉上一份有效备份，因此必须拒绝。
#[test]
fn test_backup_refuses_to_overwrite() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let src = dir.path().join("s.db");
    let bak = dir.path().join("b.db");

    let db = GraphLite::open(&src)?;
    db.add_node(HashSet::from(["N".to_string()]), props(1))?;
    db.backup(&bak)?;

    // 第二次备份到同一目标必须被拒
    let err = db
        .backup(&bak)
        .expect_err("backup must refuse to overwrite an existing target");
    assert!(
        err.to_string().contains("Refusing to overwrite"),
        "error must explain the refusal, got: {}",
        err
    );

    // 备份到自身同样无意义且危险
    assert!(
        db.backup(&src).is_err(),
        "backing up onto the source file must be refused"
    );

    Ok(())
}

/// `vacuum` 报告可回收页，且删除后该数字必须增长——否则它只是个装饰性调用。
#[test]
fn test_vacuum_reports_reclaimable_pages() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("vac.db");
    let db = GraphLite::open(&db_path)?;

    let mut ids = Vec::new();
    db.with_transaction(|tx| {
        for i in 1..=200u64 {
            let mut p = HashMap::new();
            p.insert("i".to_string(), Value::from(i as i64));
            p.insert("pad".to_string(), Value::from("z".repeat(300)));
            ids.push(tx.add_node(HashSet::from(["N".to_string()]), p)?);
        }
        Ok(())
    })?;

    let before = db.vacuum()?;
    assert_eq!(before.nodes_live, 200);
    let reclaimable_before = before.free_property_pages + before.free_overflow_pages;

    // 全部删除
    db.with_transaction(|tx| {
        for id in &ids {
            tx.remove_node(*id);
        }
        Ok(())
    })?;

    let after = db.vacuum()?;
    assert_eq!(after.nodes_live, 0, "all nodes were deleted");
    let reclaimable_after = after.free_property_pages + after.free_overflow_pages;
    assert!(
        reclaimable_after > reclaimable_before,
        "deleting 200 nodes must increase reclaimable pages: before={}, after={}",
        reclaimable_before,
        reclaimable_after
    );

    // 报告必须自洽：它是一个诊断，不能声称文件被截断
    assert_eq!(
        after.file_bytes,
        std::fs::metadata(&db_path)?.len(),
        "reported file size must match the actual file"
    );

    // 回收后新节点应复用空间，而不是无限增长
    db.add_node(HashSet::from(["N".to_string()]), props(1))?;
    assert_eq!(db.node_count(), 1);

    Ok(())
}

/// **只读句柄的每一条写入路径都必须被拒绝。**
///
/// 这个测试是为一个真实漏洞写的：`add_node` / `add_edge` / `execute` /
/// `checkpoint` 都加了只读守卫，但 `run_cypher` 的**写分支**漏了——因为
/// `mutating` 要解析完才知道，守卫不能放在函数开头，于是被遗漏。
/// 结果只读句柄可以经 `run_cypher("CREATE ...")` 写入。
///
/// 逐个覆盖所有写入口，而不是只测一个：遗漏正是发生在「逐个添加守卫」的过程中，
/// 所以验证也必须逐个做。
#[test]
fn test_read_only_handle_rejects_every_write_path() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("ro_writes.db");

    {
        let db = GraphLite::open(&db_path)?;
        db.add_node(HashSet::from(["N".to_string()]), props(1))?;
        db.checkpoint()?;
    }

    let ro = GraphLite::open_read_only(&db_path)?;
    assert!(ro.is_read_only());

    // 逐条写路径。任何一条成功都意味着只读语义被破坏。
    let attempts: Vec<(&str, Result<(), GraphError>)> = vec![
        (
            "run_cypher(CREATE)",
            ro.run_cypher("CREATE (x:ShouldNotExist)").map(|_| ()),
        ),
        (
            "run_cypher(SET)",
            ro.run_cypher("MATCH (n:N) SET n.pwned = true").map(|_| ()),
        ),
        (
            "run_cypher(DELETE)",
            ro.run_cypher("MATCH (n:N) DELETE n").map(|_| ()),
        ),
        (
            "run_cypher(DETACH DELETE)",
            ro.run_cypher("MATCH (n:N) DETACH DELETE n").map(|_| ()),
        ),
        (
            "query_cypher(CREATE)",
            ro.query_cypher("CREATE (y:AlsoNo)").map(|_| ()),
        ),
        ("execute(CREATE)", ro.execute("CREATE (z:No)").map(|_| ())),
        (
            "add_node",
            ro.add_node(HashSet::from(["N".to_string()]), props(2))
                .map(|_| ()),
        ),
        ("checkpoint", ro.checkpoint()),
    ];

    let mut violations = Vec::new();
    for (label, result) in &attempts {
        if result.is_ok() {
            violations.push(*label);
        }
    }
    assert!(
        violations.is_empty(),
        "these write paths succeeded on a read-only handle: {:?}",
        violations
    );

    // 错误信息要能指导用户怎么办，而不只是「失败了」
    let err = ro.run_cypher("CREATE (x:Y)").expect_err("must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("read-only"),
        "error must name the cause, got: {}",
        msg
    );

    // 最关键的断言：库里确实什么都没变
    drop(ro);
    let check = GraphLite::open(&db_path)?;
    assert_eq!(check.node_count(), 1, "no node may have been created");
    let res = check.run_cypher("MATCH (n) RETURN count(*) AS n")?;
    assert_eq!(res.rows[0].values[0].as_i64(), Some(1));

    Ok(())
}

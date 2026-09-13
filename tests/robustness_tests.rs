use nervusdb::page::{HeaderPage, PAGE_SIZE};
use nervusdb::{GraphError, NervusDb, NervusDbOptions, Value};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom, Write};
use tempfile::tempdir;

#[test]
fn test_file_lock_exclusive() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("exclusive.db");

    // 1. 打开第一个实例，持有排他文件锁
    let db1 = NervusDb::open(&db_path)?;

    // 2. 尝试打开第二个实例访问同一物理路径，必须被拒绝并返回 DatabaseLocked
    let db2_res = NervusDb::open(&db_path);
    assert!(
        db2_res.is_err(),
        "Second open on the same active file must fail due to exclusive file lock"
    );
    match db2_res.err().unwrap() {
        GraphError::DatabaseLocked(path) => {
            assert!(path.contains("exclusive.db"));
        }
        other => panic!("Expected DatabaseLocked error, got {:?}", other),
    }

    // 3. Drop 第一个实例，释放锁
    drop(db1);

    // 4. 第二个实例现在能够正常加锁并打开
    let db2 = NervusDb::open(&db_path)?;
    assert_eq!(db2.node_count(), 0);

    Ok(())
}

#[test]
fn test_wal_auto_checkpoint_triggers() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("auto_chk.db");

    // 设定极小的自动 checkpoint 阈值 (64KB)
    let opts = NervusDbOptions {
        buffer_pool_frames: 256,
        wal_auto_checkpoint_bytes: 64 * 1024,
        ..NervusDbOptions::default()
    };
    let db = NervusDb::open_with_options(&db_path, opts)?;

    // 持续批量写入直到 WAL 体积膨胀超过 64KB
    for batch_i in 0..15 {
        db.with_transaction(|tx| {
            for j in 0..50 {
                let mut props = HashMap::new();
                props.insert(
                    "payload".to_string(),
                    Value::from(format!(
                        "Batch_{}_Node_{}_Payload_Auto_Checkpoint",
                        batch_i, j
                    )),
                );
                tx.add_node(HashSet::from(["Item".to_string()]), props)?;
            }
            Ok(())
        })?;
    }

    // 验证自动 checkpoint 成功将主数据文件写入且 WAL 截断
    let wal_path = {
        let mut p = db_path.as_os_str().to_os_string();
        p.push(".wal");
        std::path::PathBuf::from(p)
    };

    let wal_len = if wal_path.exists() {
        std::fs::metadata(&wal_path)?.len()
    } else {
        0
    };
    println!("Auto-checkpoint WAL size after writes: {} bytes", wal_len);
    // 自动 checkpoint 截断后 WAL 文件体积应当远小于累计写入总量 (<= 64KB)
    assert!(
        wal_len <= 64 * 1024,
        "WAL size {} bytes exceeds auto checkpoint threshold",
        wal_len
    );

    Ok(())
}

#[test]
fn test_page_checksum_detects_corruption() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("corrupt_test.db");

    let node_id;
    {
        let db = NervusDb::open(&db_path)?;
        let mut props = HashMap::new();
        props.insert("name".to_string(), Value::from("ProtectedData"));
        props.insert("secret".to_string(), Value::from(999999i64));
        node_id = db.add_node(HashSet::from(["Secure".to_string()]), props)?;
        db.checkpoint()?;
    }

    // 物理篡改主文件数据页：翻转节点所在物理页的一个字节
    // Node 1 的数据页通常位于第 1 页或第 2 页
    {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&db_path)?;
        let file_len = file.metadata()?.len();
        assert!(file_len >= 8192, "Database must have at least 2 pages");

        // 篡改 Page 2（首个节点物理页）中间的某字节
        let corrupt_offset = 2 * PAGE_SIZE as u64 + 50;
        file.seek(SeekFrom::Start(corrupt_offset))?;
        let mut b = [0u8; 1];
        file.read_exact(&mut b)?;
        b[0] ^= 0xFF; // 翻转所有比特
        file.seek(SeekFrom::Start(corrupt_offset))?;
        file.write_all(&b)?;
        file.flush()?;
    }

    // 重新打开数据库并读取被损坏节点，必须被校验和拦截返回 PageChecksumMismatch。
    // 使用既有的 `try_get_node`（错误保留语义）而非另起 `get_node_result`。
    {
        let db = NervusDb::open(&db_path)?;
        let n_res = db.try_get_node(node_id);
        assert!(
            n_res.is_err(),
            "Reading from corrupted page must return error"
        );
        let err_str = n_res.err().unwrap().to_string();
        assert!(
            err_str.contains("checksum mismatch") || err_str.contains("ChecksumMismatch"),
            "Error must mention checksum mismatch, got: {}",
            err_str
        );
    }

    Ok(())
}

#[test]
fn test_version_guard_rejects_v1_v2() -> Result<(), GraphError> {
    let dir = tempdir()?;

    // 对历史上的每个版本各构造一个假文件：格式冻结后它们都必须被拒绝，
    // 且错误信息要指出「怎么迁移」而不只是「不支持」。
    //
    // 版本 4 用旧魔数 `GLDB`，5 之后用 `NVDB`——两者都要继续被**识别**，
    // 否则旧库会被当成「不是本项目的文件」，那条错误说的是「你给错文件了」，
    // 而真相是「这是旧版本格式」，会把人引向错误的方向。
    for (old_version, magic) in [(1u32, b"GLDB"), (2, b"GLDB"), (3, b"GLDB"), (4, b"GLDB")] {
        let db_path = dir.path().join(format!("old_v{}.db", old_version));

        {
            let mut f = std::fs::File::create(&db_path)?;
            let mut hdr = [0u8; PAGE_SIZE];
            hdr[0..4].copy_from_slice(magic);
            hdr[4..8].copy_from_slice(&old_version.to_le_bytes());
            hdr[HeaderPage::PAGE_SIZE_OFFSET..HeaderPage::PAGE_SIZE_OFFSET + 4]
                .copy_from_slice(&(PAGE_SIZE as u32).to_le_bytes());
            f.write_all(&hdr)?;
            f.flush()?;
        }

        let before = std::fs::read(&db_path)?;

        let open_res = NervusDb::open(&db_path);
        assert!(
            open_res.is_err(),
            "opening a version {} file must be rejected",
            old_version
        );
        let err_msg = open_res.err().unwrap().to_string();
        assert!(
            err_msg.contains(&format!("version {} is not readable", old_version)),
            "error must name the offending version, got: {}",
            err_msg
        );
        assert!(
            err_msg.contains("current format version 5"),
            "error must name the version this build speaks, got: {}",
            err_msg
        );
        // 迁移指引是这条错误存在的意义：只报「不支持」会让用户无处下手。
        assert!(
            err_msg.contains("dump") && err_msg.contains("re-import"),
            "error must explain the migration path, got: {}",
            err_msg
        );

        // 关键断言：拒绝必须发生在**任何写入之前**。
        // 该检查位于 `open_with_options` 的开头，早于 WAL 回放——若顺序反了，
        // 回放会用当前版本的语义写回旧文件，此时再报错只会留下被污染的库。
        let after = std::fs::read(&db_path)?;
        assert_eq!(
            before, after,
            "a rejected version {} file must not be modified by the failed open",
            old_version
        );
    }

    // 反向对照：**当前**版本必须能正常打开，否则上面的拒绝毫无意义。
    let cur_path = dir.path().join("current.db");
    {
        let db = NervusDb::open(&cur_path)?;
        db.add_node(HashSet::from(["N".to_string()]), HashMap::new())?;
        db.checkpoint()?;
    }
    let reopened = NervusDb::open(&cur_path)?;
    assert_eq!(reopened.node_count(), 1, "current version must open fine");

    Ok(())
}

/// CRC 目录在大规模 churn 下的两个真实缺陷（均由 LiveJournal 压测暴露）：
///
/// 1. **root 未落盘**：`flush()` 只在「本轮有变化」时才写 Page 0。一旦目录已经
///    建立、而本轮只是重复写已存在的页，`root_changed` 为假，Page 0 就完全不写，
///    磁盘上的 root 保持为 0 —— 目录链在下次打开时整条不可读。
/// 2. **淘汰未封存**：目录页被淘汰落盘时若不算 `self_crc`，它带的是上一次封存的
///    校验和，而内容已经变了，重新载入即报「directory page is corrupt」。
///
/// 两者都只在「目录页数量超过缓存容量（64）」并反复写同一批页时才出现，
/// 小规模用例覆盖不到，因此这里直接驱动 `CrcStore`。
#[test]
fn test_crc_directory_survives_churn_beyond_cache() -> Result<(), GraphError> {
    use nervusdb::crc::CrcStore;
    use nervusdb::{DiskManager, PAGE_SIZE};
    use std::sync::Arc;

    let dir = tempdir()?;
    let db_path = dir.path().join("crc_churn.db");
    let dm = Arc::new(DiskManager::open(&db_path)?);

    // 目录区跨度取 30 万页：远超 CRC_CACHE_CAPACITY(64) 张目录页，
    // 保证执行过程中必然发生「脏目录页被淘汰」。
    const SPAN: u32 = 300_000;
    for _ in 0..(SPAN + 4096) {
        dm.allocate_page()?;
    }
    let data = [0x5Au8; PAGE_SIZE];

    // 第一轮：建立目录并落盘
    let mut store = CrcStore::new(Arc::clone(&dm), u32::MAX);
    for pid in 256..(256 + 60_000) {
        store.record(pid, &data)?;
    }
    store.flush()?;
    let root = store.root();
    assert!(root != u32::MAX && root != 0, "root must be allocated");

    // 第二轮：重写**已落盘**的早先区间（同一张目录页再次变脏），
    // 同时推进到新的高地址区，逼迫缓存淘汰脏目录页。
    for pid in 256..(256 + SPAN) {
        store.record(pid, &data)?;
    }
    store.flush()?;

    // 缺陷 1：root 必须真的落到 Page 0
    let mut page0 = [0u8; PAGE_SIZE];
    dm.read_page(0, &mut page0)?;
    let disk_root = u32::from_le_bytes(
        page0[nervusdb::page::HeaderPage::CRC_DIR_PAGE_OFFSET
            ..nervusdb::page::HeaderPage::CRC_DIR_PAGE_OFFSET + 4]
            .try_into()
            .unwrap(),
    );
    assert_eq!(
        disk_root, root,
        "crc_dir_root must persist to Page 0 even when this round changed nothing new"
    );

    // 缺陷 2：冷启动后沿目录读回校验和，任何未封存的目录页都会在此报损坏
    let mut reopened = CrcStore::new(Arc::clone(&dm), disk_root);
    for pid in (256..256 + SPAN).step_by(9973) {
        reopened
            .verify(pid, &data)
            .map_err(|e| GraphError::General(format!("page {}: {}", pid, e)))?;
    }

    Ok(())
}

#[test]
fn test_batch_memory_cap_chunking() -> Result<(), GraphError> {
    use nervusdb::disk_graph::MAX_BATCH_EDGES_IN_MEMORY;

    let db = NervusDb::open(":memory:")?;

    // 真正跨越切分阈值：写入量必须**大于**单次织网上限，否则本用例
    // 只是在验证一次普通批量织网，根本不会进入分块路径。
    let total = MAX_BATCH_EDGES_IN_MEMORY + 5_000;
    // 用少量节点、大量共享边，使切分点落在同一个节点的边序列中间 —— 这是
    // 最能暴露「分块导致链断裂」的形态：后一子段必须正确接上前一子段写下的链头，
    // 而不是接回批次前的旧链头。
    db.with_transaction(|tx| {
        let hubs: Vec<u64> = (0..4)
            .map(|_| tx.add_node(HashSet::new(), HashMap::new()))
            .collect::<Result<_, _>>()?;
        for i in 0..total {
            let src = hubs[i % hubs.len()];
            let dst = hubs[(i + 1) % hubs.len()];
            tx.add_edge(src, dst, "CONNECT", HashMap::new(), (i as f64) + 1.0)?;
        }
        Ok(())
    })?;

    assert_eq!(db.node_count(), 4);
    assert_eq!(db.edge_count(), total, "all edges across chunks must land");

    // 分块不能丢边、不能重复、不能错位：边 id 在 add_edge 时已单调分配，
    // 因此 id 空间必须恰好是 1..=total 的排列，且每条的端点与写入顺序对得上。
    for eid in 1..=total as u64 {
        let edge = db
            .try_get_edge(eid)?
            .expect("every allocated edge id exists");
        // 第 i 条(0 基)的 src 是 hubs[i % 4]，即节点 id (i % 4) + 1
        let i = (eid - 1) as usize;
        assert_eq!(edge.src_id, (i % 4) as u64 + 1, "edge {} src mismatch", eid);
        assert_eq!(
            edge.dst_id,
            ((i + 1) % 4) as u64 + 1,
            "edge {} dst mismatch",
            eid
        );
    }

    // 链完整性 —— 本用例最关键的一项。
    //
    // 跨块拼接出错的典型症状是「边都在、但链断了」：后一子段若把链头接回
    // 批次前的旧链头，而不是接上一子段写下的链头，那么「每条边都存在」的检查
    // 会全部通过，只有沿链走才发现邻接丢失。
    // `integrity_check` 的度数守恒 oracle 正是拿「沿链口径」与「独立扫描 edge id
    // 口径」对撞，因此用它来锁住分块等价性最合适。
    //
    // 刻意用 oracle 而不是 Cypher 的 `MATCH ... WHERE id(a) = N`：后者是
    // O(边数²) 的（本项目已知限制，见 ROADMAP），上万条边就慢到不可用，
    // 而本用例要验证的是分块正确性，不是执行器的过滤性能。
    let report = db.integrity_check()?;
    assert!(
        report.is_ok(),
        "chunked batch must keep chains intact, got: {:?}",
        report.issues
    );
    assert_eq!(
        report.edges_checked, total,
        "degree oracle must have examined every edge"
    );

    Ok(())
}

/// 崩溃恢复后，**被回放写回的页必须重新登记校验和**。
///
/// 这是一个真实的数据丢失缺陷（由 `examples/crash_recovery_test.rs` 暴露）：
/// `StorageEngine::open` 的回放写主数据文件，且必须早于 `DiskManager::open`
/// （回放会补长文件，而页号高水位由文件长度推导），因此回放时 `CrcStore`
/// 还不存在。结果磁盘上留着**上一次**写的旧校验和，而页内容已是回放后的新内容。
///
/// 之后每次读该页都报校验和不匹配，而 `get_node` 是 lossy 的（折叠为 `None`），
/// 于是已提交的数据看起来「在崩溃后丢了」——这正是该缺陷的隐蔽之处。
///
/// ## 为什么必须真的 SIGKILL
///
/// 正常 `drop` 会走 `Drop` → 缓冲池刷盘并把校验和一并写好，掩盖该缺陷；
/// 只有**硬杀**（不执行任何 Drop）才会留下「页只在 WAL 里、主文件是旧内容」
/// 的状态，从而迫使回放写页。因此本用例必须 fork 子进程并 kill -9。
#[cfg(unix)]
#[test]
fn test_wal_replay_refreshes_page_checksums() -> Result<(), GraphError> {
    use std::process::Command;
    use std::time::{Duration, Instant};

    let dir = tempdir()?;
    let marker = dir.path().join("committed.marker");
    let db_path = dir.path().join("replay_crc.db");

    // 子进程模式：持续提交若干批，然后由父进程 SIGKILL
    if let Ok(mode) = std::env::var("GL_REPLAY_CHILD") {
        if mode == "1" {
            let path = std::env::var("GL_REPLAY_DB").expect("child db path");
            let marker_path =
                std::path::PathBuf::from(std::env::var("GL_REPLAY_MARKER").expect("marker"));
            let db = NervusDb::open(&path)?;
            let mut committed: u64 = 0;
            for _ in 0..20 {
                db.with_transaction(|tx| {
                    for _ in 0..500 {
                        committed += 1;
                        let mut m = HashMap::new();
                        m.insert("idx".to_string(), Value::from(committed as i64));
                        // 多页属性：使数据页分布更广，跨过内联区进入 CRC 目录区
                        m.insert("blob".to_string(), Value::from("Z".repeat(600)));
                        tx.add_node(HashSet::from(["N".to_string()]), m)?;
                    }
                    Ok(())
                })?;
                std::fs::write(&marker_path, committed.to_string())?;
            }
            // 若一直没被杀（不应发生），正常退出
            return Ok(());
        }
    }

    // ── 父进程 ──

    // 1. 先建一个已 checkpoint 的库，使 CRC 目录（页 >= 256）确实存在
    {
        let db = NervusDb::open(&db_path)?;
        db.with_transaction(|tx| {
            for i in 1..=1_500u64 {
                let mut m = HashMap::new();
                m.insert("idx".to_string(), Value::from(i as i64));
                m.insert("blob".to_string(), Value::from("Z".repeat(600)));
                tx.add_node(HashSet::from(["N".to_string()]), m)?;
            }
            Ok(())
        })?;
        db.checkpoint()?;
    }

    // 2. 子进程持续提交，提交到第 3 批后硬杀（不留 Drop、不刷缓冲池）
    let exe = std::env::current_exe().map_err(|e| GraphError::General(e.to_string()))?;
    let mut child = Command::new(exe)
        .env("GL_REPLAY_CHILD", "1")
        .env("GL_REPLAY_DB", db_path.to_string_lossy().to_string())
        .env("GL_REPLAY_MARKER", marker.to_string_lossy().to_string())
        .spawn()
        .map_err(|e| GraphError::General(e.to_string()))?;

    let deadline = Instant::now() + Duration::from_secs(60);
    let mut committed: u64 = 0;
    while Instant::now() < deadline {
        if let Ok(s) = std::fs::read_to_string(&marker) {
            committed = s.trim().parse().unwrap_or(0);
            if committed >= 1_500 {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        committed >= 1_500,
        "child must have committed at least one full batch before the kill"
    );

    // 3. 恢复：回放 WAL 写主文件，校验和必须随之刷新
    let db = NervusDb::open(&db_path)?;
    let expected = 1_500 + committed;
    assert_eq!(db.node_count(), expected as usize);

    for i in 1..=expected {
        db.try_get_node(i)?.unwrap_or_else(|| {
            panic!(
                "node {} must be readable after WAL replay; a stale checksum surfaces \
                 here as a missing node because get_node is lossy",
                i
            )
        });
    }

    // 4. 回放后不得残留任何校验和不一致
    let report = db.integrity_check()?;
    assert!(
        report.is_ok(),
        "post-replay database must have no checksum mismatches, got: {:?}",
        report.issues
    );

    Ok(())
}

#[test]
fn test_lock_poisoning_recovery() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("poison_test.db");
    let db = NervusDb::open(&db_path)?;

    // 正常写入节点
    let nid = db.add_node(HashSet::from(["Safe".to_string()]), HashMap::new())?;

    // 子线程中执行并在持锁逻辑外触发 panic
    let db_clone = db.clone();
    let handle = std::thread::spawn(move || {
        let _node = db_clone.get_node(nid);
        panic!("Controlled panic in worker thread");
    });
    let _ = handle.join();

    // 验证主线程和其他线程依然能够正常读取与写入
    let node_after = db.get_node(nid);
    assert!(node_after.is_some());
    assert_eq!(node_after.unwrap().id, nid);

    let n2 = db.add_node(HashSet::from(["Another".to_string()]), HashMap::new())?;
    assert_eq!(db.node_count(), 2);
    assert_eq!(n2, 2);

    Ok(())
}

/// 自动 Checkpoint 必须在**自动提交的 CRUD 入口**上生效。
///
/// ## 为什么需要这条（覆盖缺口是实测发现的）
///
/// 已有的 `test_wal_auto_checkpoint_triggers` 走的是 `with_transaction`，而那条路径的
/// 自动 Checkpoint 由 `Transaction::commit` 自己调用。**自动提交的 CRUD 入口**
/// （`add_node` 等）走的是另一条收尾路径，此前没有被任何测试覆盖过。
///
/// 这个缺口是 #18 暴露出来的：把 10 处收尾抽成统一入口时，我故意去掉
/// `with_autocommit` 里的自动 Checkpoint 调用，**全套测试无一处失败**——说明那一行
/// 在 CRUD 路径上从未被验证。补上这条之后，去掉它就会失败。
#[test]
fn test_autocommit_path_triggers_auto_checkpoint() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("autocommit_chk.db");

    // 阈值 1 字节：任何一次提交之后都必然越过它
    let db = NervusDb::open_with_options(
        &db_path,
        NervusDbOptions {
            buffer_pool_frames: 256,
            wal_auto_checkpoint_bytes: 1,
            ..NervusDbOptions::default()
        },
    )?;

    let wal_path = {
        let mut p = db_path.as_os_str().to_os_string();
        p.push(".wal");
        std::path::PathBuf::from(p)
    };

    // 走**自动提交**入口（不是 with_transaction）
    for i in 0..200 {
        let mut props = HashMap::new();
        props.insert(
            "payload".to_string(),
            Value::from(format!("Autocommit_Node_{i}_Payload_For_Wal_Growth")),
        );
        db.add_node(HashSet::from(["Item".to_string()]), props)?;
    }

    // 自动 Checkpoint 应当已经触发过：主文件被写入，且 WAL 不会一直膨胀。
    // 断言用「WAL 远小于累计写入量」而不是固定数字：阈值 1 字节时它每次提交后都会
    // 尝试截断，因此 WAL 只应保留最后一次提交的量级。
    let total_written = db.node_count();
    assert_eq!(
        total_written, 200,
        "all 200 autocommit nodes must be present"
    );

    let wal_len = if wal_path.exists() {
        std::fs::metadata(&wal_path)?.len()
    } else {
        0
    };

    // 关闭自动 Checkpoint 时，200 次提交的 WAL 会累积到远大于 64KB；
    // 开启后（阈值 1 字节）应接近「一次提交」的规模。
    assert!(
        wal_len < 64 * 1024,
        "the autocommit path must auto-checkpoint: WAL is {wal_len} bytes after 200 commits, \
         which means truncation never happened on this path"
    );

    // 重开确认数据完整（自动 Checkpoint 不能丢数据）
    drop(db);
    let reopened = NervusDb::open(&db_path)?;
    assert_eq!(
        reopened
            .query_cypher("MATCH (i:Item) RETURN count(*)")?
            .rows[0]
            .values[0],
        Value::from(200),
        "auto-checkpoint on the autocommit path must not lose committed data"
    );

    Ok(())
}

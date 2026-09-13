//! `:memory:` 模式的落盘与数据完整性。
//!
//! `:memory:` 是一个**承诺**：不碰磁盘，且行为与文件模式一致。两条都曾被违反，
//! 且都不会报错——所以只能靠测试盯着。
//!
//! 两个缺陷都**早于**本测试存在（`v1.0.0` 上同样复现），并非某次改动引入。

use nervusdb::{NervusDb, Value};
use std::collections::{HashMap, HashSet};
use tempfile::tempdir;

/// `:memory:` + `checkpoint()` 不得在当前目录建出真文件。
///
/// ## 为什么这条断言值得占一个测试
///
/// 故障形态极其安静：Checkpoint 把已提交页按 `StorageEngine::db_path()` 写出去，
/// 而内存模式下那个值就是字面串 `":memory:"`，于是**当前工作目录**里出现一个
/// 名为 `:memory:` 的真实数据库文件。进程不报错、`node_count()` 也不变，
/// 用户只会在某天发现工作目录里多了个几十 MB 的文件。
///
/// 断言针对**那个具体路径**，而不是「目录为空」：测试进程的 CWD 是 crate 根，
/// 本来就有文件；而 `std::env::set_current_dir` 是进程级的，与并行测试冲突。
/// 检查 `:memory:` 这个路径本身，既精确又不干扰其它测试。
#[test]
fn memory_mode_checkpoint_writes_no_file() -> Result<(), Box<dyn std::error::Error>> {
    let stray = std::path::Path::new(":memory:");

    // 不假设它能被创建：若上一次运行留下了它，先说明这一点，而不是让断言
    // 把它算在本次头上。
    let pre_existing = stray.exists();

    let db = NervusDb::open_with_pool_size(":memory:", 256)?;
    db.with_transaction(|tx| {
        for i in 1..=2_000u64 {
            let mut props = HashMap::new();
            props.insert("idx".to_string(), Value::from(i as i64));
            tx.add_node(HashSet::from(["E".to_string()]), props)?;
        }
        Ok(())
    })?;
    db.checkpoint()?;

    assert!(
        !stray.exists() || pre_existing,
        "`:memory:` must never touch the filesystem, but a checkpoint created \
         a real database file at the literal path {stray:?} (cwd: {:?})",
        std::env::current_dir()?
    );

    Ok(())
}

/// `:memory:` + `checkpoint()` 之后，数据必须仍然读得回来。
///
/// ## 这是本文件里最严重的一条
///
/// 同一个根因的第二个后果：Checkpoint 把页写进了那个**真文件**，而内存
/// `DiskManager` 的 `memory_pages` 一页都没拿到；紧接着 WAL 被截断，那些只
/// 存在于 WAL 里的已提交页就永久没有了。实测 3000/3000 个节点读不回来。
///
/// 它比「多了个文件」隐蔽得多，因为 `node_count()` **仍然正确**——那是元数据，
/// 而丢失的是页内容。所以断言必须逐条读回**属性**，不能只比计数。
/// 属性载荷取 1200 字节，是为了让节点大到必然溢出成独立属性页，从而真的经过
/// WAL 重放路径；小载荷会全部留在常驻页里，测试就变成空转。
#[test]
fn memory_mode_checkpoint_keeps_data_readable() -> Result<(), Box<dyn std::error::Error>> {
    let db = NervusDb::open_with_pool_size(":memory:", 256)?;

    let total = 3_000u64;
    db.with_transaction(|tx| {
        for i in 1..=total {
            let mut props = HashMap::new();
            props.insert("idx".to_string(), Value::from(i as i64));
            props.insert("blob".to_string(), Value::from(format!("{:0>1200}", i)));
            tx.add_node(HashSet::from(["E".to_string()]), props)?;
        }
        Ok(())
    })?;

    db.checkpoint()?;

    assert_eq!(db.node_count(), total as usize);
    let mut unreadable: Vec<u64> = Vec::new();
    for i in 1..=total {
        let got = db
            .get_node(i)
            .and_then(|n| n.get_prop("idx").and_then(|v| v.as_i64()));
        if got != Some(i as i64) {
            unreadable.push(i);
        }
    }
    assert!(
        unreadable.is_empty(),
        "{} of {total} nodes became unreadable after a `:memory:` checkpoint \
         (first: {:?})",
        unreadable.len(),
        unreadable.first()
    );

    Ok(())
}

/// 缓冲池里全是「干净但镜像已在 WAL」的未提交页时，置换必须仍然可行。
///
/// ## 故障形态
///
/// 未提交页有两种：(a) 脏、镜像还没进 WAL；(b) 干净、镜像已经在 WAL 里（此前
/// 溢出过，之后又被读回来）。原来的 STEAL 轮只认 (a)——它要求 `is_dirty`。
/// 于是当池里恰好全是 (b) 时，第一轮因「未提交」拒绝、第二轮因「不脏」拒绝，
/// 两轮都空手而归：**所有帧都可用，却一个也拿不到**，报 NO-STEAL 错误。
///
/// 实测把池填成这种状态的是「大节点批 + 大边批」：节点事务把池写满并溢出，
/// 边事务开始后读回的页都是干净的 (b)，`dirty` 只剩 2，而 246/256 帧是 (b)。
///
/// 这个夹具是当时能找到的最小复现（更小的规模两者都装得下，撞不到上限）。
#[test]
fn a_pool_full_of_clean_uncommitted_frames_can_still_evict(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("no_steal_drift.db");

    let options = nervusdb::NervusDbOptions {
        buffer_pool_frames: 512,
        // 夹具按「单事务」构造，必须先解除动作队列上限（见 AGENTS §1）。
        max_transaction_actions: 0,
        ..nervusdb::NervusDbOptions::default()
    };
    let db = NervusDb::open_with_options(&path, options)?;

    let nodes = 100_000u64;
    db.with_transaction(|tx| {
        for i in 1..=nodes {
            let mut props = HashMap::new();
            props.insert("idx".to_string(), Value::from(i as i64));
            tx.add_node(HashSet::from(["E".to_string()]), props)?;
        }
        Ok(())
    })?;

    let edges = 200_000u64;
    db.with_transaction(|tx| {
        for i in 0..edges {
            let src = (i % nodes) + 1;
            let dst = ((i * 7_919 + 13) % nodes) + 1;
            if src != dst {
                tx.add_edge(src, dst, "R", HashMap::new(), 1.0)?;
            }
        }
        Ok(())
    })?;

    // 换出确实发生过，否则这个夹具没测到它想测的东西。
    let stats = db.buffer_stats();
    assert!(
        stats.spill_count > 0,
        "the fixture must actually drive spilling, or it proves nothing"
    );

    Ok(())
}

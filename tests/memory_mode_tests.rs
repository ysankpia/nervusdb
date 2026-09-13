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

/// `:memory:` 上的 `backup` 必须被拒绝，且**理由要说对**。
///
/// ## 为什么这条值得单独一个测试
///
/// 「失败」与「失败得有意义」是两回事。这条路径此前没有被走过，它失败在
/// `File::open(":memory:")` 上，报的是：
///
/// ```text
/// Storage I/O error: No such file or directory (os error 2)
/// ```
///
/// 调用方读到的意思是「我给的路径有问题」。真实原因是「这个库没有文件可复制」，
/// 而这两者指向完全不同的补救动作：一个是检查路径拼写，另一个是改用
/// `dump_cypher`。所以断言不能只写 `is_err()` —— 那正是让这个缺陷长期幸存的写法。
///
/// 更早的版本里它甚至**不会失败**：`:memory:` 的 checkpoint 缺陷会先把一个垃圾
/// 文件写到 `":memory:"` 这个路径上，于是 `File::open` 成功，backup **报告成功**
/// 并复制出一份无意义的副本。那次修复在本文件的另一个测试里；这里盯的是错误信息。
#[test]
fn backing_up_a_memory_database_is_refused_with_the_real_reason(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let db = NervusDb::open(":memory:")?;
    db.with_transaction(|tx| {
        tx.add_node(HashSet::from(["N".to_string()]), HashMap::new())?;
        Ok(())
    })?;

    let dest = dir.path().join("from_memory.db");
    let err = db
        .backup(&dest)
        .expect_err("backing up a `:memory:` database must be refused")
        .to_string();

    assert!(
        err.contains(":memory:"),
        "the error must name the actual cause (an in-memory database), got: {err}"
    );
    assert!(
        err.contains("dump_cypher"),
        "the error must point at a way forward, got: {err}"
    );
    assert!(
        !err.contains("No such file"),
        "a missing-file error sends the caller to check their path, which is not \
         the problem; got: {err}"
    );

    // 而且不得留下任何文件。
    assert!(
        !dest.exists(),
        "a refused backup must not create the target"
    );

    Ok(())
}

// =========================================================================
// 公开 API 的可用性守卫（此前零覆盖的入口）
// =========================================================================

/// `Transaction::add_edges` 的 `edge_id` 是引擎分配的，输入值必须被忽略。
///
/// ## 为什么钉住一个「被忽略的字段」
///
/// `EdgeInsert` 是公开类型，`edge_id` 是公开字段，于是 `EdgeInsert { edge_id: 999_000, .. }`
/// 完全合法——而 `add_edges` 会忽略它并返回自己分配的 ID。调用方传入一个
/// 精心构造的 ID、拿回 `[1, 2, 3, 4]`，然后发现 `get_edge(999_000)` 是 `None`。
///
/// 这与 #19 删掉的那批接口是同一类问题：**声称能做的事，做了没效果**。区别是
/// 这里字段不能删（提交路径内部真的用它承载已分配的 ID），所以改为钉住行为，
/// 并让 `EdgeInsert::new` 成为不会误设的入口。
///
/// 断言刻意分成两半：返回的 ID **可用**，传入的 ID **不存在**。只断言前者会
/// 漏掉「ID 被采纳了但顺序错乱」，只断言后者会漏掉「ID 被采纳了」。
#[test]
fn add_edges_assigns_ids_and_ignores_the_supplied_edge_id() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempdir()?;
    let db = NervusDb::open(dir.path().join("batch_edges.db"))?;

    let decoy = 999_000u64;
    let assigned = db.with_transaction(|tx| {
        let nodes: Vec<_> = (0..5)
            .map(|i| {
                (
                    HashSet::from(["N".to_string()]),
                    HashMap::from([("i".to_string(), Value::from(i as i64))]),
                )
            })
            .collect();
        let ids = tx.add_nodes(nodes)?;

        // 用 `new`（正确入口）与字面量（带诱饵 ID）各构造一半，确认两条都按引擎分配
        let mut edges = Vec::new();
        for i in 0..4usize {
            if i % 2 == 0 {
                edges.push(nervusdb::disk_graph::EdgeInsert::new(
                    ids[i],
                    ids[i + 1],
                    "E",
                    HashMap::new(),
                    1.0,
                ));
            } else {
                edges.push(nervusdb::disk_graph::EdgeInsert {
                    edge_id: decoy + i as u64,
                    src_id: ids[i],
                    dst_id: ids[i + 1],
                    edge_type: "E".to_string(),
                    properties: HashMap::new(),
                    weight: 1.0,
                });
            }
        }
        tx.add_edges(edges)
    })?;

    assert_eq!(assigned.len(), 4, "one id per input edge");

    // 返回的 ID 必须真的可用。
    for id in &assigned {
        assert!(
            db.get_edge(*id).is_some(),
            "add_edges returned id {id}, but no such edge exists"
        );
    }
    // 返回的 ID 必须互不相同（顺序错乱会表现为重复）。
    let unique: std::collections::HashSet<_> = assigned.iter().collect();
    assert_eq!(
        unique.len(),
        4,
        "returned ids must be distinct: {assigned:?}"
    );

    // 传入的 ID 必须没有被采纳。
    for decoy_id in (decoy..decoy + 4).filter(|i| i % 2 == 1) {
        assert!(
            db.get_edge(decoy_id).is_none(),
            "the supplied edge_id {decoy_id} must be ignored, but an edge with that \
             id exists"
        );
    }

    Ok(())
}

/// 4 个此前从未被任何测试调用过的公开入口，至少要能正常工作。
///
/// 覆盖它们不是为了凑数字，而是因为「没测过」和「测过且正确」在出问题前看起来
/// 一样——这正是本轮发现的三个缺陷的共同来源。`export_subgraph` 的 `to_json`
/// 单独用外部 JSON 解析器验收（见 `docs/testing.md` 的同类做法）。
#[test]
fn previously_uncovered_public_entry_points_work() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let db = NervusDb::open(dir.path().join("entry_points.db"))?;
    db.with_transaction(|tx| {
        for i in 1..=5u64 {
            tx.add_node(
                HashSet::from(["P".to_string()]),
                HashMap::from([("i".to_string(), Value::from(i as i64))]),
            )?;
        }
        for i in 1..5u64 {
            tx.add_edge(i, i + 1, "R", HashMap::new(), 1.0)?;
        }
        Ok(())
    })?;

    // `export_subgraph`
    let full = db.export_subgraph(100)?;
    assert_eq!(full.nodes.len(), 5);
    assert_eq!(full.edges.len(), 4);
    assert!(!full.truncated);
    assert_eq!(full.total_nodes, 5);

    // `limit` 必须真的截断，且 `truncated` 要说实话——一个静默截断的图会误导判断。
    let part = db.export_subgraph(2)?;
    assert_eq!(part.nodes.len(), 2);
    assert!(
        part.truncated,
        "a limited export must report itself as truncated"
    );
    assert_eq!(part.total_nodes, 5, "totals ignore the limit");

    // 子图必须自洽：不能有指向集合外节点的边。
    let present: std::collections::HashSet<u64> = part.nodes.iter().map(|n| n.id).collect();
    for e in &part.edges {
        assert!(
            present.contains(&e.src) && present.contains(&e.dst),
            "exported edge {} dangles outside the node set",
            e.id
        );
    }

    // `GraphExport::to_json`
    let json = full.to_json();
    assert!(json.contains("\"nodes\""), "json must carry nodes: {json}");
    assert!(json.contains("\"edges\""), "json must carry edges: {json}");

    // `Transaction::tx_id`：回滚过的 id 不得被复用（复用会让「事务唯一标识」失效）
    let mut seen = Vec::new();
    for _ in 0..3 {
        let tx = db.begin_transaction()?;
        seen.push(tx.tx_id());
        tx.rollback()?;
    }
    let next = db.begin_transaction()?.tx_id();
    let distinct: std::collections::HashSet<_> = seen.iter().collect();
    assert_eq!(
        distinct.len(),
        seen.len(),
        "tx ids must be distinct: {seen:?}"
    );
    assert!(
        !seen.contains(&next),
        "a rolled-back transaction's id ({seen:?}) must not be reused, but {next} repeats one"
    );

    Ok(())
}

//! 事务动作队列**溢出到 WAL**：一个事务可以大于内存上限。
//!
//! ## 这套测试要证明什么
//!
//! 溢出（spill）的收益是「事务不再受内存上限约束」，代价是「提交前 WAL 里就有
//! 未提交数据」。收益容易验证，**代价才是危险的**——未提交数据出现在 WAL 里，
//! 若崩溃恢复或 Checkpoint 处理不当，就会让本该被回滚的事务变成落盘的事实。
//!
//! 因此这里的重点是代价一侧：
//!
//! 1. 溢出的动作**按序**施加（`AddNode` 必须先于引用它的 `AddEdge`）；
//! 2. 回滚后**零残留**，重开也看不到；
//! 3. 溢出期间 **Checkpoint 被拒绝**，而不是静默截断 WAL 把动作丢掉；
//! 4. 未提交的溢出帧**绝不被恢复写入主文件**；
//! 5. 溢出的动作**不改变错误时机**——一个非法动作仍在提交时报错。
//!
//! 全部走公开 API。

use nervusdb::{GraphError, NervusDb, NervusDbOptions, Value};
use std::collections::{HashMap, HashSet};
use tempfile::tempdir;

/// 打开一个启用溢出的库，池与上限都设得很小以便快速触顶。
fn open_spilling(
    name: &str,
    max_actions: usize,
) -> Result<(tempfile::TempDir, NervusDb), GraphError> {
    let dir = tempdir()?;
    let db = NervusDb::open_with_options(
        dir.path().join(name),
        NervusDbOptions {
            buffer_pool_frames: 256,
            max_transaction_actions: max_actions,
            spill_transaction_actions: true,
            ..Default::default()
        },
    )?;
    Ok((dir, db))
}

fn count(db: &NervusDb, label: &str) -> Result<i64, GraphError> {
    let r = db.run_cypher(&format!("MATCH (n:{label}) RETURN count(*)"))?;
    match &r.rows[0].values[0] {
        Value::Int(v) => Ok(*v),
        other => panic!("count must be Int, got {other:?}"),
    }
}

// =========================================================================
// 1. 收益：事务真的能大于上限
// =========================================================================

/// 上限 100，提交 1000 个节点：溢出使事务不再被内存上限拦住。
///
/// 断言不只是「成功了」，还包括**数据完整**：1000 个节点、每个属性都对。
#[test]
fn test_spill_lifts_the_memory_cap() -> Result<(), GraphError> {
    let (_dir, db) = open_spilling("spill_lift.db", 100)?;

    let mut tx = db.begin_transaction()?;
    for i in 0..1000i64 {
        tx.add_node(
            HashSet::from(["S".to_string()]),
            HashMap::from([("i".to_string(), Value::from(i))]),
        )?;
    }
    tx.commit()?;

    assert_eq!(count(&db, "S")?, 1000, "all 1000 nodes must be committed");

    // 属性必须逐个正确：溢出按序读回，编号错位会在总和上暴露
    let sum = db.run_cypher("MATCH (s:S) RETURN sum(s.i)")?;
    assert_eq!(
        sum.rows[0].values[0],
        Value::from((0..1000i64).sum::<i64>()),
        "property values must survive the spill in order"
    );

    Ok(())
}

/// 溢出的 `AddNode` 必须先于引用它的 `AddEdge` 被施加。
///
/// 这是溢出最容易出错的地方：动作分散在内存窗口与 WAL 两处，读回顺序若按
/// 「先内存后磁盘」就会让边先于节点，产生一个指向不存在节点的边。
#[test]
fn test_spill_preserves_action_order_across_the_window() -> Result<(), GraphError> {
    let (_dir, db) = open_spilling("spill_order.db", 10)?;

    let mut tx = db.begin_transaction()?;
    // 交替入队：每 5 个节点后接一条边。上限 10 保证窗口被反复腾空，
    // 因此「已溢出的节点」与「常驻的边」会同时存在于两处。
    let mut ids = Vec::new();
    for i in 0..30u64 {
        ids.push(tx.add_node(
            HashSet::from(["O".to_string()]),
            HashMap::from([("i".to_string(), Value::from(i as i64))]),
        )?);
        if i > 0 {
            tx.add_edge(
                ids[(i - 1) as usize],
                ids[i as usize],
                "NEXT",
                HashMap::new(),
                1.0,
            )?;
        }
    }
    tx.commit()?;

    assert_eq!(count(&db, "O")?, 30);

    // 链必须完整：29 条边，且每条的两个端点都存在
    let edges = db.run_cypher("MATCH ()-[r:NEXT]->() RETURN count(r)")?;
    assert_eq!(
        edges.rows[0].values[0],
        Value::from(29),
        "the chain must be intact"
    );

    let joined = db.run_cypher("MATCH (a:O)-[:NEXT]->(b:O) RETURN count(*)")?;
    assert_eq!(
        joined.rows[0].values[0],
        Value::from(29),
        "every edge must resolve to two real nodes (order was preserved)"
    );

    Ok(())
}

/// 批量入口（`add_nodes`）同样能溢出。
///
/// 这条曾是真实缺口：批量路径有一份自己的上限检查，看不到溢出逻辑，于是
/// 「单条能溢出、批量报错」——同一选项两种行为。
#[test]
fn test_batch_entry_can_also_spill() -> Result<(), GraphError> {
    let (_dir, db) = open_spilling("spill_batch.db", 20)?;

    let mut tx = db.begin_transaction()?;
    let items: Vec<_> = (0..50)
        .map(|i| {
            (
                HashSet::from(["B".to_string()]),
                HashMap::from([("i".to_string(), Value::from(i as i64))]),
            )
        })
        .collect();
    tx.add_nodes(items)?;
    tx.commit()?;

    assert_eq!(count(&db, "B")?, 50, "the batch entry point must spill too");
    Ok(())
}

// =========================================================================
// 2. 代价：回滚与恢复
// =========================================================================

/// 溢出过的事务回滚后必须**零残留**，包括 WAL 里的动作帧。
#[test]
fn test_rollback_after_spill_leaves_no_trace() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let path = dir.path().join("spill_rollback.db");
    {
        let db = NervusDb::open_with_options(
            &path,
            NervusDbOptions {
                buffer_pool_frames: 256,
                max_transaction_actions: 50,
                spill_transaction_actions: true,
                ..Default::default()
            },
        )?;

        let mut tx = db.begin_transaction()?;
        for i in 0..500i64 {
            tx.add_node(
                HashSet::from(["Taint".to_string()]),
                HashMap::from([("i".to_string(), Value::from(i))]),
            )?;
        }
        // 溢出确实发生过：WAL 里现在有该事务的动作帧
        assert!(
            db.buffer_stats().wal_size_bytes > 0,
            "spilling must have written action frames to the WAL"
        );
        tx.rollback()?;

        assert_eq!(
            count(&db, "Taint")?,
            0,
            "rollback must leave nothing visible"
        );
        db.checkpoint()?;
    }

    // 重开：恢复必须同样看不到那些动作
    let db = NervusDb::open(&path)?;
    assert_eq!(
        count(&db, "Taint")?,
        0,
        "uncommitted spilled actions must never be replayed after a reopen"
    );
    Ok(())
}

/// 溢出期间 Checkpoint 必须**被拒绝**，而不是静默截断 WAL 丢掉动作。
///
/// 这是本项工作里最危险的交互：如果放行，截断会让未提交动作消失，而事务随后
/// 提交时读不到它们——表现为「提交成功但数据少了」，且不报错。
#[test]
fn test_checkpoint_is_refused_while_actions_are_spilled() -> Result<(), GraphError> {
    let (_dir, db) = open_spilling("spill_checkpoint.db", 50)?;

    let mut tx = db.begin_transaction()?;
    for i in 0..300i64 {
        tx.add_node(
            HashSet::from(["C".to_string()]),
            HashMap::from([("i".to_string(), Value::from(i))]),
        )?;
    }

    let err = db
        .checkpoint()
        .expect_err("a checkpoint during an uncommitted spill must be refused");
    let msg = err.to_string();
    assert!(
        msg.contains("Checkpoint refused"),
        "the refusal must say what happened, got: {msg}"
    );
    assert!(
        msg.contains("Commit or roll back first"),
        "the refusal must say what to do, got: {msg}"
    );

    // 拒绝之后数据仍必须完整（拒绝没有破坏任何东西）
    tx.commit()?;
    assert_eq!(count(&db, "C")?, 300);

    // 事务结束后 Checkpoint 恢复可用
    db.checkpoint()?;
    assert_eq!(
        count(&db, "C")?,
        300,
        "a checkpoint after commit must keep the data"
    );
    Ok(())
}

/// 溢出的事务提交后，重开必须看到**全部**数据。
///
/// 这条覆盖「提交写 WAL → 重开 → 恢复」的完整链路：溢出的动作帧与
/// 提交页帧同时存在于 WAL 中，恢复只应回放后者、忽略前者。
#[test]
fn test_spilled_transaction_survives_reopen() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let path = dir.path().join("spill_reopen.db");
    {
        let db = NervusDb::open_with_options(
            &path,
            NervusDbOptions {
                buffer_pool_frames: 256,
                max_transaction_actions: 40,
                spill_transaction_actions: true,
                ..Default::default()
            },
        )?;

        let mut tx = db.begin_transaction()?;
        for i in 0..400i64 {
            tx.add_node(
                HashSet::from(["R".to_string()]),
                HashMap::from([("i".to_string(), Value::from(i))]),
            )?;
        }
        tx.commit()?;
        // 不做 Checkpoint：故意让恢复去处理 WAL 里的动作帧
    }

    let db = NervusDb::open(&path)?;
    assert_eq!(count(&db, "R")?, 400);
    let sum = db.run_cypher("MATCH (r:R) RETURN sum(r.i)")?;
    assert_eq!(sum.rows[0].values[0], Value::from((0..400i64).sum::<i64>()));
    Ok(())
}

// =========================================================================
// 3. 溢出不改变语义
// =========================================================================

/// 溢出不改变**错误时机**：一个非法动作仍在提交时报错，而不是入队时报错。
///
/// 事务的原子性保证围绕「提交失败」构建。若溢出让非法动作提前在 `add_node`
/// 报错，调用方会看到与不溢出时不同的失败点——那是静默的语义变化。
#[test]
fn test_spill_does_not_move_the_error_point() -> Result<(), GraphError> {
    let (_dir, db) = open_spilling("spill_error.db", 20)?;

    db.execute("CREATE (:U {k: 1})")?;
    db.execute("CREATE (:U {k: 2})")?;
    db.create_unique_constraint("U", "k")?;

    let mut tx = db.begin_transaction()?;
    // 先入队足够多合法动作，把窗口腾空（触发溢出）
    for i in 0..100i64 {
        tx.add_node(
            HashSet::from(["V".to_string()]),
            HashMap::from([("i".to_string(), Value::from(i))]),
        )?;
    }
    // 再入队一个违反唯一约束的动作：**入队必须成功**
    tx.add_node(
        HashSet::from(["U".to_string()]),
        HashMap::from([("k".to_string(), Value::from(1))]),
    )
    .expect("enqueueing a violating action must succeed; the error belongs to commit");

    // 提交时失败
    let err = tx
        .commit()
        .expect_err("the constraint violation must fail the commit");
    assert!(
        matches!(err, GraphError::UniqueConstraintViolation { .. }),
        "expected a unique-constraint violation at commit, got {err:?}"
    );

    // 且部分写入被回滚：全部 100 个溢出动作都不该留下
    assert_eq!(
        count(&db, "V")?,
        0,
        "a failed commit must roll back the spilled actions too"
    );
    Ok(())
}

/// 关闭溢出时行为回到「触顶报错」——默认行为不变。
#[test]
fn test_spill_disabled_still_reports_overflow() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = NervusDb::open_with_options(
        dir.path().join("spill_off.db"),
        NervusDbOptions {
            buffer_pool_frames: 256,
            max_transaction_actions: 10,
            spill_transaction_actions: false,
            ..Default::default()
        },
    )?;

    let mut tx = db.begin_transaction()?;
    let mut hit = false;
    for i in 0..100i64 {
        if tx
            .add_node(
                HashSet::from(["W".to_string()]),
                HashMap::from([("i".to_string(), Value::from(i))]),
            )
            .is_err()
        {
            hit = true;
            break;
        }
    }
    assert!(
        hit,
        "with spilling disabled the cap must still report an error"
    );
    drop(tx);
    Ok(())
}

/// 一次**已经成功**的提交绝不能因为另一个事务在溢出而报错。
///
/// ## 这条测试来自我自己实现里的一个真实缺陷
///
/// 溢出期间 Checkpoint 被拒绝（见上一个测试）是对的。但自动 Checkpoint 走的是
/// 同一条路径，于是：tx1 溢出后保持打开，tx2 提交时自动 Checkpoint 被拒，
/// `commit()` 把那个错误抛了出来——**tx2 的提交返回 `Err`，而它的数据已经
/// 持久化**。实测确认：会话内可见 1 个 `:Other`，重开后仍是 1 个。
///
/// 那是一个调用方无法处理的状态：它唯一合理的反应是重试，而重试会产生重复数据。
/// 因此自动 Checkpoint 的「推迟」必须被吞掉并重新置位，只有真正的 I/O 错误才上报。
#[test]
fn test_successful_commit_is_not_reported_as_failed_by_a_deferred_checkpoint(
) -> Result<(), GraphError> {
    let dir = tempdir()?;
    let path = dir.path().join("spill_deferred_cp.db");
    {
        let db = NervusDb::open_with_options(
            &path,
            NervusDbOptions {
                buffer_pool_frames: 256,
                max_transaction_actions: 50,
                spill_transaction_actions: true,
                // 阈值 1 字节：提交后必然触发自动 Checkpoint
                wal_auto_checkpoint_bytes: 1,
                ..Default::default()
            },
        )?;

        // tx1 溢出动作并保持打开 —— 它会让任何 Checkpoint 必须推迟
        let mut tx1 = db.begin_transaction()?;
        for i in 0..300i64 {
            tx1.add_node(
                HashSet::from(["Held".to_string()]),
                HashMap::from([("i".to_string(), Value::from(i))]),
            )?;
        }

        // tx2 提交：自动 Checkpoint 会被推迟，但提交本身成功了
        let mut tx2 = db.begin_transaction()?;
        tx2.add_node(HashSet::from(["Other".to_string()]), HashMap::new())?;
        tx2.commit()
            .expect("a deferred checkpoint must not turn a successful commit into an error");

        assert_eq!(count(&db, "Other")?, 1);

        tx1.rollback()?;

        // 溢出结束后，Checkpoint 恢复可用，并且不再被推迟
        db.checkpoint()?;
    }

    // 重开确认 tx2 的数据确实持久化了（而不是「报错其实失败」的反面：
    // 「没报错其实失败」）
    let db = NervusDb::open(&path)?;
    assert_eq!(
        count(&db, "Other")?,
        1,
        "the committed data must be durable after the deferred checkpoint"
    );
    assert_eq!(
        count(&db, "Held")?,
        0,
        "the rolled-back transaction leaves nothing"
    );
    Ok(())
}

/// 溢出过程中**进程消失**（未提交、未回滚），重开后必须看不到那些动作。
///
/// ## 与「回滚后重开」不同
///
/// `rollback()` 会走正常的清理路径。这里模拟的是 Drop 来不及跑、更接近 `SIGKILL`
/// 的情形：WAL 里留着该事务的 `ActionWrite` 帧，而**没有任何 `TxCommit`**。
///
/// 恢复必须把它们当成「未提交」跳过。依据是恢复只施加 `PageWrite`，而
/// `ActionWrite` 描述的是「尚未施加的动作」——两者在结构上就不同，不是靠 `tx_id`
/// 是否出现过来判断的。这条测试固定住这个性质。
#[test]
fn test_uncommitted_spilled_actions_are_ignored_after_an_abrupt_exit() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let path = dir.path().join("spill_abrupt.db");
    {
        let db = NervusDb::open_with_options(
            &path,
            NervusDbOptions {
                buffer_pool_frames: 256,
                max_transaction_actions: 30,
                spill_transaction_actions: true,
                ..Default::default()
            },
        )?;

        let mut tx = db.begin_transaction()?;
        for i in 0..500i64 {
            tx.add_node(
                HashSet::from(["Ghost".to_string()]),
                HashMap::from([("i".to_string(), Value::from(i))]),
            )?;
        }
        assert!(
            db.buffer_stats().wal_size_bytes > 0,
            "the transaction must have spilled action frames into the WAL"
        );
        // 不 commit、不 rollback；显式 drop 让 WAL 里的动作帧留在原地。
        drop(tx);
        // 不做 Checkpoint —— 让那些帧真的留在 WAL 里等恢复来处理。
    }

    let db = NervusDb::open(&path)?;
    assert_eq!(
        count(&db, "Ghost")?,
        0,
        "action frames without a matching TxCommit must never be replayed"
    );
    // 库必须仍然可用
    db.execute("CREATE (:After {ok: true})")?;
    assert_eq!(count(&db, "After")?, 1);
    Ok(())
}

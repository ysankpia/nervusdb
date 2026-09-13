//! 锁等待（`lock_wait_ms`）：第二个句柄等待而不是立刻失败。
//!
//! ## 这套测试要证明什么
//!
//! 它来自一个具体的问题：「我开两三个会话窗口同时写同一个库，会卡吗？」
//! 实测答案是**第二个窗口直接报错**（嵌入式单写者模型的常态，SQLite 默认亦然）。
//! `lock_wait_ms` 把「立刻失败」变成「等一会儿再失败」——对**先后**写入（第二个
//! 窗口在第一个写完之后才来）这是关键差别。
//!
//! 因此重点在三条边界上，而不是「能等到」这一条：
//!
//! 1. **默认 0 的行为不得改变**——不等待、立即失败。这是兼容性底线。
//! 2. **等到就成功**——持有者释放后，等待者必须拿到锁。
//! 3. **等不到仍要失败**——超时后是 `DatabaseLocked`，不是静默成功，也不是
//!    无限等待。一个「等下去就好」的实现会把调用方挂死。
//!
//! 还要证伪一个误解：**它不会让两个写者并存。** 因此有一条测试断言两个写句柄
//! 在等待模式下**仍然**互相排斥。

use nervusdb::{GraphError, NervusDb, NervusDbOptions};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::tempdir;

fn opts(wait_ms: u64) -> NervusDbOptions {
    NervusDbOptions {
        lock_wait_ms: wait_ms,
        ..Default::default()
    }
}

/// 建一个干净的库并**checkpoint**，避免 WAL 未回放影响只读打开。
fn setup(path: &std::path::Path) -> Result<(), GraphError> {
    let db = NervusDb::open(path)?;
    db.execute("CREATE (:Seed {v: 1})")?;
    db.checkpoint()?;
    Ok(())
}

// =========================================================================
// 1. 默认行为不得改变
// =========================================================================

/// 默认（`lock_wait_ms = 0`）必须**立即**失败，不能引入任何等待。
///
/// 这条是兼容性底线：加这个选项之前，第二个写句柄的行为是立刻返回错误。若默认
/// 变成了等待，所有依赖「快速失败」的调用方（例如启动时探测文件是否被占用）
/// 都会在不知不觉中变慢。
#[test]
fn test_default_does_not_wait() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let path = dir.path().join("nowait.db");
    setup(&path)?;

    let _first = NervusDb::open_with_options(&path, opts(0))?;
    let t0 = Instant::now();
    let err = match NervusDb::open_with_options(&path, opts(0)) {
        Ok(_) => panic!("the second writer must fail by default"),
        Err(e) => e,
    };
    let elapsed = t0.elapsed();

    assert!(
        matches!(err, GraphError::DatabaseLocked(_)),
        "expected DatabaseLocked, got {err:?}"
    );
    assert!(
        elapsed < Duration::from_millis(100),
        "the default must fail immediately, took {elapsed:?}"
    );
    Ok(())
}

// =========================================================================
// 2. 等到就成功
// =========================================================================

/// 持有者在等待窗口内释放 → 等待者必须拿到锁。
///
/// 这是这个选项存在的**唯一理由**：先后写入不该失败。
#[test]
fn test_waits_until_the_other_handle_releases() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let path = dir.path().join("wait.db");
    setup(&path)?;

    let holder = NervusDb::open_with_options(&path, opts(0))?;

    // 150ms 后释放持有者
    let path2 = path.clone();
    let released = Arc::new(AtomicBool::new(false));
    let released2 = Arc::clone(&released);
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        drop(holder);
        released2.store(true, Ordering::SeqCst);
        path2
    });

    // 等待预算 3s，远超持有时间
    let t0 = Instant::now();
    let second = NervusDb::open_with_options(&path, opts(3000))?;
    let elapsed = t0.elapsed();

    assert!(
        released.load(Ordering::SeqCst),
        "the second handle must only succeed after the first released"
    );
    assert!(
        elapsed >= Duration::from_millis(100),
        "it must actually have waited, took {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(3000),
        "it must not have waited for the full budget, took {elapsed:?}"
    );

    // 拿到的锁是真的：能写
    second.execute("CREATE (:After {v: 2})")?;
    let n = second.run_cypher("MATCH (a:After) RETURN count(*)")?;
    assert_eq!(n.rows[0].values[0], nervusdb::Value::from(1));

    let _ = releaser.join();
    Ok(())
}

// =========================================================================
// 3. 等不到仍要失败（不能挂死）
// =========================================================================

/// 持有者不释放 → 超时后必须是 `DatabaseLocked`，且耗时应接近预算。
///
/// 「一直等下去」的实现会让调用方永久挂起，比立即失败更糟：失败至少可诊断。
#[test]
fn test_times_out_with_an_error() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let path = dir.path().join("timeout.db");
    setup(&path)?;

    let _holder = NervusDb::open_with_options(&path, opts(0))?;

    let budget = 300;
    let t0 = Instant::now();
    let err = match NervusDb::open_with_options(&path, opts(budget)) {
        Ok(_) => panic!("it must give up when the holder never releases"),
        Err(e) => e,
    };
    let elapsed = t0.elapsed();

    assert!(
        matches!(err, GraphError::DatabaseLocked(_)),
        "expected DatabaseLocked after the timeout, got {err:?}"
    );
    assert!(
        elapsed >= Duration::from_millis(budget - 50),
        "it must have used its budget, took {elapsed:?}"
    );
    // 允许 5 倍余量：退避上限 20ms，加调度抖动，不该更久
    assert!(
        elapsed < Duration::from_millis(budget * 5),
        "it must not overrun its budget, took {elapsed:?}"
    );

    // 错误信息必须告诉调用方可以调这个选项，否则用户不知道有解法
    let msg = err.to_string();
    assert!(
        msg.contains("lock_wait_ms"),
        "the error must name the option that changes this, got: {msg}"
    );
    Ok(())
}

// =========================================================================
// 4. 它不让两个写者并存（证伪误解）
// =========================================================================

/// 两个写句柄在等待模式下**仍然**互斥。
///
/// 这条测试存在是因为这个选项的名字容易被误解成「现在可以多进程写了」。
/// 它只是等待超时/成功，不是并发写。若不写这条测试，将来有人可能据此误以为
/// 并发写已经支持。
#[test]
fn test_waiting_does_not_permit_two_concurrent_writers() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let path = dir.path().join("still_exclusive.db");
    setup(&path)?;

    let first = NervusDb::open_with_options(&path, opts(500))?;

    // 拿住不放到等待超时：第二个**必须**失败，无论它等了多久
    let err = match NervusDb::open_with_options(&path, opts(200)) {
        Ok(_) => panic!("two writers must still exclude each other"),
        Err(e) => e,
    };
    assert!(matches!(err, GraphError::DatabaseLocked(_)), "got {err:?}");

    // 第一个句柄全程有效
    first.execute("CREATE (:Still {v: 1})")?;
    Ok(())
}

/// `lock_wait_ms` 对**只读**句柄同样生效：等写者释放后能打开。
///
/// 只读打开被写者挡住是常见情形（「后台在写，前台想看一眼」）。它同样不该在
/// 写者刚好要结束时立刻失败。
#[test]
fn test_read_only_handle_also_waits() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let path = dir.path().join("ro_wait.db");
    setup(&path)?;

    let writer = NervusDb::open_with_options(&path, opts(0))?;

    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        drop(writer);
    });

    let mut ro_opts = opts(3000);
    ro_opts.read_only = true;
    let ro = NervusDb::open_with_options(&path, ro_opts)?;
    assert!(ro.is_read_only());
    assert_eq!(ro.node_count(), 1, "the seed node must be visible");

    let _ = releaser.join();
    Ok(())
}

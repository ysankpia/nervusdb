//! 跨进程验证：两个**真实进程**先后写同一个库。
//!
//! ## 为什么需要它
//!
//! 同进程内的失败与跨进程失败走的是同一条 `try_lock` 路径，但**同进程**的 flock
//! 语义在平台间并不等价（同一进程两次 flock 可能互相放行）。因此「两个会话窗口」
//! 这个场景必须在**真的两个进程**上验证。
//!
//! ## 为什么用退出码而不是 stdout
//!
//! 子进程是同一个测试二进制、以测试过滤参数启动的，它的 `println!` 会进测试框架
//! 的捕获缓冲，`Command::output()` 拿不到。首版据此判断「子进程没有输出」，把一个
//! 正常退出误读成「没有被拒绝」。**退出码不会被捕获**，因此子进程用退出码回报结果：
//!
//! - `0` = 成功打开并写入
//! - `3` = 因 `DatabaseLocked` 被拒（预期的排他行为）
//! - 其它非零 = 意外错误
//!
//! 这是父进程唯一能可靠读到的东西。

use nervusdb::{NervusDb, NervusDbOptions, Value};
use std::process::{Command, Stdio};
use tempfile::tempdir;

const CHILD_ENV: &str = "NERVUSDB_LOCK_CHILD_DB";
const CHILD_WAIT: &str = "NERVUSDB_LOCK_CHILD_WAIT";

/// 子进程被锁拒时的退出码。
const EXIT_LOCKED: i32 = 3;

#[test]
fn test_second_process_waits_for_the_first() -> Result<(), std::boxed::Box<dyn std::error::Error>> {
    // ---- 子进程模式 ----
    if let Ok(db_path) = std::env::var(CHILD_ENV) {
        let wait: u64 = std::env::var(CHILD_WAIT)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let opts = NervusDbOptions {
            lock_wait_ms: wait,
            ..Default::default()
        };
        let code = match NervusDb::open_with_options(&db_path, opts) {
            Ok(db) => {
                if db.execute("CREATE (:FromChild {v: 1})").is_ok() {
                    0
                } else {
                    4
                }
            }
            Err(nervusdb::GraphError::DatabaseLocked(_)) => EXIT_LOCKED,
            Err(_) => 5,
        };
        std::process::exit(code);
    }

    // ---- 父进程模式 ----
    let dir = tempdir()?;
    let path = dir.path().join("cross.db");
    {
        let db = NervusDb::open(&path)?;
        db.execute("CREATE (:Seed {v: 1})")?;
        db.checkpoint()?;
    }

    let exe = std::env::current_exe()?;
    let spawn_child = |wait_ms: u64| -> std::io::Result<std::process::ExitStatus> {
        Command::new(&exe)
            .args([
                "test_second_process_waits_for_the_first",
                "--",
                "--nocapture",
            ])
            .env(CHILD_ENV, path.to_str().unwrap())
            .env(CHILD_WAIT, wait_ms.to_string())
            // 子进程的测试框架输出（"running 1 test"）与父进程的混在一起会让人以为
            // 测试跑了两遍。结果通过退出码回报，因此这里不需要它的 stdout。
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
    };

    // --- 1. 父进程持有写锁；子进程不等待 -> 必须被拒 ---
    let holder = NervusDb::open(&path)?;
    let status = spawn_child(0)?;
    assert_eq!(
        status.code(),
        Some(EXIT_LOCKED),
        "a second PROCESS must be refused while the parent holds the write lock \
         (exit {EXIT_LOCKED} expected, got {:?})",
        status.code()
    );

    // --- 2. 父进程仍然有效：子进程的失败没有破坏它 ---
    holder.execute("CREATE (:Parent {v: 2})")?;
    drop(holder);

    // --- 3. 父进程释放后，子进程成功 ---
    let status = spawn_child(0)?;
    assert_eq!(
        status.code(),
        Some(0),
        "after the parent released, the child must succeed (got {:?})",
        status.code()
    );

    // --- 4. 子进程**等待**时能跨过父进程的短暂持有 ---
    //
    // 这是 `lock_wait_ms` 在**跨进程**场景下的核心断言：先后写入不该失败。
    let holder = NervusDb::open(&path)?;
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(250));
        drop(holder);
    });
    let status = spawn_child(5000)?;
    assert_eq!(
        status.code(),
        Some(0),
        "a child with a wait budget must survive a 250ms hold by the parent (got {:?})",
        status.code()
    );
    let _ = releaser.join();

    // --- 5. 子进程写的数据确实落盘 ---
    let db = NervusDb::open(&path)?;
    let n = db.run_cypher("MATCH (f:FromChild) RETURN count(*)")?;
    assert!(
        matches!(n.rows[0].values[0], Value::Int(v) if v >= 2),
        "the child's writes must be visible after reopen, got {:?}",
        n.rows[0].values[0]
    );

    Ok(())
}

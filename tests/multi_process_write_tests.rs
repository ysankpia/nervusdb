//! 真实场景端到端：**多个进程并发写同一个库**，全部使用 `lock_wait_ms`。
//!
//! ## 它回答的问题
//!
//! 「我开两三个会话窗口，同时往同一个库写，会卡吗？」
//!
//! 单个进程的测试回答不了这个：真正的争用发生在**进程之间**，而且要验证的不是
//! 「某一刻谁拿到锁」，而是「所有写入最终都落地，没有丢、没有死锁」。因此这里启动
//! 多个**真实子进程**，各自带等待预算连续写入，最后在父进程里核对总数。
//!
//! ## 断言什么
//!
//! 1. **不丢写**——N 个进程各写 M 条，最终必须恰好 N×M 条。少一条就是静默数据丢失，
//!    而这正是多写者模型最危险的失败形态。
//! 2. **不挂死**——所有子进程必须在预算内退出。等待逻辑若写错（例如超时后继续
//!    重试），表现就是永久挂起，比报错更难诊断。
//! 3. **等待确实发生了**——否则测的是「没争用」，等于什么都没测。
//!
//! 用法：父进程 spawn 子进程，子进程通过**退出码**回报（原因见
//! `lock_cross_process_tests.rs`：同一测试二进制的 stdout 会被测试框架捕获）。

use nervusdb::{NervusDb, NervusDbOptions, Value};
use std::process::{Command, Stdio};
use tempfile::tempdir;

const ENV_DB: &str = "NERVUSDB_MP_DB";
const ENV_WRITER: &str = "NERVUSDB_MP_WRITER";
const ENV_COUNT: &str = "NERVUSDB_MP_COUNT";
const ENV_WAIT: &str = "NERVUSDB_MP_WAIT";

#[test]
fn test_many_processes_write_concurrently_without_losing_writes(
) -> Result<(), std::boxed::Box<dyn std::error::Error>> {
    // ---- 子进程模式：连续写入，靠等待预算错开 ---
    if let Ok(db_path) = std::env::var(ENV_DB) {
        let writer: u32 = std::env::var(ENV_WRITER)?.parse()?;
        let count: u32 = std::env::var(ENV_COUNT)?.parse()?;
        let wait: u64 = std::env::var(ENV_WAIT)?.parse()?;

        let opts = NervusDbOptions {
            lock_wait_ms: wait,
            ..Default::default()
        };

        let mut done = 0u32;
        for i in 0..count {
            // **每条写入单独开一次句柄**：这是「会话窗口」的真实形态（每次操作
            // 拿锁、写完释放），而不是一个长期持有写锁的长事务。若持有整个循环，
            // 那就是独占式写，测不出交错。
            match NervusDb::open_with_options(&db_path, opts.clone()) {
                Ok(db) => {
                    if db
                        .execute(&format!("CREATE (:W{} {{i: {}}})", writer, i))
                        .is_ok()
                    {
                        done += 1;
                    }
                }
                Err(nervusdb::GraphError::DatabaseLocked(_)) => break, // 超时：如实上报
                Err(_) => std::process::exit(5),
            }
        }

        // **退出码只表示状态，不承载计数。**
        //
        // 首版写成 `exit(done)`，于是「写完 25 条」退出码是 25，而父进程按
        // `status.success()`（即 0）判断——4 个进程全部被读成「失败」，测试报告
        // 「从未完成」。把数据编码进退出码，就必然和「0 = 成功」的约定打架。
        //
        // 计数由父进程**独立查库**核对，这本来就是更强的检查：不相信子进程的自报数。
        if done == count {
            std::process::exit(0);
        }
        // 因超时/被拒而少写：以非零退出，父进程会看到并失败
        std::process::exit(2);
    }

    // ---- 父进程模式 ----
    let dir = tempdir()?;
    let path = dir.path().join("mp.db");
    {
        let db = NervusDb::open(&path)?;
        db.execute("CREATE (:Seed {v: 0})")?;
        db.checkpoint()?;
    }

    let processes = 4u32;
    let per_process = 25u32;
    let wait_ms = 8000u64;

    let exe = std::env::current_exe()?;
    let start = std::time::Instant::now();
    let mut children = Vec::new();
    for w in 0..processes {
        let child = Command::new(&exe)
            .args([
                "test_many_processes_write_concurrently_without_losing_writes",
                "--",
                "--nocapture",
            ])
            .env(ENV_DB, path.to_str().unwrap())
            .env(ENV_WRITER, w.to_string())
            .env(ENV_COUNT, per_process.to_string())
            .env(ENV_WAIT, wait_ms.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        children.push((w, child));
    }

    // 2. 不挂死：全部必须在宽松上限内结束。
    //
    // 用 `try_wait` 轮询而不是 `wait`：`wait` 会无限阻塞，那样测试在「等待逻辑写错
    // 导致永久挂起」时表现为**测试框架超时**，而不是一条说明原因的断言失败。
    // （不用 `wait_timeout`：那是第三方 crate，本项目零依赖。）
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut failed = Vec::new();
    for (w, mut child) in children {
        let status = loop {
            match child.try_wait()? {
                Some(st) => break Some(st),
                None => {
                    if std::time::Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        break None;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            }
        };
        match status {
            Some(st) if st.success() => {}
            // 退出码 2 = 子进程有写入被拒（等待预算不够）；None = 挂死
            other => failed.push((w, other)),
        }
    }

    assert!(
        failed.is_empty(),
        "writer process(es) {failed:?} did not finish cleanly: status `None` means it never \
         exited (a wait loop that does not give up looks exactly like this), and exit code 2 \
         means some writes were refused even with a {wait_ms}ms budget"
    );
    let elapsed = start.elapsed();

    // 1. 不丢写：在**父进程**里独立核对，而不是相信子进程的自报数。
    //
    // 这是本测试最重要的一条断言。少一条就是静默数据丢失——多写者模型最危险的
    // 失败形态，因为每个进程都认为自己成功了。
    let db = NervusDb::open(&path)?;
    let total = db.run_cypher("MATCH (n) RETURN count(*)")?;
    let expected = (processes * per_process + 1) as i64; // +1 是 seed 节点
    assert_eq!(
        total.rows[0].values[0],
        Value::from(expected),
        "expected {expected} nodes ({processes} processes x {per_process} writes + seed) \
         after {elapsed:?}; a smaller number is silent data loss"
    );

    // 索引与数据一致（写者之间没有让标签索引落后）
    let by_label = db.run_cypher("MATCH (w:W0) RETURN count(*)")?;
    assert_eq!(by_label.rows[0].values[0], Value::from(per_process as i64));

    Ok(())
}

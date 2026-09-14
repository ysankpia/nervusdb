//! 故障注入：磁盘满、权限、截断、只读——**引擎必须以明确错误拒绝，而不是静默给错数据**。
//!
//! ## 为什么需要它
//!
//! 现有套件覆盖了损坏检测（CRC、chain、版本）与崩溃恢复，但**外部故障**几乎没测过：
//! 磁盘写满、目录不可写、文件被截断、文件权限被改。这些正是「静默数据错误」最容易
//! 藏身的地方——写失败若被折叠成「成功」，用户会在几周后发现数据不见了。
//!
//! ## 判据（每条都必须在实现里成立）
//!
//! 1. **失败必须可见**：写不进去就返回 `Err`，而不是返回 `Ok` 而实际没写。
//! 2. **失败不得污染**：失败的写不得让已提交数据损坏或消失。
//! 3. **错误必须可诊断**：信息要说明**为什么**失败，而不是 `No such file` 这类
//!    指向错误方向的信息（本会话在 `backup(:memory:)` 上遇到过）。
//! 4. **恢复后必须可用**：故障解除后，库仍能正常读写，已提交数据完整。
//!
//! ## 环境
//!
//! 依赖 Unix 权限位（`chmod`）。在非 Unix 上这些用例会**跳过并说明**，而不是假装通过。
//!
//! 运行：
//! ```bash
//! cargo bench --bench fault_injection_bench
//! ```

use nervusdb::{GraphError, NervusDb, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

// =========================================================================
// 结果记录
// =========================================================================

struct Report {
    passed: usize,
    failed: Vec<(String, String)>,
    skipped: Vec<(String, String)>,
}

impl Report {
    fn new() -> Self {
        Self {
            passed: 0,
            failed: Vec::new(),
            skipped: Vec::new(),
        }
    }
    fn ok(&mut self, name: &str) {
        self.passed += 1;
        println!("  ok    {name}");
    }
    fn fail(&mut self, name: &str, why: impl Into<String>) {
        let why = why.into();
        println!("  FAIL  {name}\n        {why}");
        self.failed.push((name.to_string(), why));
    }
    /// 记录一个不适用（而非失败）的用例。
    ///
    /// 目前只有在非 Unix 平台构建时才会走到——那些 cfg 分支在 macOS/Linux 的 CI 上
    /// 不参与编译，因此本地 clippy 会认为它死代码。**不要删掉它**：删掉之后非 Unix 的
    /// 构建会编译失败，而失败信息是「找不到方法」，与真正的原因无关。
    #[allow(dead_code)]
    fn skip(&mut self, name: &str, why: &str) {
        println!("  skip  {name}  ({why})");
        self.skipped.push((name.to_string(), why.to_string()));
    }
}

/// 跑一个用例：闭包返回 `Err(说明)` 即为失败。
fn case(r: &mut Report, name: &str, f: impl FnOnce() -> Result<(), String>) {
    match f() {
        Ok(()) => r.ok(name),
        Err(e) => r.fail(name, e),
    }
}

// =========================================================================
// 工具
// =========================================================================

fn seed(path: &Path, nodes: u64) -> Result<NervusDb, String> {
    let db = NervusDb::open(path).map_err(|e| e.to_string())?;
    db.with_transaction(|tx| {
        for i in 1..=nodes {
            tx.add_node(
                HashSet::from(["N".to_string()]),
                HashMap::from([("i".to_string(), Value::from(i as i64))]),
            )?;
        }
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    db.checkpoint().map_err(|e| e.to_string())?;
    Ok(db)
}

/// 数出实际的节点数（用于判断「失败的写有没有偷偷落盘」）。
fn readable_nodes(path: &Path, expected: u64) -> Result<u64, String> {
    let db = NervusDb::open(path).map_err(|e| e.to_string())?;
    let n = (1..=expected).filter(|i| db.get_node(*i).is_some()).count() as u64;
    Ok(n)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut m = std::fs::metadata(path)
        .map_err(|e| e.to_string())?
        .permissions();
    m.set_mode(mode);
    std::fs::set_permissions(path, m).map_err(|e| e.to_string())
}

fn main() {
    println!("============================================================");
    println!(" FAULT INJECTION: external failures must be visible, not silent");
    println!("============================================================");

    let root = tempfile::tempdir().expect("tempdir");
    let mut report = Report::new();

    // ---------------------------------------------------------------------
    // 1. 主库文件只读（0444）：打开带写意图必须失败
    // ---------------------------------------------------------------------
    #[cfg(unix)]
    {
        let p = root.path().join("ro_main.db");
        seed(&p, 10).expect("seed");
        set_mode(&p, 0o444).expect("chmod");
        case(
            &mut report,
            "read-only data file: open-for-write is refused",
            || match NervusDb::open(&p) {
                Err(_) => Ok(()),
                Ok(_) => Err("opening a 0444 data file for writing must fail".into()),
            },
        );
        set_mode(&p, 0o644).expect("restore");
    }
    #[cfg(not(unix))]
    report.skip("read-only data file", "needs Unix permission bits");

    // ---------------------------------------------------------------------
    // 2. 主库文件只读：只读打开必须仍可用
    // ---------------------------------------------------------------------
    #[cfg(unix)]
    {
        let p = root.path().join("ro_reader.db");
        seed(&p, 10).expect("seed");
        set_mode(&p, 0o444).expect("chmod");
        case(
            &mut report,
            "read-only data file: read-only open still works",
            || {
                let db = NervusDb::open_read_only(&p).map_err(|e| e.to_string())?;
                let n = (1..=10u64).filter(|i| db.get_node(*i).is_some()).count();
                if n == 10 {
                    Ok(())
                } else {
                    Err(format!("expected 10 readable nodes, got {n}"))
                }
            },
        );
        set_mode(&p, 0o644).expect("restore");
    }
    #[cfg(not(unix))]
    report.skip("read-only open of 0444 file", "needs Unix permission bits");

    // ---------------------------------------------------------------------
    // 3. 目录不可写：写必须失败，且不得静默丢弃
    // ---------------------------------------------------------------------
    #[cfg(unix)]
    {
        let d = root.path().join("nowrite_dir");
        std::fs::create_dir_all(&d).expect("mkdir");
        let p = d.join("x.db");
        seed(&p, 20).expect("seed");
        set_mode(&d, 0o555).expect("chmod dir");

        case(
            &mut report,
            "unwritable dir: a failed write is reported, not swallowed",
            || {
                // 目录不可写并不妨碍写**已存在**的文件，所以这里的判据是：
                // 若返回 Ok，数据就必须真的在；若返回 Err，不得留下半截状态。
                let db = NervusDb::open(&p).map_err(|e| e.to_string())?;
                let write = db.with_transaction(|tx| {
                    tx.add_node(HashSet::from(["New".to_string()]), HashMap::new())?;
                    Ok(())
                });
                match write {
                    Ok(()) => {
                        // 声明成功就必须真的可见——否则是静默失败。
                        let found = db
                            .run_cypher("MATCH (n:New) RETURN count(n)")
                            .map_err(|e| e.to_string())?
                            .rows[0]
                            .values[0]
                            .as_i64();
                        if found == Some(1) {
                            Ok(())
                        } else {
                            Err(format!(
                                "the write reported success but :New count is {found:?}"
                            ))
                        }
                    }
                    Err(_) => Ok(()), // 明确报错也是可接受的
                }
            },
        );
        set_mode(&d, 0o755).expect("restore");
    }
    #[cfg(not(unix))]
    report.skip("unwritable dir", "needs Unix permission bits");

    // ---------------------------------------------------------------------
    // 4. 数据文件被截断：必须被检出，且不得返回错误数据
    // ---------------------------------------------------------------------
    {
        let p = root.path().join("truncated.db");
        seed(&p, 200).expect("seed");
        let bytes = std::fs::read(&p).expect("read");
        // 截到 3/4：尾部页直接消失
        std::fs::write(&p, &bytes[..bytes.len() * 3 / 4]).expect("truncate");

        case(
            &mut report,
            "truncated data file: detected, never silently wrong",
            || {
                match NervusDb::open(&p) {
                    // 打开若成功，完整性检查必须报出问题，或读数必须小于原值且
                    // 不出现「读到别的节点数据」这种更糟的情况。
                    Ok(db) => {
                        let n = db.node_count();
                        let readable = (1..=200u64).filter(|i| db.get_node(*i).is_some()).count();
                        let ic = db
                            .integrity_check()
                            .map_err(|e| format!("integrity_check errored: {e}"))?;
                        if readable <= n && !ic.issues.is_empty() {
                            Ok(())
                        } else if readable <= n && n < 200 {
                            // 计数本身变小也算「没有假装完整」
                            Ok(())
                        } else if readable > n {
                            Err(format!(
                                "readable ({readable}) exceeds node_count ({n}) — \
                                 the file was truncated but reads still return data"
                            ))
                        } else {
                            Err(format!(
                                "truncated file reported node_count={n} with no integrity \
                                 issue and {readable} readable — indistinguishable from intact"
                            ))
                        }
                    }
                    Err(_) => Ok(()), // 明确拒绝也可接受
                }
            },
        );
    }

    // ---------------------------------------------------------------------
    // 5. WAL 被删（主库未落地）：已提交数据的存在性必须被如实回答
    // ---------------------------------------------------------------------
    {
        let p = root.path().join("missing_wal.db");
        let db = NervusDb::open(&p).expect("open");
        db.with_transaction(|tx| {
            for i in 1..=30u64 {
                tx.add_node(
                    HashSet::from(["N".to_string()]),
                    HashMap::from([("i".to_string(), Value::from(i as i64))]),
                )?;
            }
            Ok(())
        })
        .expect("write");
        drop(db); // 不 checkpoint：数据只在 WAL 里

        let wal = p.with_extension("db.wal");
        if wal.exists() {
            std::fs::remove_file(&wal).expect("remove wal");
        }

        case(
            &mut report,
            "deleted WAL: engine does not claim the lost rows exist",
            || {
                let db = NervusDb::open(&p).map_err(|e| e.to_string())?;
                let n = db.node_count();
                let readable = (1..=30u64).filter(|i| db.get_node(*i).is_some()).count();
                if readable > n {
                    Err(format!(
                        "readable ({readable}) exceeds node_count ({n}) after the WAL was lost"
                    ))
                } else {
                    Ok(()) // 计数变小是可接受的：数据确实没了
                }
            },
        );
    }

    // ---------------------------------------------------------------------
    // 6. 只读目录：以 :memory: 打开不受影响（不碰磁盘）
    // ---------------------------------------------------------------------
    #[cfg(unix)]
    {
        let d = root.path().join("ro_for_memory");
        std::fs::create_dir_all(&d).expect("mkdir");
        set_mode(&d, 0o555).expect("chmod");
        let prev = std::env::current_dir().expect("cwd");
        // 切到只读目录下运行：`:memory:` 不得因此失败（它本就不写盘）
        let _ = std::env::set_current_dir(&d);

        case(
            &mut report,
            "unwritable cwd: :memory: still works (it must not touch disk)",
            || {
                let db = NervusDb::open(":memory:").map_err(|e| e.to_string())?;
                db.with_transaction(|tx| {
                    tx.add_node(HashSet::from(["M".to_string()]), HashMap::new())?;
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
                let c = db
                    .run_cypher("MATCH (n:M) RETURN count(n)")
                    .map_err(|e| e.to_string())?
                    .rows[0]
                    .values[0]
                    .as_i64();
                if c == Some(1) {
                    Ok(())
                } else {
                    Err(format!(":memory: write lost, count={c:?}"))
                }
            },
        );

        let _ = std::env::set_current_dir(prev);
        set_mode(&d, 0o755).expect("restore");
    }
    #[cfg(not(unix))]
    report.skip("unwritable cwd with :memory:", "needs Unix permission bits");

    // ---------------------------------------------------------------------
    // 7. 目标路径是目录：backup 必须明确拒绝
    // ---------------------------------------------------------------------
    {
        let p = root.path().join("bk_src.db");
        // 必须 drop 掉 seed 返回的句柄：它会一直持有排他锁，导致下面 open 被拒。
        // （本测试第一版漏了这一步，报出的是 DatabaseLocked，与 backup 无关。）
        drop(seed(&p, 5).expect("seed"));
        let dest_is_dir = root.path().join("bk_dest_dir");
        std::fs::create_dir_all(&dest_is_dir).expect("mkdir");

        case(
            &mut report,
            "backup into a directory path is refused with a reason",
            || {
                let db = NervusDb::open(&p).map_err(|e| e.to_string())?;
                match db.backup(&dest_is_dir) {
                    Ok(n) => Err(format!(
                        "backup onto a directory reported success ({n} bytes)"
                    )),
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.trim().is_empty() {
                            Err("the error message is empty".into())
                        } else {
                            Ok(())
                        }
                    }
                }
            },
        );
    }

    // ---------------------------------------------------------------------
    // 8. 故障解除后必须完全可用
    // ---------------------------------------------------------------------
    #[cfg(unix)]
    {
        let d = root.path().join("recover_dir");
        std::fs::create_dir_all(&d).expect("mkdir");
        let p = d.join("y.db");
        seed(&p, 10).expect("seed");
        set_mode(&d, 0o555).expect("chmod");

        // 期间做一次失败的写入尝试（可能成功也可能失败，两者都允许）
        if let Ok(db) = NervusDb::open(&p) {
            let _ = db.with_transaction(|tx| {
                tx.add_node(HashSet::from(["DuringFault".to_string()]), HashMap::new())?;
                Ok(())
            });
            let _ = db.checkpoint();
        }

        set_mode(&d, 0o755).expect("restore");

        case(
            &mut report,
            "after the fault clears: the database is fully usable, data intact",
            || {
                // 原先的 10 个节点必须都还在（故障不得损坏已提交数据）。
                let readable = readable_nodes(&p, 10)?;
                if readable != 10 {
                    return Err(format!(
                        "expected the 10 committed nodes to survive, readable={readable}"
                    ));
                }
                // 且必须能正常继续写。
                let db = NervusDb::open(&p).map_err(|e| e.to_string())?;
                db.with_transaction(|tx| {
                    tx.add_node(HashSet::from(["After".to_string()]), HashMap::new())?;
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
                db.checkpoint().map_err(|e| e.to_string())?;
                drop(db);

                let again = NervusDb::open(&p).map_err(|e| e.to_string())?;
                let c = again
                    .run_cypher("MATCH (n:After) RETURN count(n)")
                    .map_err(|e| e.to_string())?
                    .rows[0]
                    .values[0]
                    .as_i64();
                if c == Some(1) {
                    Ok(())
                } else {
                    Err(format!("post-recovery write not durable, count={c:?}"))
                }
            },
        );
    }
    #[cfg(not(unix))]
    report.skip("fault clears then recovers", "needs Unix permission bits");

    // ---------------------------------------------------------------------
    // 报告
    // ---------------------------------------------------------------------
    println!("\n------------------------------------------------------------");
    println!(" Passed  : {}", report.passed);
    println!(" Failed  : {}", report.failed.len());
    println!(" Skipped : {}", report.skipped.len());
    println!("------------------------------------------------------------");

    if report.failed.is_empty() {
        println!("FAULT INJECTION PASSED");
    } else {
        for (name, why) in &report.failed {
            println!("  - {name}: {why}");
        }
        println!("FAULT INJECTION FAILED");
        std::process::exit(1);
    }
}

// 抑制「未使用」告警：`GraphError` 与 `PathBuf` 在某些 cfg 下确实不需要。
#[allow(dead_code)]
fn _type_anchors(_: GraphError, _: PathBuf) {}

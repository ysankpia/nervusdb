//! 页边界穷举：跨过直接页覆盖面、页目录层级、属性页槽位等**结构性边界**。
//!
//! ## 为什么它和普通测试不同
//!
//! 普通测试用「几百个节点」验证语义，规模远小于任何结构性边界。而本项目的寻址是
//! 固定公式：
//!
//! - 每页 128 条节点记录（32B）/ 64 条边记录（64B）；
//! - Page 0 内联 **32 个**直接节点目录页号（覆盖前 4096 个节点）；
//! - 同样 32 个直接边目录页号（覆盖前 2048 条边）；
//! - 超出后走**间接目录页**，目录页本身也可能多级。
//!
//! 这些数字是**魔数边界**：127/128/129 个节点、2047/2048/2049 条边、4095/4096/4097
//! 个节点。越界出错时症状是「读到别的实体的数据」——静默、且看起来像数据错乱而不是
//! 边界问题。因此每个边界都要**在其两侧各测一次**。
//!
//! 运行：
//! ```bash
//! cargo bench --bench page_boundary_bench
//! cargo bench --bench page_boundary_bench -- --quick   # 只跑边界，不跑完整扫描
//! ```

use nervusdb::{NervusDb, Value};
use std::collections::{HashMap, HashSet};

struct Report {
    passed: usize,
    failed: Vec<(String, String)>,
}

impl Report {
    fn new() -> Self {
        Self {
            passed: 0,
            failed: Vec::new(),
        }
    }
    fn case(&mut self, name: &str, f: impl FnOnce() -> Result<String, String>) {
        match f() {
            Ok(msg) => {
                self.passed += 1;
                println!("  ok    {name:<44} {msg}");
            }
            Err(e) => {
                println!("  FAIL  {name}\n        {e}");
                self.failed.push((name.to_string(), e));
            }
        }
    }
}

fn open(name: &str) -> Result<(tempfile::TempDir, NervusDb), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let db = NervusDb::open(dir.path().join(name)).map_err(|e| e.to_string())?;
    Ok((dir, db))
}

/// 建 n 个节点，每个带一个可校验的整数属性。
fn fill_nodes(db: &NervusDb, n: u64) -> Result<(), String> {
    db.with_transaction(|tx| {
        for i in 1..=n {
            tx.add_node(
                HashSet::from(["N".to_string()]),
                HashMap::from([("i".to_string(), Value::from(i as i64))]),
            )?;
        }
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 全部读回校验：节点 i 必须存在，且其 `i` 属性必须**恰好**是 i。
///
/// 只查「存在」不够——越界读会返回**别的节点的数据**，那时 `get_node` 仍是 `Some`。
fn verify_nodes(db: &NervusDb, n: u64) -> Result<(), String> {
    let mut wrong = Vec::new();
    for i in 1..=n {
        match db.get_node(i) {
            None => wrong.push(format!("{i}: missing")),
            Some(node) => {
                let got = node.get_prop("i").and_then(|v| v.as_i64());
                if got != Some(i as i64) {
                    wrong.push(format!("{i}: prop i={got:?}"));
                }
            }
        }
        if wrong.len() > 5 {
            break;
        }
    }
    if wrong.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} node(s) wrong or missing: {:?}",
            wrong.len(),
            wrong
        ))
    }
}

fn fill_edges(db: &NervusDb, n: u64, nodes: u64) -> Result<(), String> {
    db.with_transaction(|tx| {
        for i in 0..n {
            let src = (i % nodes) + 1;
            let dst = ((i * 7919 + 13) % nodes) + 1;
            tx.add_edge(src, dst, "R", HashMap::new(), 1.0)?;
        }
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn verify_edges(db: &NervusDb, n: u64) -> Result<(), String> {
    let count = db.edge_count();
    if count < n as usize {
        return Err(format!("edge_count={count}, expected at least {n}"));
    }
    Ok(())
}

fn main() {
    println!("============================================================");
    println!(" PAGE BOUNDARIES: both sides of every structural edge");
    println!("============================================================");
    let quick = std::env::args().any(|a| a == "--quick");
    let mut r = Report::new();

    // ---------------------------------------------------------------
    // 节点记录密度边界：128 条/页
    // ---------------------------------------------------------------
    for n in [1u64, 127, 128, 129, 255, 256, 257] {
        r.case(&format!("nodes={n} (record-per-page boundary)"), || {
            let (_d, db) = open(&format!("n{n}.db"))?;
            fill_nodes(&db, n)?;
            if db.node_count() != n as usize {
                return Err(format!("node_count={}, expected {n}", db.node_count()));
            }
            verify_nodes(&db, n)?;
            Ok(format!("count={}", db.node_count()))
        });
    }

    // ---------------------------------------------------------------
    // 直接目录页边界：覆盖前 4096 个节点（32 页 × 128）
    // ---------------------------------------------------------------
    for n in [4095u64, 4096, 4097] {
        r.case(&format!("nodes={n} (direct-page coverage edge)"), || {
            let (_d, db) = open(&format!("d{n}.db"))?;
            fill_nodes(&db, n)?;
            verify_nodes(&db, n)?;
            // 跨边界时目录页数量会增加；checkpoint 后重开仍须一致。
            db.checkpoint().map_err(|e| e.to_string())?;
            drop(db);
            let path = _d.path().join(format!("d{n}.db"));
            let re = NervusDb::open(&path).map_err(|e| e.to_string())?;
            if re.node_count() != n as usize {
                return Err(format!(
                    "after reopen node_count={}, expected {n}",
                    re.node_count()
                ));
            }
            verify_nodes(&re, n)?;
            Ok(format!("count={n}, reopen OK"))
        });
    }

    // ---------------------------------------------------------------
    // 直接边目录页边界：覆盖前 2048 条边（32 页 × 64）
    // ---------------------------------------------------------------
    for e in [2047u64, 2048, 2049] {
        r.case(&format!("edges={e} (direct-edge-page edge)"), || {
            let (_d, db) = open(&format!("e{e}.db"))?;
            let nodes = 300u64; // 足以让边分散到多页
            fill_nodes(&db, nodes)?;
            fill_edges(&db, e, nodes)?;
            verify_edges(&db, e)?;
            verify_nodes(&db, nodes)?;
            Ok(format!("edges={}", db.edge_count()))
        });
    }

    // ---------------------------------------------------------------
    // 属性页槽位边界：一页内多记录共享槽位数组
    // ---------------------------------------------------------------
    for n in [1u64, 15, 16, 17, 63, 64, 65] {
        r.case(&format!("{n} nodes sharing slotted pages"), || {
            let (_d, db) = open(&format!("s{n}.db"))?;
            db.with_transaction(|tx| {
                for i in 1..=n {
                    // 每条 ~200B，使一页内可放多条但又不至溢出
                    let mut props = HashMap::new();
                    props.insert("i".to_string(), Value::from(i as i64));
                    props.insert("pad".to_string(), Value::from("x".repeat(180)));
                    tx.add_node(HashSet::from(["S".to_string()]), props)?;
                }
                Ok(())
            })
            .map_err(|e| e.to_string())?;
            for i in 1..=n {
                let node = db.get_node(i).ok_or(format!("node {i} missing"))?;
                if node.get_prop("i").and_then(|v| v.as_i64()) != Some(i as i64) {
                    return Err(format!("node {i} has wrong prop"));
                }
                if node
                    .get_prop("pad")
                    .and_then(|v| v.as_str())
                    .map(|s| s.len())
                    != Some(180)
                {
                    return Err(format!("node {i} lost its padding payload"));
                }
            }
            Ok(format!("{n} nodes verified"))
        });
    }

    // ---------------------------------------------------------------
    // 内联/溢出属性边界：1KB
    // ---------------------------------------------------------------
    for size in [1023usize, 1024, 1025] {
        r.case(
            &format!("property payload {size}B (inline/overflow edge)"),
            || {
                let (_d, db) = open(&format!("p{size}.db"))?;
                let id = db
                    .add_node(
                        HashSet::from(["P".to_string()]),
                        HashMap::from([("blob".to_string(), Value::from("z".repeat(size)))]),
                    )
                    .map_err(|e| e.to_string())?;
                let got = db
                    .get_node(id)
                    .and_then(|n| {
                        n.get_prop("blob")
                            .and_then(|v| v.as_str().map(String::from))
                    })
                    .ok_or("property unreadable")?;
                if got.len() != size {
                    return Err(format!("payload length {} != {size}", got.len()));
                }
                // 冷重启后仍须一致（溢出链要能从磁盘重建）
                db.checkpoint().map_err(|e| e.to_string())?;
                drop(db);
                let path = _d.path().join(format!("p{size}.db"));
                let re = NervusDb::open(&path).map_err(|e| e.to_string())?;
                let again = re
                    .get_node(id)
                    .and_then(|n| {
                        n.get_prop("blob")
                            .and_then(|v| v.as_str().map(String::from))
                    })
                    .ok_or("property unreadable after reopen")?;
                if again.len() != size {
                    return Err(format!("after reopen payload {} != {size}", again.len()));
                }
                Ok(format!("{size}B round trip + reopen"))
            },
        );
    }

    // ---------------------------------------------------------------
    // 小池 + 跨边界：STEAL 溢出路径在结构性边界上是否仍正确
    // ---------------------------------------------------------------
    if !quick {
        for n in [4095u64, 4097] {
            r.case(
                &format!("nodes={n} with a 64-frame pool (spill path)"),
                || {
                    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
                    let path = dir.path().join(format!("tiny{n}.db"));
                    let db = NervusDb::open_with_pool_size(&path, 64).map_err(|e| e.to_string())?;
                    fill_nodes(&db, n)?;
                    verify_nodes(&db, n)?;
                    db.checkpoint().map_err(|e| e.to_string())?;
                    drop(db);
                    let re = NervusDb::open_with_pool_size(&path, 64).map_err(|e| e.to_string())?;
                    verify_nodes(&re, n)?;
                    Ok(format!("{n} nodes through a 64-frame pool"))
                },
            );
        }
    }

    println!("\n------------------------------------------------------------");
    println!(" Passed: {}", r.passed);
    println!(" Failed: {}", r.failed.len());
    println!("------------------------------------------------------------");
    if r.failed.is_empty() {
        println!("PAGE BOUNDARY PASSED");
    } else {
        for (n, e) in &r.failed {
            println!("  - {n}: {e}");
        }
        println!("PAGE BOUNDARY FAILED");
        std::process::exit(1);
    }
}

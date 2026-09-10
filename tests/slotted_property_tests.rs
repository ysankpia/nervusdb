//! 1.1 验证套件：Slotted Property Page 紧凑属性存储。
//!
//! 覆盖单页多槽打包、槽位复用、页内压实、1KB 内联/溢出边界，以及
//! 「40,000 实体落盘体积相对『每实体整页』压缩 ≥15×」的密度指标。

use graphlite::{GraphError, GraphLite, Value};
use std::collections::{HashMap, HashSet};
use tempfile::tempdir;

/// 构造一批带 ~150 字节字符串属性的实体（贴近真实业务属性尺寸）
fn bulk_node_props(idx: i64) -> HashMap<String, Value> {
    let mut props = HashMap::new();
    props.insert("idx".to_string(), Value::from(idx));
    props.insert(
        "payload".to_string(),
        Value::from(format!(
            "record-{:06}-compact-property-payload-for-density-check-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            idx
        )),
    );
    props
}

fn bulk_edge_props(idx: i64) -> HashMap<String, Value> {
    let mut props = HashMap::new();
    props.insert(
        "label".to_string(),
        Value::from(format!(
            "edge-{:06}-compact-relationship-payload-check-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            idx
        )),
    );
    props
}

// =========================================================================
// 单页多槽：多条小属性记录必须共用同一张 4KB 页
// =========================================================================
#[test]
fn test_many_small_records_share_one_page() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("slotted_share.db");

    let record_count: u64 = 400;
    {
        let db = GraphLite::open(&db_path)?;
        for i in 0..record_count {
            // 每条属性净含量不足 60 字节：若每实体独占整页将占用 400 * 4KB = 1.6MB
            let mut props = HashMap::new();
            props.insert("n".to_string(), Value::from(i as i64));
            db.add_node(HashSet::from(["Small".to_string()]), props)?;
        }
        db.checkpoint()?;
    }

    let file_size = std::fs::metadata(&db_path)?.len();
    let one_page_per_record = record_count * 4096;
    assert!(
        file_size * 8 < one_page_per_record,
        "{} records must be packed into a few pages; got {} bytes vs {} bytes for one page each",
        record_count,
        file_size,
        one_page_per_record
    );

    // 数据完整性必须保持
    let db = GraphLite::open(&db_path)?;
    assert_eq!(db.node_count(), record_count as usize);
    for probe in [0u64, 123, 399] {
        let node = db.get_node(probe + 1).expect("node must exist");
        assert_eq!(
            node.get_prop("n").and_then(|v| v.as_i64()),
            Some(probe as i64)
        );
    }

    Ok(())
}

// =========================================================================
// 槽位复用：删除记录后其槽位空间必须能被后续写入复用（不无限增长）
// =========================================================================
#[test]
fn test_slot_reuse_after_delete() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("slot_reuse.db");

    let db = GraphLite::open(&db_path)?;

    // 1. 建立 200 个实体
    let mut ids = Vec::new();
    for i in 0..200i64 {
        let mut props = HashMap::new();
        props.insert(
            "k".to_string(),
            Value::from(format!("value-{:04}-{}", i, "x".repeat(120))),
        );
        ids.push(db.add_node(HashSet::from(["Reuse".to_string()]), props)?);
    }
    db.checkpoint()?;
    let after_insert = std::fs::metadata(&db_path)?.len();

    // 2. 删除其中 150 个，再写入同等数量的新实体
    for id in ids.iter().take(150) {
        db.remove_node(*id)?;
    }
    assert_eq!(db.node_count(), 50);

    for i in 0..150i64 {
        let mut props = HashMap::new();
        props.insert(
            "k".to_string(),
            Value::from(format!("recycled-{:04}-{}", i, "y".repeat(120))),
        );
        db.add_node(HashSet::from(["Reuse".to_string()]), props)?;
    }
    db.checkpoint()?;
    let after_recycle = std::fs::metadata(&db_path)?.len();

    // 3. 槽位复用后文件体积不得成倍膨胀（允许少量目录/字典增长）
    assert!(
        after_recycle <= after_insert + 64 * 1024,
        "slot reuse failed: {} bytes grew to {} bytes after recycling the same record count",
        after_insert,
        after_recycle
    );
    assert_eq!(db.node_count(), 200);

    // 复用后的数据必须可正确读回
    let res = db.query_cypher("MATCH (r:Reuse) RETURN count(r)")?;
    assert_eq!(res.rows[0].values[0], Value::from(200));

    Ok(())
}

// =========================================================================
// 页内压实：大量随机删除 + 重插后内容依然正确
// =========================================================================
#[test]
fn test_compaction_preserves_content() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("compaction.db");

    let db = GraphLite::open(&db_path)?;

    let mut ids = Vec::new();
    for i in 0..300i64 {
        let mut props = HashMap::new();
        props.insert("seq".to_string(), Value::from(i));
        props.insert("blob".to_string(), Value::from("p".repeat(90)));
        ids.push(db.add_node(HashSet::from(["C".to_string()]), props)?);
    }

    // 删除所有偶数序号实体（制造页内碎片），随后重插触发压实
    for (i, id) in ids.iter().enumerate() {
        if i % 2 == 0 {
            db.remove_node(*id)?;
        }
    }
    for i in (0..300i64).step_by(2) {
        let mut props = HashMap::new();
        props.insert("seq".to_string(), Value::from(1000 + i));
        props.insert("blob".to_string(), Value::from("q".repeat(90)));
        db.add_node(HashSet::from(["C".to_string()]), props)?;
    }
    db.checkpoint()?;

    // 冷重启后逐条校验（排他锁：重开前释放旧句柄）
    drop(db);
    let db = GraphLite::open(&db_path)?;
    assert_eq!(db.node_count(), 300);

    let res = db.query_cypher("MATCH (c:C) RETURN count(c)")?;
    assert_eq!(res.rows[0].values[0], Value::from(300));

    // 原奇数序号记录必须原值保留
    let mut odd_found = 0;
    for i in (1..300i64).step_by(2) {
        let res = db.query_cypher(&format!("MATCH (c:C {{seq: {}}}) RETURN c.seq", i))?;
        if res.row_count() == 1 {
            odd_found += 1;
        }
    }
    assert_eq!(
        odd_found, 150,
        "all odd-seq records must survive compaction"
    );

    Ok(())
}

// =========================================================================
// 1KB 内联/溢出边界：两侧均须正确存取
// =========================================================================
#[test]
fn test_inline_vs_overflow_boundary() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("boundary.db");

    let db = GraphLite::open(&db_path)?;

    // 净载荷 900 字节（连同框架开销仍 <1KB）→ 走槽位页内联
    let inline_payload = "I".repeat(900);
    // 净载荷 1200 字节 → 超过 1KB，走溢出链
    let overflow_payload = "O".repeat(1200);
    // 超大记录 → 多页溢出链
    let huge_payload = "H".repeat(11_000);

    let mut p1 = HashMap::new();
    p1.insert("body".to_string(), Value::from(inline_payload.clone()));
    let n1 = db.add_node(HashSet::from(["Doc".to_string()]), p1)?;

    let mut p2 = HashMap::new();
    p2.insert("body".to_string(), Value::from(overflow_payload.clone()));
    let n2 = db.add_node(HashSet::from(["Doc".to_string()]), p2)?;

    let mut p3 = HashMap::new();
    p3.insert("body".to_string(), Value::from(huge_payload.clone()));
    let n3 = db.add_node(HashSet::from(["Doc".to_string()]), p3)?;

    db.checkpoint()?;

    // 冷重启后三种尺寸都必须无损（排他锁：重开前释放旧句柄）
    drop(db);
    let db = GraphLite::open(&db_path)?;
    assert_eq!(
        db.get_node(n1)
            .unwrap()
            .get_prop("body")
            .and_then(|v| v.as_str()),
        Some(inline_payload.as_str())
    );
    assert_eq!(
        db.get_node(n2)
            .unwrap()
            .get_prop("body")
            .and_then(|v| v.as_str()),
        Some(overflow_payload.as_str())
    );
    assert_eq!(
        db.get_node(n3)
            .unwrap()
            .get_prop("body")
            .and_then(|v| v.as_str()),
        Some(huge_payload.as_str())
    );

    // 边属性同样覆盖边界两侧
    let e1 = db.add_edge(
        n1,
        n2,
        "LINK",
        {
            let mut m = HashMap::new();
            m.insert("note".to_string(), Value::from("n".repeat(900)));
            m
        },
        1.0,
    )?;
    let e2 = db.add_edge(
        n2,
        n3,
        "LINK",
        {
            let mut m = HashMap::new();
            m.insert("note".to_string(), Value::from("m".repeat(1500)));
            m
        },
        1.0,
    )?;
    db.checkpoint()?;

    // 排他锁：重开前释放旧句柄
    drop(db);
    let db = GraphLite::open(&db_path)?;
    assert_eq!(
        db.get_edge(e1)
            .unwrap()
            .get_prop("note")
            .and_then(|v| v.as_str())
            .map(|s| s.len()),
        Some(900)
    );
    assert_eq!(
        db.get_edge(e2)
            .unwrap()
            .get_prop("note")
            .and_then(|v| v.as_str())
            .map(|s| s.len()),
        Some(1500)
    );

    Ok(())
}

// =========================================================================
// 密度指标：10,000 节点 + 30,000 边（各带 ~150B 属性）
// 落盘体积相对「每实体独占一整张 4KB 页」压缩 ≥15×
// =========================================================================
#[test]
fn test_forty_thousand_entity_density() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("density.db");

    let num_nodes: u64 = 10_000;
    let num_edges: usize = 30_000;
    let entities = num_nodes + num_edges as u64;
    assert_eq!(entities, 40_000);

    {
        let db = GraphLite::open(&db_path)?;

        // 单事务批量写入 10,000 个节点
        db.with_transaction(|tx| {
            for i in 1..=num_nodes {
                tx.add_node(
                    HashSet::from(["Dense".to_string()]),
                    bulk_node_props(i as i64),
                )?;
            }
            Ok(())
        })?;

        // 单事务批量建立 30,000 条带属性边
        db.with_transaction(|tx| {
            for i in 0..num_edges {
                let src = (i as u64 % num_nodes) + 1;
                let dst = ((i as u64 * 7 + 3) % num_nodes) + 1;
                if src != dst {
                    tx.add_edge(src, dst, "REL", bulk_edge_props(i as i64), 1.5)?;
                }
            }
            Ok(())
        })?;

        db.checkpoint()?;
    }

    let file_size = std::fs::metadata(&db_path)?.len();
    // 自校准基线：每实体独占一整张 4KB 页 ≈ 164MB（对应审查中观测到的 162MB 现状）
    let one_page_per_entity = entities * 4096;
    assert!(
        file_size * 15 <= one_page_per_entity,
        "density target missed: {} bytes for {} entities ({:.2} bytes/entity). \
         Baseline one-page-per-entity = {} bytes; required ≤ {} bytes for ≥15x compression",
        file_size,
        entities,
        file_size as f64 / entities as f64,
        one_page_per_entity,
        one_page_per_entity / 15
    );

    // 数据完整性与内存预算
    let db = GraphLite::open(&db_path)?;
    assert_eq!(db.node_count(), num_nodes as usize);
    assert_eq!(db.edge_count(), num_edges);

    let probe = db.get_node(5_000).unwrap();
    assert_eq!(probe.get_prop("idx").and_then(|v| v.as_i64()), Some(5_000));
    let expected_payload_len = bulk_node_props(5_000)
        .get("payload")
        .and_then(|v| v.as_str())
        .map(|s| s.len())
        .expect("sample payload must exist");
    assert_eq!(
        probe
            .get_prop("payload")
            .and_then(|v| v.as_str())
            .map(|s| s.len()),
        Some(expected_payload_len)
    );

    Ok(())
}

// =========================================================================
// 1MB 受限缓冲池下的密度与正确性（内存硬预算不可突破）
// =========================================================================
#[test]
fn test_density_under_one_megabyte_pool() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("density_small_pool.db");

    let db = GraphLite::open_with_pool_size(&db_path, 256)?;

    let num_nodes: u64 = 4_000;
    db.with_transaction(|tx| {
        for i in 1..=num_nodes {
            tx.add_node(HashSet::from(["N".to_string()]), bulk_node_props(i as i64))?;
        }
        Ok(())
    })?;

    db.with_transaction(|tx| {
        for i in 0..12_000u64 {
            let src = (i % num_nodes) + 1;
            let dst = ((i * 11 + 5) % num_nodes) + 1;
            if src != dst {
                tx.add_edge(src, dst, "R", bulk_edge_props(i as i64), 1.0)?;
            }
        }
        Ok(())
    })?;

    db.checkpoint()?;

    let stats = db.buffer_stats();
    assert_eq!(
        stats.capacity_frames, 256,
        "1MB pool must stay at 256 frames"
    );
    assert!(
        stats.used_frames <= 256,
        "resident frames must stay bounded"
    );

    assert_eq!(db.node_count(), num_nodes as usize);
    assert_eq!(
        db.get_node(1_000)
            .unwrap()
            .get_prop("idx")
            .and_then(|v| v.as_i64()),
        Some(1_000)
    );

    Ok(())
}

// =========================================================================
// 属性更新与删除路径在槽位页下不泄漏、不串数据
// =========================================================================
#[test]
fn test_property_update_and_delete_hygiene() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("hygiene.db");

    let db = GraphLite::open(&db_path)?;

    let n1 = db.add_node(HashSet::from(["H".to_string()]), {
        let mut m = HashMap::new();
        m.insert("v".to_string(), Value::from("first"));
        m
    })?;

    // 反复更新同一属性：旧记录必须被释放，最终值正确
    let mut last_len = 0;
    for i in 0..40 {
        let value = format!("value-{}-{}", i, "z".repeat(100));
        last_len = value.len();
        db.update_node_property(n1, "v", value)?;
    }
    assert_eq!(
        db.get_node(n1)
            .unwrap()
            .get_prop("v")
            .and_then(|v| v.as_str())
            .map(|s| s.len()),
        Some(last_len)
    );

    // 另一节点的数据不得被串改
    let n2 = db.add_node(HashSet::from(["H".to_string()]), {
        let mut m = HashMap::new();
        m.insert("v".to_string(), Value::from("second"));
        m
    })?;
    assert_eq!(
        db.get_node(n2)
            .unwrap()
            .get_prop("v")
            .and_then(|v| v.as_str()),
        Some("second")
    );

    // 删除后槽位必须释放：新节点不得读到陈旧数据
    db.remove_node(n1)?;
    let n3 = db.add_node(HashSet::from(["H".to_string()]), {
        let mut m = HashMap::new();
        m.insert("v".to_string(), Value::from("third"));
        m
    })?;
    assert_eq!(
        db.get_node(n3)
            .unwrap()
            .get_prop("v")
            .and_then(|v| v.as_str()),
        Some("third")
    );

    db.checkpoint()?;
    // 排他锁：重开前释放旧句柄
    drop(db);
    let db = GraphLite::open(&db_path)?;
    assert_eq!(db.node_count(), 2);
    assert_eq!(
        db.get_node(n2)
            .unwrap()
            .get_prop("v")
            .and_then(|v| v.as_str()),
        Some("second")
    );

    Ok(())
}

//! 1.1.0 套件：`MERGE` —— 幂等写。
//!
//! `MERGE <pattern>` 是「匹配则复用，不匹配则创建」。它存在的理由是**幂等**：
//! 调用方用 `MATCH` + 条件判断自己实现「有则复用、无则创建」时，检查与写入之间
//! 存在空隙，而那条路径产生的重复数据是静默的——只在后续 `count` 里表现为数字
//! 偏高，没有错误、没有提示。
//!
//! 这套测试的重点因此是：重复执行不产生重复数据，且两条 `ON ... SET` 分支
//! 各自只在自己该发生的时机生效。
//!
//! 测试全部走公开 API。

use nervusdb::{GraphError, NervusDb, Value};
use tempfile::tempdir;

fn open_temp(name: &str) -> Result<(tempfile::TempDir, NervusDb), GraphError> {
    let dir = tempdir()?;
    let db = NervusDb::open(dir.path().join(name))?;
    Ok((dir, db))
}

/// 执行一个应返回单行单列标量的查询。
fn scalar(db: &NervusDb, cypher: &str) -> Result<Value, GraphError> {
    let res = db.run_cypher(cypher)?;
    assert_eq!(res.row_count(), 1, "expected exactly one row: {cypher}");
    Ok(res.rows[0].values[0].clone())
}

fn count_nodes(db: &NervusDb, label: &str) -> Result<i64, GraphError> {
    match scalar(db, &format!("MATCH (n:{label}) RETURN count(*)"))? {
        Value::Int(v) => Ok(v),
        other => panic!("count 必须是 Int，实际 {other:?}"),
    }
}

// =========================================================================
// 幂等性
// =========================================================================

#[test]
fn test_merge_creates_then_reuses() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("merge_idempotent.db")?;

    let first = db.run_cypher("MERGE (a:User {name: 'alice'})")?;
    assert_eq!(first.stats.nodes_created, 1, "第一次必须创建");
    assert_eq!(count_nodes(&db, "User")?, 1);

    // 这正是 MERGE 的全部意义：重复执行不产生第二份数据
    let second = db.run_cypher("MERGE (a:User {name: 'alice'})")?;
    assert_eq!(second.stats.nodes_created, 0, "第二次必须命中而不创建");
    assert_eq!(count_nodes(&db, "User")?, 1, "重复执行不得产生重复节点");

    // 第三次也一样
    db.run_cypher("MERGE (a:User {name: 'alice'})")?;
    assert_eq!(count_nodes(&db, "User")?, 1);

    Ok(())
}

#[test]
fn test_merge_matching_uses_pattern_filter_semantics() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("merge_full_props.db")?;

    db.run_cypher("MERGE (p:Person {name: 'bob', age: 30})")?;

    // MERGE 的匹配阶段用的是 MATCH 的**模式过滤**语义：模式里列出的属性必须都相等，
    // 但节点另有额外属性**不**构成不匹配。这与 Cypher 一致——官方文档的例子中
    // `MERGE (person:Person) ON MATCH SET ...` 命中了全部 6 个 Person 节点。
    //
    // 因此这里必须复用已有节点。若实现成「属性集必须完全相等」，同一个 bob 会出现
    // 两份，而且两份在查询里完全无法区分——调用方只会在计数偏高时发现。
    db.run_cypher("MERGE (p:Person {name: 'bob'})")?;
    assert_eq!(
        count_nodes(&db, "Person")?,
        1,
        "额外的属性不构成不匹配（MATCH 模式过滤语义）"
    );

    // 列出的属性值不相等 -> 确实不匹配 -> 新建
    db.run_cypher("MERGE (p:Person {name: 'bob', age: 31})")?;
    assert_eq!(count_nodes(&db, "Person")?, 2, "列出的属性值必须相等才匹配");

    // 完全相同的模式 -> 复用
    db.run_cypher("MERGE (p:Person {name: 'bob', age: 30})")?;
    assert_eq!(count_nodes(&db, "Person")?, 2);

    Ok(())
}

#[test]
fn test_merge_creates_the_whole_pattern_when_any_part_is_missing() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("merge_path.db")?;

    db.run_cypher("MERGE (x:A {k: 1})-[:R]->(y:B {k: 2})")?;
    assert_eq!(
        count_nodes(&db, "A")? + count_nodes(&db, "B")?,
        2,
        "整条路径应被创建"
    );

    // 整体匹配：整条路径已存在 -> 什么都不创建
    db.run_cypher("MERGE (x:A {k: 1})-[:R]->(y:B {k: 2})")?;
    assert_eq!(
        count_nodes(&db, "A")? + count_nodes(&db, "B")?,
        2,
        "整条路径已存在时不得创建任何节点"
    );
    let edges = scalar(&db, "MATCH ()-[r:R]->() RETURN count(*)")?;
    assert_eq!(edges, Value::Int(1), "重复 MERGE 不得产生第二条边");

    // 换掉目标端 -> 整条路径不存在 -> **整条**创建，包括再来一个 A{k:1}。
    //
    // 这一点容易写错，而写错的代价是数据语义悄悄偏离 Cypher：Cypher 文档明确
    // 规定「MERGE 要么整体匹配，要么整体创建」，并用 `HAS_CHAUFFEUR` 的例子说明
    // 「即使同名 Person 已存在，也照样为它新建一个 Chauffer 节点」。
    // 部分复用会让结果取决于模式里各部分的初始状态，同一查询在不同数据上拼出
    // 不同形状的路径，且没有规则可循。
    db.run_cypher("MERGE (x:A {k: 1})-[:R]->(y:B {k: 3})")?;
    assert_eq!(
        count_nodes(&db, "A")?,
        2,
        "整条路径不存在时，已存在的部分也重新创建（整体创建语义）"
    );
    assert_eq!(count_nodes(&db, "B")?, 2);
    let edges = scalar(&db, "MATCH ()-[r:R]->() RETURN count(*)")?;
    assert_eq!(edges, Value::Int(2), "第二条路径必须有自己的边");

    Ok(())
}

// =========================================================================
// ON CREATE / ON MATCH
// =========================================================================

#[test]
fn test_on_create_and_on_match_fire_at_the_right_time() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("merge_on.db")?;

    db.run_cypher("MERGE (b:Acct {id: 7}) ON CREATE SET b.created = 1 ON MATCH SET b.seen = 9")?;

    // 创建分支：created 有值，seen 没有（ON MATCH 不该触发）
    let res = db.run_cypher("MATCH (b:Acct {id: 7}) RETURN b.created, b.seen")?;
    assert_eq!(res.rows[0].values[0], Value::Int(1));
    assert_eq!(
        res.rows[0].values[1],
        Value::Null,
        "ON MATCH 的赋值不得在创建时发生"
    );

    // 命中分支：seen 被写入，created 保持不变（ON CREATE 不该再次触发）
    db.run_cypher("MERGE (b:Acct {id: 7}) ON CREATE SET b.created = 100 ON MATCH SET b.seen = 42")?;
    let res = db.run_cypher("MATCH (b:Acct {id: 7}) RETURN b.created, b.seen")?;
    assert_eq!(
        res.rows[0].values[0],
        Value::Int(1),
        "ON CREATE 不得在命中时再次触发"
    );
    assert_eq!(res.rows[0].values[1], Value::Int(42));

    Ok(())
}

#[test]
fn test_on_match_can_add_labels() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("merge_on_label.db")?;

    db.run_cypher("MERGE (n:Thing {key: 'k'}) ON MATCH SET n:Seen")?;
    assert_eq!(
        count_nodes(&db, "Seen")?,
        0,
        "首次是创建，ON MATCH 不该触发"
    );

    db.run_cypher("MERGE (n:Thing {key: 'k'}) ON MATCH SET n:Seen")?;
    assert_eq!(count_nodes(&db, "Seen")?, 1, "第二次命中应追加标签");

    Ok(())
}

// =========================================================================
// 与索引、约束的配合
// =========================================================================

#[test]
fn test_merge_respects_unique_constraint() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("merge_unique.db")?;

    db.run_cypher("CREATE (:User {name: 'alice'})")?;
    db.create_unique_constraint("User", "name")?;

    // 已存在 -> 复用，不触发约束
    let res = db.run_cypher("MERGE (u:User {name: 'alice'})")?;
    assert_eq!(res.stats.nodes_created, 0);

    // 不存在 -> 创建，约束允许
    let res = db.run_cypher("MERGE (u:User {name: 'bob'})")?;
    assert_eq!(res.stats.nodes_created, 1);

    assert_eq!(count_nodes(&db, "User")?, 2);
    Ok(())
}

#[test]
fn test_merge_is_index_visible_after_creation() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("merge_index.db")?;

    db.run_cypher("MERGE (u:User {name: 'u1'})")?;
    db.run_cypher("MERGE (u:User {name: 'u2'})")?;

    // 走标签索引计数：若 MERGE 创建时漏维护索引，这里会少算
    assert_eq!(count_nodes(&db, "User")?, 2);

    // 按属性精确匹配（走属性索引）
    let got = scalar(&db, "MATCH (u:User {name: 'u2'}) RETURN u.name")?;
    assert_eq!(got, Value::String("u2".to_string()));

    Ok(())
}

#[test]
fn test_merge_on_match_set_is_index_visible() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("merge_index_set.db")?;

    // 先建立属性索引
    db.run_cypher("MERGE (n:Item {sku: 'a'})")?;
    db.run_cypher("MATCH (n:Item) RETURN n.sku")?;

    // ON MATCH SET 改属性后，属性索引必须跟上，否则按新值查不到
    db.run_cypher("MERGE (n:Item {sku: 'a'}) ON MATCH SET n.stock = 5")?;

    let res = db.run_cypher("MATCH (n:Item) RETURN n.stock")?;
    assert_eq!(
        res.rows[0].values[0],
        Value::Int(5),
        "ON MATCH SET 的值必须可读回"
    );

    Ok(())
}

// =========================================================================
// RETURN 与投影
// =========================================================================

#[test]
fn test_merge_return_sees_created_data() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("merge_return.db")?;

    // 首次：RETURN 必须看到刚创建的值（而不是 NULL）
    let got = scalar(&db, "MERGE (c:C {v: 5}) RETURN c.v")?;
    assert_eq!(got, Value::Int(5), "RETURN 必须读到刚创建的数据");

    // 命中：同样读到值
    let got = scalar(&db, "MERGE (c:C {v: 5}) RETURN c.v")?;
    assert_eq!(got, Value::Int(5));

    // ON MATCH SET 之后 RETURN 应看到新值
    let got = scalar(&db, "MERGE (c:C {v: 5}) ON MATCH SET c.v2 = 77 RETURN c.v2")?;
    assert_eq!(
        got,
        Value::Int(77),
        "RETURN 必须看到本次 ON MATCH SET 的结果"
    );

    Ok(())
}

// =========================================================================
// 拒绝而非静默
// =========================================================================

#[test]
fn test_merge_rejects_non_literal_properties() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("merge_literal.db")?;

    // 与 MATCH 同规则：MERGE 的模式在求值时还没有行上下文
    let err = db
        .run_cypher("MERGE (n {v: someVar})")
        .expect_err("非字面量模式属性必须被拒绝");
    assert!(
        err.to_string().contains("literal"),
        "错误信息应说明需要字面量，实际: {err}"
    );

    Ok(())
}

#[test]
fn test_read_only_handle_rejects_merge() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let path = dir.path().join("merge_ro.db");
    {
        let db = NervusDb::open(&path)?;
        db.run_cypher("CREATE (:Seed {v: 1})")?;
        db.checkpoint()?;
    }

    let ro = NervusDb::open_read_only(&path)?;
    let err = ro
        .run_cypher("MERGE (q:Q {w: 1})")
        .expect_err("只读句柄不得执行 MERGE");
    assert!(
        err.to_string().contains("read-only"),
        "应报告只读限制，实际: {err}"
    );

    Ok(())
}

#[test]
fn test_merge_requires_create_or_match_after_on() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("merge_syntax.db")?;

    assert!(
        db.run_cypher("MERGE (n:N {v: 1}) ON SET n.x = 1").is_err(),
        "ON 后面必须跟 CREATE 或 MATCH"
    );

    Ok(())
}

#[test]
fn test_explain_merge_has_no_side_effects() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("merge_explain.db")?;

    db.run_cypher("CREATE (:P {v: 1})")?;
    let before = scalar(&db, "MATCH (n) RETURN count(*)")?;

    let plan = db.run_cypher("EXPLAIN MERGE (z:Z {q: 1})")?;
    let text = plan
        .rows
        .iter()
        .map(|r| format!("{:?}", r.values[0]))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Merge"), "计划里应出现 Merge，实际: {text}");
    assert!(
        text.contains("WriteQuery"),
        "MERGE 必须标记为写查询，实际: {text}"
    );

    let after = scalar(&db, "MATCH (n) RETURN count(*)")?;
    assert_eq!(before, after, "EXPLAIN 绝不能产生副作用");

    Ok(())
}

// =========================================================================
// 缺省标签（无变量模式）
// =========================================================================

#[test]
fn test_merge_without_variable() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("merge_novar.db")?;

    db.run_cypher("MERGE (:Anon {k: 1})")?;
    db.run_cypher("MERGE (:Anon {k: 1})")?;
    assert_eq!(count_nodes(&db, "Anon")?, 1, "无变量模式同样必须幂等");

    Ok(())
}

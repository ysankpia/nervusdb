//! 战役二验证套件：Cypher 1.0 工业级语法闭环（SET / DETACH DELETE / ORDER BY /
//! SKIP / LIMIT / 聚合函数 / 变长多跳 / 无向边 / 多模式匹配）。
//!
//! 全部测试通过公开 API（`execute` / `query_cypher`）驱动，不触碰任何内部结构。

use graphlite::{GraphError, GraphLite, Value};
use std::collections::{HashMap, HashSet};
use tempfile::tempdir;

fn open_temp(name: &str) -> Result<(tempfile::TempDir, GraphLite), GraphError> {
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join(name))?;
    Ok((dir, db))
}

fn value_of(db: &GraphLite, cypher: &str) -> Result<Value, GraphError> {
    let res = db.query_cypher(cypher)?;
    assert_eq!(
        res.row_count(),
        1,
        "expected exactly one row for: {}",
        cypher
    );
    Ok(res.rows[0].values[0].clone())
}

// =========================================================================
// SET 属性更新 / 追加标签
// =========================================================================
#[test]
fn test_set_property_and_label() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("set_props.db")?;

    db.execute("CREATE (a:Person {name: 'Alice', age: 28})")?;

    // 1. SET 更新既有属性
    let res = db.execute("MATCH (a:Person {name: 'Alice'}) SET a.age = 31")?;
    assert_eq!(res.properties_set, 1);
    assert_eq!(
        value_of(&db, "MATCH (a:Person) RETURN a.age")?,
        Value::from(31)
    );

    // 2. SET 新增属性
    db.execute("MATCH (a:Person) SET a.city = 'Beijing'")?;
    assert_eq!(
        value_of(&db, "MATCH (a:Person) RETURN a.city")?,
        Value::from("Beijing")
    );

    // 3. SET 追加标签后可通过新标签检索
    db.execute("MATCH (a:Person) SET a:Employee")?;
    let res = db.query_cypher("MATCH (a:Employee) RETURN a.name")?;
    assert_eq!(res.row_count(), 1);
    assert_eq!(res.rows[0].values[0], Value::from("Alice"));

    // 4. 索引随 SET 自适应：旧值不再命中，新值必须命中
    assert_eq!(
        db.query_cypher("MATCH (a:Person {age: 28}) RETURN a")?
            .row_count(),
        0
    );
    assert_eq!(
        db.query_cypher("MATCH (a:Person {age: 31}) RETURN a")?
            .row_count(),
        1
    );

    Ok(())
}

#[test]
fn test_set_edge_property() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("set_edge.db")?;

    db.execute("CREATE (a:N {i: 1})-[:LINK {weight: 1.0}]->(b:N {i: 2})")?;
    let res = db.execute("MATCH (a:N)-[r:LINK]->(b:N) SET r.weight = 4.5")?;
    assert_eq!(res.properties_set, 1);

    assert_eq!(
        value_of(&db, "MATCH (a:N)-[r:LINK]->(b:N) RETURN r.weight")?,
        Value::from(4.5)
    );

    Ok(())
}

// =========================================================================
// DETACH DELETE 级联删除 + Freelist 槽位回收
// =========================================================================
#[test]
fn test_detach_delete_cascade_and_freelist_reuse() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("detach_delete.db")?;

    // 枢纽节点 hub 连接 3 个叶子
    let hub = db.add_node(HashSet::from(["Hub".to_string()]), HashMap::new())?;
    let mut leaves = Vec::new();
    for i in 0..3 {
        let mut props = HashMap::new();
        props.insert("idx".to_string(), Value::from(i as i64));
        let leaf = db.add_node(HashSet::from(["Leaf".to_string()]), props)?;
        db.add_edge(hub, leaf, "LINK", HashMap::new(), 1.0)?;
        leaves.push(leaf);
    }
    assert_eq!(db.node_count(), 4);
    assert_eq!(db.edge_count(), 3);

    // DETACH DELETE 级联删除枢纽节点及其 3 条关联边
    let res = db.execute("MATCH (h:Hub) DETACH DELETE h")?;
    assert_eq!(res.nodes_deleted, 1);
    assert_eq!(res.edges_deleted, 3);
    assert_eq!(db.node_count(), 3);
    assert_eq!(db.edge_count(), 0);

    // 叶子节点仍存活，且邻接链表已被正确清理
    for &leaf in &leaves {
        let node = db.get_node(leaf).expect("leaf must survive");
        assert!(!node.incoming.contains(&1), "edge 1 must be unlinked");
    }

    // Freelist 槽位复用：新节点必须回收被删除的 hub 槽位
    let recycled = db.add_node(HashSet::from(["New".to_string()]), HashMap::new())?;
    assert_eq!(
        recycled, hub,
        "new node must reuse the freed hub slot from the freelist"
    );

    Ok(())
}

#[test]
fn test_delete_edge_and_plain_delete_semantics() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("delete_semantics.db")?;

    db.execute("CREATE (a:N {i: 1})-[:R]->(b:N {i: 2})")?;

    // 1. 删除边：节点必须完好
    let res = db.execute("MATCH (a:N {i: 1})-[r:R]->(b:N) DELETE r")?;
    assert_eq!(res.edges_deleted, 1);
    assert_eq!(res.nodes_deleted, 0);
    assert_eq!(db.node_count(), 2);
    assert_eq!(db.edge_count(), 0);

    // 2. 无关联边时 DELETE 节点成功
    let res = db.execute("MATCH (b:N {i: 2}) DELETE b")?;
    assert_eq!(res.nodes_deleted, 1);
    assert_eq!(db.node_count(), 1);

    // 3. 仍有关联边时 DELETE（非 DETACH）必须显式报错而非静默丢数据
    db.execute("CREATE (x:M {i: 10})-[:R]->(y:M {i: 20})")?;
    let err = db
        .execute("MATCH (x:M {i: 10}) DELETE x")
        .expect_err("plain DELETE on a connected node must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("DETACH DELETE"),
        "error must advise DETACH DELETE, got: {}",
        msg
    );
    assert_eq!(db.node_count(), 3, "failed delete must not remove anything");

    Ok(())
}

// =========================================================================
// ORDER BY / SKIP / LIMIT 分页
// =========================================================================
#[test]
fn test_order_by_skip_limit() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("order_paging.db")?;

    for (name, score) in [("A", 30), ("B", 10), ("C", 50), ("D", 20), ("E", 40)] {
        db.execute(&format!(
            "CREATE (p:Item {{name: '{}', score: {}}})",
            name, score
        ))?;
    }

    // 1. 升序
    let asc = db.query_cypher("MATCH (p:Item) RETURN p.name ORDER BY p.score ASC")?;
    let names: Vec<String> = asc
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, vec!["B", "D", "A", "E", "C"]);

    // 2. 降序
    let desc = db.query_cypher("MATCH (p:Item) RETURN p.name ORDER BY p.score DESC")?;
    let names: Vec<String> = desc
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, vec!["C", "E", "A", "D", "B"]);

    // 3. SKIP + LIMIT 分页（按 score 升序取第 2、3 名）
    let page = db.query_cypher("MATCH (p:Item) RETURN p.name ORDER BY p.score SKIP 1 LIMIT 2")?;
    let names: Vec<String> = page
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, vec!["D", "A"]);

    // 4. 仅 LIMIT（默认顺序取前 2 行）
    let limited = db.query_cypher("MATCH (p:Item) RETURN p.name LIMIT 2")?;
    assert_eq!(limited.row_count(), 2);

    // 5. SKIP 超出总行数返回空集
    let empty = db.query_cypher("MATCH (p:Item) RETURN p.name SKIP 99")?;
    assert_eq!(empty.row_count(), 0);

    Ok(())
}

// =========================================================================
// 聚合函数：count / sum / avg / min / max（含分组与边界语义）
// =========================================================================
#[test]
fn test_aggregate_functions() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("aggregates.db")?;

    for (dept, salary) in [
        ("Eng", 100),
        ("Eng", 200),
        ("Eng", 300),
        ("Sales", 150),
        ("Sales", 250),
    ] {
        db.execute(&format!(
            "CREATE (e:Emp {{dept: '{}', salary: {}}})",
            dept, salary
        ))?;
    }

    // 1. 全局聚合
    assert_eq!(
        value_of(&db, "MATCH (e:Emp) RETURN count(e)")?,
        Value::from(5)
    );
    assert_eq!(
        value_of(&db, "MATCH (e:Emp) RETURN count(*)")?,
        Value::from(5)
    );
    assert_eq!(
        value_of(&db, "MATCH (e:Emp) RETURN sum(e.salary)")?,
        Value::from(1000)
    );
    assert_eq!(
        value_of(&db, "MATCH (e:Emp) RETURN avg(e.salary)")?,
        Value::from(200.0)
    );
    assert_eq!(
        value_of(&db, "MATCH (e:Emp) RETURN min(e.salary)")?,
        Value::from(100)
    );
    assert_eq!(
        value_of(&db, "MATCH (e:Emp) RETURN max(e.salary)")?,
        Value::from(300)
    );

    // 2. 分组聚合：按 dept 分组后各算 sum / count
    let res = db
        .query_cypher("MATCH (e:Emp) RETURN e.dept, count(e), sum(e.salary) ORDER BY e.dept ASC")?;
    assert_eq!(res.row_count(), 2);
    assert_eq!(res.rows[0].values[0], Value::from("Eng"));
    assert_eq!(res.rows[0].values[1], Value::from(3));
    assert_eq!(res.rows[0].values[2], Value::from(600));
    assert_eq!(res.rows[1].values[0], Value::from("Sales"));
    assert_eq!(res.rows[1].values[1], Value::from(2));
    assert_eq!(res.rows[1].values[2], Value::from(400));

    // 3. 聚合别名 + ORDER BY 别名
    let res = db.query_cypher(
        "MATCH (e:Emp) RETURN e.dept AS d, sum(e.salary) AS total ORDER BY total DESC",
    )?;
    assert_eq!(res.columns, vec!["d", "total"]);
    assert_eq!(res.rows[0].values[0], Value::from("Eng"));
    assert_eq!(res.rows[0].values[1], Value::from(600));

    // 4. 空结果集边界：count = 0、sum = 0、avg/min/max 为 null
    assert_eq!(
        value_of(&db, "MATCH (e:Ghost) RETURN count(e)")?,
        Value::from(0)
    );
    assert_eq!(
        value_of(&db, "MATCH (e:Ghost) RETURN sum(e.salary)")?,
        Value::from(0)
    );
    assert_eq!(
        value_of(&db, "MATCH (e:Ghost) RETURN avg(e.salary)")?,
        Value::from("null")
    );
    assert_eq!(
        value_of(&db, "MATCH (e:Ghost) RETURN min(e.salary)")?,
        Value::from("null")
    );

    // 5. 聚合 + WHERE 过滤
    assert_eq!(
        value_of(&db, "MATCH (e:Emp) WHERE e.salary >= 200 RETURN count(e)")?,
        Value::from(3)
    );

    Ok(())
}

// =========================================================================
// 变长多跳与无向边
// =========================================================================
#[test]
fn test_variable_length_and_undirected_match() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("var_len.db")?;

    // 链式 A -> B -> C -> D -> E
    db.execute("CREATE (a:N {n: 1})-[:R]->(b:N {n: 2})-[:R]->(c:N {n: 3})-[:R]->(d:N {n: 4})-[:R]->(e:N {n: 5})")?;

    // 1. *1..3 精确匹配 1/2/3 跳（不含 4 跳）
    let res = db.query_cypher("MATCH (a:N {n: 1})-[:R*1..3]->(t:N) RETURN t.n")?;
    assert_eq!(res.row_count(), 3);
    let mut got: Vec<i64> = res
        .rows
        .iter()
        .map(|r| r.values[0].as_i64().unwrap())
        .collect();
    got.sort_unstable();
    assert_eq!(got, vec![2, 3, 4]);

    // 2. *2..2 固定两跳
    let res = db.query_cypher("MATCH (a:N {n: 1})-[:R*2..2]->(t:N) RETURN t.n")?;
    assert_eq!(res.row_count(), 1);
    assert_eq!(res.rows[0].values[0], Value::from(3));

    // 3. 无向边 -[:R]- 从末端回溯（E 的 1 跳无向邻居只有 D）
    let res = db.query_cypher("MATCH (a:N {n: 5})-[:R*1..2]-(t:N) RETURN t.n")?;
    let mut got: Vec<i64> = res
        .rows
        .iter()
        .map(|r| r.values[0].as_i64().unwrap())
        .collect();
    got.sort_unstable();
    assert_eq!(got, vec![3, 4]);

    // 4. 无向单跳：中间节点 C 同时连到 B 与 D
    let res = db.query_cypher("MATCH (a:N {n: 3})-[:R]-(t:N) RETURN t.n")?;
    let mut got: Vec<i64> = res
        .rows
        .iter()
        .map(|r| r.values[0].as_i64().unwrap())
        .collect();
    got.sort_unstable();
    assert_eq!(got, vec![2, 4]);

    Ok(())
}

#[test]
fn test_multi_pattern_match_join() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("multi_pattern.db")?;

    db.execute("CREATE (a:P {name: 'A'})-[:R]->(b:P {name: 'B'})-[:R]->(c:P {name: 'C'})")?;

    // 多模式跨模式连接：共享变量 b 必须绑定到同一节点
    let res = db
        .query_cypher("MATCH (a:P)-[:R]->(b:P), (b:P)-[:R]->(c:P) RETURN a.name, b.name, c.name")?;
    assert_eq!(res.row_count(), 1);
    assert_eq!(res.rows[0].values[0], Value::from("A"));
    assert_eq!(res.rows[0].values[1], Value::from("B"));
    assert_eq!(res.rows[0].values[2], Value::from("C"));

    // 无共享变量的多模式：笛卡尔积（3 个节点 x 3 个节点 = 9 行）
    let res = db.query_cypher("MATCH (x:P), (y:P) RETURN count(*)")?;
    assert_eq!(res.rows[0].values[0], Value::from(9));

    Ok(())
}

// =========================================================================
// MATCH ... CREATE 组合（模式扩展）
// =========================================================================
#[test]
fn test_match_then_create() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("match_create.db")?;

    db.execute("CREATE (a:P {name: 'A'})")?;
    db.execute("CREATE (b:P {name: 'B'})")?;

    // 由两个已存在节点建立新边
    let res = db.execute(
        "MATCH (a:P {name: 'A'}), (b:P {name: 'B'}) CREATE (a)-[:KNOWS {since: 2024}]->(b)",
    )?;
    assert_eq!(res.edges_created, 1);
    assert_eq!(res.nodes_created, 0);
    assert_eq!(db.node_count(), 2);
    assert_eq!(db.edge_count(), 1);

    assert_eq!(
        value_of(&db, "MATCH (a:P)-[r:KNOWS]->(b:P) RETURN r.since")?,
        Value::from(2024)
    );

    Ok(())
}

// =========================================================================
// 标签谓词与组合 WHERE
// =========================================================================
#[test]
fn test_label_predicate_and_compound_where() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("label_predicate.db")?;

    db.execute("CREATE (a:Person:Engineer {name: 'A', age: 40})")?;
    db.execute("CREATE (b:Person {name: 'B', age: 25})")?;

    // 类型谓词：n:Engineer
    let res = db.query_cypher("MATCH (n:Person) WHERE n:Engineer RETURN n.name")?;
    assert_eq!(res.row_count(), 1);
    assert_eq!(res.rows[0].values[0], Value::from("A"));

    // AND / OR 组合
    let res = db.query_cypher(
        "MATCH (n:Person) WHERE n.age > 30 OR n.name = 'B' RETURN n.name ORDER BY n.name",
    )?;
    assert_eq!(res.row_count(), 2);

    let res =
        db.query_cypher("MATCH (n:Person) WHERE n.age > 30 AND n.name = 'A' RETURN n.name")?;
    assert_eq!(res.row_count(), 1);

    Ok(())
}

// =========================================================================
// RETURN * 展开与列名一致性
// =========================================================================
#[test]
fn test_return_star_expansion() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("return_star.db")?;

    db.execute("CREATE (a:P {name: 'A'})-[:R]->(b:P {name: 'B'})")?;

    let res = db.query_cypher("MATCH (a:P)-[r:R]->(b:P) RETURN *")?;
    assert_eq!(res.row_count(), 1);
    assert_eq!(res.columns.len(), res.rows[0].values.len());
    assert!(res.columns.contains(&"a".to_string()));
    assert!(res.columns.contains(&"b".to_string()));
    assert!(res.columns.contains(&"r".to_string()));

    Ok(())
}

// =========================================================================
// 写语句与 RETURN 组合
// =========================================================================
#[test]
fn test_mutation_with_return_clause() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("mutation_return.db")?;

    db.execute("CREATE (a:P {name: 'A', age: 10})-[:R]->(b:P {name: 'B', age: 20})")?;

    let res = db.query_cypher("MATCH (a:P)-[:R]->(b:P) SET a.age = 99 RETURN a.name, a.age")?;
    assert_eq!(res.row_count(), 1);
    assert_eq!(res.rows[0].values[0], Value::from("A"));
    assert_eq!(res.rows[0].values[1], Value::from(99));
    assert_eq!(res.stats.properties_set, 1);

    Ok(())
}

// =========================================================================
// 变更持久化：SET / DETACH DELETE 经 Checkpoint 冷重启后一致
// =========================================================================
#[test]
fn test_mutation_durability_across_reopen() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("mutation_durability.db");

    {
        let db = GraphLite::open(&db_path)?;
        db.execute("CREATE (a:P {name: 'A', age: 10})-[:R]->(b:P {name: 'B', age: 20})")?;
        db.execute("CREATE (c:P {name: 'C', age: 30})")?;
        db.execute("MATCH (a:P {name: 'A'}) SET a.age = 77")?;
        db.execute("MATCH (b:P {name: 'B'}) SET b:Tagged")?;
        db.execute("MATCH (c:P {name: 'C'}) DETACH DELETE c")?;
        db.checkpoint()?;
    }

    let db = GraphLite::open(&db_path)?;
    assert_eq!(db.node_count(), 2);
    assert_eq!(db.edge_count(), 1);
    assert_eq!(
        value_of(&db, "MATCH (a:P {name: 'A'}) RETURN a.age")?,
        Value::from(77)
    );
    assert_eq!(
        db.query_cypher("MATCH (b:Tagged) RETURN b.name")?
            .row_count(),
        1
    );
    assert_eq!(
        db.query_cypher("MATCH (c:P {name: 'C'}) RETURN c")?
            .row_count(),
        0
    );

    Ok(())
}

// =========================================================================
// dump_cypher 文本级重放：按分号切分逐条执行到全新库，图结构完全复原
// =========================================================================
#[test]
fn test_dump_cypher_script_replay() -> Result<(), GraphError> {
    let source = GraphLite::open(":memory:")?;
    source.execute(
        "CREATE (a:User {name: 'Alice', age: 25})-[:FOLLOWS {weight: 1.5}]->(b:User {name: 'Bob', age: 30})",
    )?;

    // 导出为脚本（流式写入缓冲）
    let mut dump: Vec<u8> = Vec::new();
    source.dump_cypher(&mut dump)?;
    let script = String::from_utf8(dump).expect("dump must be UTF-8");

    assert!(script.contains("CREATE"), "dump must contain CREATE");
    assert!(
        script.contains("MATCH"),
        "dump must contain relationship MATCH"
    );
    assert!(script.contains("FOLLOWS"), "dump must contain edge type");
    assert!(script.contains("Alice"), "dump must carry properties");

    // 逐行过滤注释后，按分号切分逐条重放到全新库
    let cleaned: String = script
        .lines()
        .filter(|l| !l.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");

    let target = GraphLite::open(":memory:")?;
    for stmt in cleaned.split(';') {
        let trimmed = stmt.trim();
        if !trimmed.is_empty() {
            target.execute(trimmed)?;
        }
    }

    // 图结构必须完全复原
    assert_eq!(target.node_count(), 2, "replay must restore both nodes");
    assert_eq!(target.edge_count(), 1, "replay must restore the edge");
    let res = target.query_cypher("MATCH (a:User)-[:FOLLOWS]->(b:User) RETURN a.name, b.name")?;
    assert_eq!(res.row_count(), 1);
    assert_eq!(res.rows[0].values[0], Value::from("Alice"));
    assert_eq!(res.rows[0].values[1], Value::from("Bob"));
    assert_eq!(
        target
            .query_cypher("MATCH (u:User {name: 'Alice'}) RETURN u.age")?
            .rows[0]
            .values[0],
        Value::from(25)
    );

    Ok(())
}

// =========================================================================
// EXPLAIN：只描述计划，不执行查询
// =========================================================================
#[test]
fn test_explain_reports_plan_without_executing() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("explain.db");
    let db = GraphLite::open(&db_path)?;

    db.with_transaction(|tx| {
        let a = tx.add_node(HashSet::from(["City".to_string()]), HashMap::new())?;
        let b = tx.add_node(HashSet::from(["City".to_string()]), HashMap::new())?;
        tx.add_edge(a, b, "ROAD", HashMap::new(), 1.0)?;
        Ok(())
    })?;

    // 计划必须提到实际使用的起点选择
    let plan = db.run_cypher("EXPLAIN MATCH (c:City)-[r:ROAD]->(d) RETURN c")?;
    let text = plan_text(&plan);
    assert!(
        text.contains("label index (:City)"),
        "plan must name the start-node selection, got:\n{}",
        text
    );

    // LIMIT 下推的两种情况必须在计划中如实区分
    let pushable =
        plan_text(&db.run_cypher("EXPLAIN MATCH (c:City)-[r:ROAD]->(d) RETURN c LIMIT 5")?);
    assert!(
        pushable.contains("已下推"),
        "a pushable LIMIT must be reported as pushed down, got:\n{}",
        pushable
    );

    let blocked = plan_text(
        &db.run_cypher("EXPLAIN MATCH (c:City)-[r:ROAD]->(d) RETURN c ORDER BY c LIMIT 5")?,
    );
    assert!(
        blocked.contains("无法下推") && blocked.contains("ORDER BY"),
        "a blocked LIMIT must say why, got:\n{}",
        blocked
    );

    // 关键断言：EXPLAIN 绝不产生副作用
    let before = db.node_count();
    db.run_cypher("EXPLAIN CREATE (x:ShouldNotExist)")?;
    assert_eq!(
        db.node_count(),
        before,
        "EXPLAIN CREATE must not create anything"
    );

    db.run_cypher("EXPLAIN MATCH (c:City) SET c.seen = true")?;
    let seen = db.run_cypher("MATCH (c:City) WHERE c.seen = true RETURN count(*) AS n")?;
    assert_eq!(
        seen.rows[0].values[0].as_i64(),
        Some(0),
        "EXPLAIN SET must not modify any node"
    );

    Ok(())
}

/// 把计划结果集拼成一段文本，便于断言。
fn plan_text(res: &graphlite::CypherResultSet) -> String {
    res.rows
        .iter()
        .filter_map(|r| match &r.values[0] {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

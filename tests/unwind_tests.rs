//! 1.1.0 套件：`UNWIND` 与模式属性表达式。
//!
//! `UNWIND <list> AS x` 把列表展开为行，是 Cypher 里把**批量数据**表达进查询的
//! 唯一手段：没有它，插入 1000 行需要 1000 条语句或一次 SDK 调用；有了它，
//! `UNWIND [...] AS x CREATE (n {v: x})` 就是一条语句。
//!
//! 这个套件覆盖的**每一个**行为都对应一个曾经可能静默出错的点，尤其是：
//! - 非列表输入的展开语义（产生 1 行，不是报错、也不是 0 行）
//! - 新建节点的变量必须回填进返回行（否则 `RETURN n.x` 是 NULL）
//! - `MATCH (n {k: <非字面量>})` 必须报错而不是静默不匹配
//! - 唯一约束在 `UNWIND ... CREATE` 路径上同样生效
//!
//! 测试全部走公开 API。

use graphlite::{GraphError, GraphLite, Value};
use tempfile::tempdir;

fn open_temp(name: &str) -> Result<(tempfile::TempDir, GraphLite), GraphError> {
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join(name))?;
    Ok((dir, db))
}

/// 取出单列结果的所有值。
///
/// 走 `run_cypher`（按 AST 自动路由读写），因为它返回结果集；
/// `GraphLite::execute` 只返回统计摘要。
fn column(db: &GraphLite, cypher: &str) -> Result<Vec<Value>, GraphError> {
    let res = db.run_cypher(cypher)?;
    Ok(res.rows.iter().map(|r| r.values[0].clone()).collect())
}

// =========================================================================
// 展开语义
// =========================================================================

#[test]
fn test_unwind_expands_list_into_rows() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("unwind_basic.db")?;

    let res = db.run_cypher("UNWIND [1, 2, 3] AS x RETURN x")?;
    assert_eq!(res.row_count(), 3, "三个元素应产生三行");
    assert_eq!(res.columns, vec!["x".to_string()]);
    let got: Vec<Value> = res.rows.iter().map(|r| r.values[0].clone()).collect();
    assert_eq!(
        got,
        vec![Value::Int(1), Value::Int(2), Value::Int(3)],
        "行顺序必须与列表顺序一致"
    );

    Ok(())
}

#[test]
fn test_unwind_empty_list_yields_zero_rows() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("unwind_empty.db")?;

    let res = db.run_cypher("UNWIND [] AS x RETURN x")?;
    assert_eq!(res.row_count(), 0, "空列表应产生 0 行");
    // 列名仍然必须存在：零行的结果集不等于零列
    assert_eq!(res.columns, vec!["x".to_string()]);

    Ok(())
}

#[test]
fn test_unwind_scalar_yields_single_row() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("unwind_scalar.db")?;

    // 非列表值当作单元素列表：Cypher 的 `UNWIND 42 AS x` 产生一行。
    // 若实现成「报错」或「0 行」，两种都会让批量脚本在最外层循环上失配。
    let res = db.run_cypher("UNWIND 42 AS x RETURN x")?;
    assert_eq!(res.row_count(), 1);
    assert_eq!(res.rows[0].values[0], Value::Int(42));

    Ok(())
}

#[test]
fn test_unwind_preserves_element_types() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("unwind_types.db")?;

    // 字符串元素必须以字符串返回，不能被统一成数字或 JSON 字符串
    let got = column(&db, "UNWIND ['a', 'b'] AS s RETURN s")?;
    assert_eq!(
        got,
        vec![
            Value::String("a".to_string()),
            Value::String("b".to_string())
        ]
    );

    // 混合类型列表同样合法
    let got = column(&db, "UNWIND [1, 2.5, 'x'] AS v RETURN v")?;
    assert_eq!(got.len(), 3);
    assert_eq!(got[0], Value::Int(1));
    assert_eq!(got[1], Value::Float(2.5));
    assert_eq!(got[2], Value::String("x".to_string()));

    Ok(())
}

// =========================================================================
// 批量写入：UNWIND 的核心用途
// =========================================================================

#[test]
fn test_unwind_create_inserts_one_node_per_element() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("unwind_create.db")?;

    let res = db.run_cypher("UNWIND [10, 20, 30] AS v CREATE (n:Num {v: v})")?;
    assert_eq!(res.stats.nodes_created, 3, "每个元素应创建一个节点");

    // 独立验证：数据确实落盘了，而不是只报了数字
    let got = column(&db, "MATCH (n:Num) RETURN n.v ORDER BY n.v")?;
    assert_eq!(
        got,
        vec![Value::Int(10), Value::Int(20), Value::Int(30)],
        "属性值必须来自 UNWIND 变量，而不是固定值"
    );

    Ok(())
}

#[test]
fn test_unwind_create_returns_created_variables() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("unwind_create_return.db")?;

    // `RETURN m.s` 必须拿到刚写入的值。
    //
    // 这个断言针对一个具体缺陷：`CREATE` 子句创建节点后若不把变量绑定回行上下文，
    // `RETURN m.s` 会渲染成 NULL —— 一个刚刚成功写入 `s = 'a'` 的节点却报告
    // 「没有这个属性」。这属于静默的错误答案，比报错更危险。
    let res = db.run_cypher("UNWIND ['a', 'b'] AS s CREATE (m:Str {s: s}) RETURN m.s AS s")?;
    assert_eq!(res.stats.nodes_created, 2);
    let got: Vec<Value> = res.rows.iter().map(|r| r.values[0].clone()).collect();
    assert_eq!(
        got,
        vec![
            Value::String("a".to_string()),
            Value::String("b".to_string())
        ],
        "新建节点的属性必须能从 RETURN 中读回"
    );

    Ok(())
}

#[test]
fn test_unwind_create_builds_edges_between_created_nodes() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("unwind_edges.db")?;

    // 一个元素创建一个「节点 + 一条边」：这是批量建图的基本形状
    db.run_cypher("CREATE (center:Hub {name: 'h'})")?;
    let res = db
        .execute("MATCH (h:Hub) UNWIND [1, 2, 3] AS i CREATE (h)-[r:LINK {i: i}]->(n:Leaf {i: i})");
    // MATCH + UNWIND 的组合不在本版本的语法面内：确认它被**明确拒绝**，
    // 而不是被静默当成另一种语义执行
    match res {
        Ok(_) => panic!("MATCH 与 UNWIND 的组合未实现，不应静默接受"),
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("UNWIND") || msg.contains("trailing") || msg.contains("understood"),
                "拒绝原因应指向语法，实际: {msg}"
            );
        }
    }

    // 纯 UNWIND 建边是支持的
    let res = db.run_cypher("UNWIND [7, 8] AS i CREATE (a:L {i: i})-[r:NEXT {j: i}]->(b:M)")?;
    assert_eq!(res.stats.nodes_created, 4, "每个元素 2 个节点");
    assert_eq!(res.stats.edges_created, 2, "每个元素 1 条边");

    // 边属性来自 UNWIND 变量：每个元素一条边，属性各不相同
    let got = column(&db, "MATCH (:L)-[r:NEXT]->(:M) RETURN r.j ORDER BY r.j")?;
    assert_eq!(
        got,
        vec![Value::Int(7), Value::Int(8)],
        "边属性必须逐个来自 UNWIND 变量"
    );

    // 端点节点的属性同样来自 UNWIND 变量
    let got = column(&db, "MATCH (a:L) RETURN a.i ORDER BY a.i")?;
    assert_eq!(got, vec![Value::Int(7), Value::Int(8)]);

    Ok(())
}

#[test]
fn test_unwind_unique_constraint_applies_to_every_element() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("unwind_unique.db")?;

    db.run_cypher("CREATE (:Num {v: 1})")?;
    db.create_unique_constraint("Num", "v")?;

    // 列表里有一个与既有节点冲突的值：整条语句必须失败。
    // 若实现成「跳过冲突项继续」，调用方会以为 3 条都写成功了。
    let err = db
        .execute("UNWIND [2, 1, 3] AS v CREATE (n:Num {v: v})")
        .expect_err("重复值必须被拒绝");
    assert!(
        err.to_string().contains("Unique constraint"),
        "应报告唯一约束冲突，实际: {err}"
    );

    // 关键：不能留下部分写入的 2
    let got = column(&db, "MATCH (n:Num) RETURN n.v ORDER BY n.v")?;
    assert_eq!(got, vec![Value::Int(1)], "失败的语句不得留下部分写入的节点");

    Ok(())
}

// =========================================================================
// 与投影管线的组合
// =========================================================================

#[test]
fn test_unwind_works_with_aggregation_and_paging() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("unwind_agg.db")?;

    // 聚合：UNWIND 产生的行应正常参与聚合
    assert_eq!(
        column(&db, "UNWIND [1, 2, 3, 4] AS x RETURN sum(x)")?[0],
        Value::Int(10)
    );
    assert_eq!(
        column(&db, "UNWIND [1, 2, 3, 4] AS x RETURN count(*)")?[0],
        Value::Int(4)
    );
    assert_eq!(
        column(&db, "UNWIND [1, 2, 3, 4] AS x RETURN avg(x)")?[0],
        Value::Float(2.5)
    );

    // ORDER BY + LIMIT：排序必须在截断之前发生
    let got = column(
        &db,
        "UNWIND [5, 1, 4, 2, 3] AS x RETURN x ORDER BY x DESC LIMIT 2",
    )?;
    assert_eq!(got, vec![Value::Int(5), Value::Int(4)]);

    // SKIP
    let got = column(
        &db,
        "UNWIND [5, 1, 4, 2, 3] AS x RETURN x ORDER BY x SKIP 3",
    )?;
    assert_eq!(got, vec![Value::Int(4), Value::Int(5)]);

    Ok(())
}

#[test]
fn test_unwind_return_star_includes_the_variable() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("unwind_star.db")?;

    // `RETURN *` 在无模式可依赖时必须至少给出 UNWIND 变量。
    // 返回零列会让下游按列名取值的代码全部落空。
    let res = db.run_cypher("UNWIND [1, 2] AS x RETURN *")?;
    assert_eq!(res.columns, vec!["x".to_string()]);
    assert_eq!(res.row_count(), 2);

    Ok(())
}

// =========================================================================
// 拒绝而非静默
// =========================================================================

#[test]
fn test_match_rejects_non_literal_pattern_properties() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("unwind_match_literal.db")?;

    db.run_cypher("CREATE (:P {v: 1})")?;

    // `MATCH (n {v: someVar})` 无法求值（n 尚未绑定）。
    // 必须报错：若静默当作「不匹配」，调用方看到 0 行却不知道条件根本没被求值。
    let err = db
        .execute("MATCH (n {v: m}) RETURN n")
        .expect_err("非字面量模式属性必须被拒绝");
    assert!(
        err.to_string().contains("literal"),
        "错误信息应说明需要字面量，实际: {err}"
    );

    // 字面量仍然正常工作
    let res = db.run_cypher("MATCH (n:P {v: 1}) RETURN n")?;
    assert_eq!(res.row_count(), 1);

    Ok(())
}

#[test]
fn test_unwind_requires_as_variable() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("unwind_syntax.db")?;

    assert!(db.run_cypher("UNWIND [1,2,3]").is_err(), "缺少 AS 必须报错");
    assert!(
        db.run_cypher("UNWIND [1,2,3] AS").is_err(),
        "缺少变量名必须报错"
    );

    Ok(())
}

#[test]
fn test_unwind_scalar_binding_has_no_properties() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("unwind_scalar_prop.db")?;

    // 标量没有属性：属性访问返回 NULL（与 Cypher 一致），而不是报错或伪造值
    let got = column(&db, "UNWIND [1] AS x RETURN x.k")?;
    assert_eq!(got, vec![Value::Null]);

    Ok(())
}

#[test]
fn test_unwind_created_nodes_are_queryable_by_index() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("unwind_index.db")?;

    // UNWIND 路径必须维护二级索引：否则按标签查询会漏掉这些节点
    db.run_cypher("UNWIND ['u1', 'u2', 'u3'] AS name CREATE (u:User {name: name})")?;

    let res = db.run_cypher("MATCH (u:User) RETURN count(*)")?;
    assert_eq!(res.rows[0].values[0], Value::Int(3));

    // 按属性精确匹配（走属性索引）
    let res = db.run_cypher("MATCH (u:User {name: 'u2'}) RETURN u.name")?;
    assert_eq!(
        res.rows[0].values[0],
        Value::String("u2".to_string()),
        "属性索引必须覆盖 UNWIND 写入的数据"
    );

    Ok(())
}

#[test]
fn test_read_only_handle_allows_readonly_unwind() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let path = dir.path().join("unwind_ro.db");

    {
        let db = GraphLite::open(&path)?;
        db.run_cypher("CREATE (:Seed {v: 1})")?;
        db.checkpoint()?;
    }

    let ro = GraphLite::open_read_only(&path)?;

    // 纯读 UNWIND 不写任何东西，只读句柄应当接受
    let res = ro.run_cypher("UNWIND [1, 2, 3] AS x RETURN x")?;
    assert_eq!(res.row_count(), 3, "只读句柄应能执行纯读 UNWIND");

    // 带 CREATE 的 UNWIND 必须被拒绝
    let err = ro
        .execute("UNWIND [1, 2] AS x CREATE (n:N {v: x})")
        .expect_err("只读句柄不得执行写 UNWIND");
    assert!(
        err.to_string().contains("Read-only"),
        "应报告只读限制，实际: {err}"
    );

    Ok(())
}

#[test]
fn test_explain_unwind_has_no_side_effects() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("unwind_explain.db")?;

    db.run_cypher("CREATE (:P {v: 1})")?;
    let before = column(&db, "MATCH (n) RETURN count(*)")?;

    let plan = db.run_cypher("EXPLAIN UNWIND [1, 2] AS x CREATE (n:X {v: x})")?;
    // EXPLAIN 每行是一个计划行，需要拼起来看
    let text = plan
        .rows
        .iter()
        .map(|r| format!("{:?}", r.values[0]))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("Unwind"),
        "计划里应出现 Unwind 步骤，实际: {text}"
    );
    assert!(
        text.contains("WriteQuery"),
        "带 CREATE 的 UNWIND 应标记为写查询，实际: {text}"
    );

    let after = column(&db, "MATCH (n) RETURN count(*)")?;
    assert_eq!(before, after, "EXPLAIN 绝不能产生副作用");

    // 纯读 UNWIND 的计划应标记为读查询
    let plan = db.run_cypher("EXPLAIN UNWIND [1] AS x RETURN x")?;
    let text = plan
        .rows
        .iter()
        .map(|r| format!("{:?}", r.values[0]))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("ReadQuery"),
        "纯读 UNWIND 应标记为读查询，实际: {text}"
    );

    Ok(())
}

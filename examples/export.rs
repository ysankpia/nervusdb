// 样例 10：导出与可视化数据
use nervusdb::NervusDb;

fn main() -> Result<(), nervusdb::GraphError> {
    let dir = tempfile::tempdir().unwrap();
    let db = NervusDb::open(dir.path().join("t.db"))?;
    db.run_cypher("CREATE (a:N {v: 1})-[:R]->(b:N {v: 2})")?;

    let mut out = Vec::new();
    db.dump_cypher(&mut out)?;
    println!("{}", String::from_utf8_lossy(&out));

    let export = db.export_subgraph(5_000)?;
    println!("{}", export.to_json());
    assert!(
        !export.truncated,
        "超过 limit 时 truncated 为 true，不会静默截断"
    );
    Ok(())
}

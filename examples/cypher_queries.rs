// 样例 5：Cypher 查询
use nervusdb::NervusDb;

fn main() -> Result<(), nervusdb::GraphError> {
    let dir = tempfile::tempdir().unwrap();
    let db = NervusDb::open(dir.path().join("t.db"))?;

    db.run_cypher("CREATE (:Person {name: 'a', age: 1})")?;

    let rows = db.run_cypher("MATCH (n:Person) RETURN n.name AS name, n.age AS age")?;
    for row in &rows.rows {
        println!("{:?}", row.values);
    }

    let stats = db.execute("UNWIND [1,2,3] AS i CREATE (n:Num {v: i})")?;
    assert_eq!(stats.nodes_created, 3);

    let plan = db.run_cypher("EXPLAIN MATCH (n:Person) RETURN n")?;
    println!("{:?}", plan.rows[0].values[0]);
    Ok(())
}

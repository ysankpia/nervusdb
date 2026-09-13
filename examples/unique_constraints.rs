// 样例 6：唯一约束 + 跨重开存活
use nervusdb::{NervusDb, Value};
use std::collections::{HashMap, HashSet};

fn main() -> Result<(), nervusdb::GraphError> {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("t.db");
    {
        let db = NervusDb::open(&p)?;
        db.create_unique_constraint("Person", "name")?;
        db.run_cypher("CREATE (:Person {name: 'alice'})")?;
        assert!(db.run_cypher("CREATE (:Person {name: 'alice'})").is_err());
        assert!(db
            .with_transaction(|tx| {
                tx.add_node(
                    HashSet::from(["Person".to_string()]),
                    HashMap::from([("name".to_string(), Value::from("alice"))]),
                )?;
                Ok(())
            })
            .is_err());
        println!("{:?}", db.unique_constraints());
    }
    // 跨重开存活
    let db = NervusDb::open(&p)?;
    assert_eq!(db.unique_constraints().len(), 1);
    Ok(())
}

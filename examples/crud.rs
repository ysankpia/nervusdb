// 样例 2：增删查改
use nervusdb::{NervusDb, Value};
use std::collections::{HashMap, HashSet};

fn main() -> Result<(), nervusdb::GraphError> {
    let dir = tempfile::tempdir().unwrap();
    let db = NervusDb::open(dir.path().join("t.db"))?;

    let id = db.add_node(
        HashSet::from(["Person".to_string()]),
        HashMap::from([("name".to_string(), Value::from("林渊"))]),
    )?;
    let other = db.add_node(HashSet::new(), HashMap::new())?;
    db.add_edge(id, other, "KNOWS", HashMap::new(), 1.0)?;

    if let Some(node) = db.get_node(id) {
        println!("{:?}", node.properties);
    }
    let node = db.try_get_node(id)?;
    assert!(node.is_some());

    db.update_node_property(id, "age", 28i64)?;
    db.remove_edge(1)?;
    db.remove_node(other)?;

    assert_eq!(db.node_count(), 1);
    println!("labels: {:?}", db.index_labels());
    Ok(())
}

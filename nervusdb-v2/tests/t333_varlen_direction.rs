use nervusdb_v2::Db;
use nervusdb_v2_query::{Params, Result, Value, prepare};
use tempfile::tempdir;

fn setup_graph() -> Db {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let mut txn = db.begin_write();
        let label = txn.get_or_create_label("N").unwrap();
        let rel = txn.get_or_create_rel_type("R").unwrap();

        let a = txn.create_node(1, label).unwrap();
        let b = txn.create_node(2, label).unwrap();
        let c = txn.create_node(3, label).unwrap();

        txn.set_node_property(a, "name".to_string(), "A".into())
            .unwrap();
        txn.set_node_property(b, "name".to_string(), "B".into())
            .unwrap();
        txn.set_node_property(c, "name".to_string(), "C".into())
            .unwrap();

        txn.create_edge(a, rel, b);
        txn.create_edge(b, rel, c);
        txn.commit().unwrap();
    }

    db
}

#[test]
fn t333_variable_length_supports_right_to_left_direction() {
    let db = setup_graph();
    let snapshot = db.snapshot();
    let params = Params::new();

    let query = prepare(
        "MATCH (b {name: 'B'})<-[:R*1..2]-(a)
         RETURN a.name AS name
         ORDER BY name",
    )
    .unwrap();

    let rows: Vec<_> = query
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>>>()
        .unwrap();

    let names: Vec<_> = rows
        .iter()
        .map(|row| row.get("name").cloned().unwrap_or(Value::Null))
        .collect();

    assert_eq!(names, vec![Value::String("A".to_string())]);
}

#[test]
fn t333_variable_length_supports_undirected_direction() {
    let db = setup_graph();
    let snapshot = db.snapshot();
    let params = Params::new();

    let query = prepare(
        "MATCH (b {name: 'B'})-[:R*1..1]-(x)
         RETURN x.name AS name
         ORDER BY name",
    )
    .unwrap();

    let rows: Vec<_> = query
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>>>()
        .unwrap();

    let names: Vec<_> = rows
        .iter()
        .map(|row| row.get("name").cloned().unwrap_or(Value::Null))
        .collect();

    assert_eq!(
        names,
        vec![
            Value::String("A".to_string()),
            Value::String("C".to_string())
        ]
    );
}

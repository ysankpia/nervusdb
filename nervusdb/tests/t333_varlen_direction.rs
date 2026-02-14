use nervusdb::Db;
use nervusdb_query::{Params, Result, Value, prepare};
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

#[test]
fn t333_variable_length_supports_bidirectional_arrow_form() {
    let db = setup_graph();
    let snapshot = db.snapshot();
    let params = Params::new();

    let query = prepare(
        "MATCH (b {name: 'B'})<-[:R*1..1]->(x)
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

fn setup_reuse_graph() -> Db {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let mut txn = db.begin_write();
        let label_a = txn.get_or_create_label("A").unwrap();
        let label_b = txn.get_or_create_label("B").unwrap();
        let label_c = txn.get_or_create_label("C").unwrap();
        let label_d = txn.get_or_create_label("D").unwrap();
        let label_e = txn.get_or_create_label("E").unwrap();
        let rel = txn.get_or_create_rel_type("R").unwrap();

        let a = txn.create_node(10, label_a).unwrap();
        let b1 = txn.create_node(11, label_b).unwrap();
        let b2 = txn.create_node(12, label_b).unwrap();
        let c1 = txn.create_node(13, label_c).unwrap();
        let c2 = txn.create_node(14, label_c).unwrap();
        let d1 = txn.create_node(15, label_d).unwrap();
        let d2 = txn.create_node(16, label_d).unwrap();
        let e1 = txn.create_node(17, label_e).unwrap();
        let e2 = txn.create_node(18, label_e).unwrap();

        txn.set_node_property(a, "name".to_string(), "a".into())
            .unwrap();
        txn.set_node_property(b1, "name".to_string(), "b1".into())
            .unwrap();
        txn.set_node_property(b2, "name".to_string(), "b2".into())
            .unwrap();
        txn.set_node_property(c1, "name".to_string(), "c1".into())
            .unwrap();
        txn.set_node_property(c2, "name".to_string(), "c2".into())
            .unwrap();
        txn.set_node_property(d1, "name".to_string(), "d1".into())
            .unwrap();
        txn.set_node_property(d2, "name".to_string(), "d2".into())
            .unwrap();
        txn.set_node_property(e1, "name".to_string(), "e1".into())
            .unwrap();
        txn.set_node_property(e2, "name".to_string(), "e2".into())
            .unwrap();

        txn.create_edge(a, rel, b1);
        txn.create_edge(a, rel, b2);
        txn.create_edge(c1, rel, b1);
        txn.create_edge(c2, rel, b2);
        txn.create_edge(d1, rel, c1);
        txn.create_edge(d2, rel, c2);
        txn.create_edge(d1, rel, e1);
        txn.create_edge(d2, rel, e2);
        txn.commit().unwrap();
    }

    db
}

#[test]
fn t333_varlen_does_not_reuse_bound_edge_from_previous_hop() {
    let db = setup_reuse_graph();
    let snapshot = db.snapshot();
    let params = Params::new();

    let query = prepare(
        "MATCH (a:A)
         MATCH (a)-[:R]->()<-[:R*3]->(c)
         RETURN c.name AS name
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
            Value::String("e1".to_string()),
            Value::String("e2".to_string())
        ]
    );
}

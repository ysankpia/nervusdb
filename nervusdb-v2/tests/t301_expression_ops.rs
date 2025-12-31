use nervusdb_v2::query::{Params, Value};
use nervusdb_v2::{Db, GraphSnapshot, PropertyValue};
use tempfile::tempdir;

#[test]
fn test_arithmetic_in_where_and_set() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t301_expr.ndb");
    let db = Db::open(&db_path)?;

    // Seed data.
    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person")?;
        let alice = txn.create_node(1, person)?;
        txn.set_node_property(
            alice,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )?;
        txn.set_node_property(alice, "age".to_string(), PropertyValue::Int(20))?;

        let bob = txn.create_node(2, person)?;
        txn.set_node_property(
            bob,
            "name".to_string(),
            PropertyValue::String("Bob".to_string()),
        )?;
        txn.set_node_property(bob, "age".to_string(), PropertyValue::Int(30))?;

        txn.commit()?;
    }

    // Arithmetic in WHERE.
    {
        let snapshot = db.snapshot();
        let q = "MATCH (n:Person) WHERE n.age + 1 = 21 RETURN n";
        let prep = nervusdb_v2::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);

        let n = rows[0].get("n").unwrap();
        let Value::NodeId(iid) = n else {
            panic!("expected node id, got {n:?}");
        };
        assert_eq!(
            snapshot.node_property(*iid, "name"),
            Some(PropertyValue::String("Alice".to_string()))
        );
    }

    // Arithmetic in SET (increment age).
    {
        let read_snapshot = db.snapshot();
        let mut txn = db.begin_write();
        let q = "MATCH (n:Person) WHERE n.name = 'Alice' SET n.age = n.age + 1";
        let prep = nervusdb_v2::query::prepare(q)?;
        let n = prep.execute_write(&read_snapshot, &mut txn, &Params::default())?;
        assert_eq!(n, 1);
        txn.commit()?;
    }

    let snapshot = db.snapshot();
    let alice = snapshot
        .nodes()
        .find(|&iid| snapshot.resolve_external(iid) == Some(1))
        .expect("node 1 should exist");
    assert_eq!(
        snapshot.node_property(alice, "age"),
        Some(PropertyValue::Int(21))
    );

    Ok(())
}

#[test]
fn test_string_ops_in_and_count_star() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t301_str.ndb");
    let db = Db::open(&db_path)?;

    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person")?;

        let a = txn.create_node(1, person)?;
        txn.set_node_property(
            a,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )?;
        txn.set_node_property(a, "age".to_string(), PropertyValue::Int(20))?;

        let b = txn.create_node(2, person)?;
        txn.set_node_property(
            b,
            "name".to_string(),
            PropertyValue::String("Bob".to_string()),
        )?;
        txn.set_node_property(b, "age".to_string(), PropertyValue::Int(30))?;

        txn.commit()?;
    }

    let snapshot = db.snapshot();

    // STARTS WITH / ENDS WITH / CONTAINS
    {
        let q = "MATCH (n:Person) WHERE n.name STARTS WITH 'A' RETURN n";
        let prep = nervusdb_v2::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);
    }
    {
        let q = "MATCH (n:Person) WHERE n.name ENDS WITH 'b' RETURN n";
        let prep = nervusdb_v2::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);
    }
    {
        let q = "MATCH (n:Person) WHERE n.name CONTAINS 'li' RETURN n";
        let prep = nervusdb_v2::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);
    }

    // IN with list literal.
    {
        let q = "MATCH (n:Person) WHERE n.age IN [20, 30] RETURN n";
        let prep = nervusdb_v2::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 2);
    }

    // COUNT(*)
    {
        let q = "MATCH (n:Person) RETURN count(*)";
        let prep = nervusdb_v2::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("agg_0"), Some(&Value::Float(2.0)));
    }

    Ok(())
}

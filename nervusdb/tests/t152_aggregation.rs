use nervusdb::query::{Value, WriteableGraph};
use nervusdb::{Db, GraphSnapshot, PropertyValue};
use tempfile::tempdir;

#[test]
fn test_aggregation_functions() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t152.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    // Create data:
    // Person A: knows B (age 20), knows C (age 30), knows D (age 20)
    // Person X: knows Y (age 40)

    let person_label = txn.get_or_create_label("Person")?;
    let knows_rel = txn.get_or_create_rel_type_id("KNOWS")?;

    // A
    let a = txn.create_node(100, person_label)?;
    txn.set_node_property(
        a,
        "name".to_string(),
        PropertyValue::String("A".to_string()),
    )?;

    // B
    let b = txn.create_node(101, person_label)?;
    txn.set_node_property(
        b,
        "name".to_string(),
        PropertyValue::String("B".to_string()),
    )?;
    txn.set_node_property(b, "age".to_string(), PropertyValue::Int(20))?;

    // C
    let c = txn.create_node(102, person_label)?;
    txn.set_node_property(
        c,
        "name".to_string(),
        PropertyValue::String("C".to_string()),
    )?;
    txn.set_node_property(c, "age".to_string(), PropertyValue::Int(30))?;

    // D
    let d = txn.create_node(103, person_label)?;
    txn.set_node_property(
        d,
        "name".to_string(),
        PropertyValue::String("D".to_string()),
    )?;
    txn.set_node_property(d, "age".to_string(), PropertyValue::Int(20))?;

    // X
    let x = txn.create_node(104, person_label)?;
    txn.set_node_property(
        x,
        "name".to_string(),
        PropertyValue::String("X".to_string()),
    )?;

    // Y
    let y = txn.create_node(105, person_label)?;
    txn.set_node_property(
        y,
        "name".to_string(),
        PropertyValue::String("Y".to_string()),
    )?;
    txn.set_node_property(y, "age".to_string(), PropertyValue::Int(40))?;

    txn.create_edge(a, knows_rel, b);
    txn.create_edge(a, knows_rel, c);
    txn.create_edge(a, knows_rel, d);
    txn.create_edge(x, knows_rel, y);

    txn.commit()?;
    let snapshot = db.snapshot();

    // Test 1: Count friends per person
    {
        let query = "MATCH (n:Person)-[:KNOWS]->(friend) RETURN n, count(friend)";
        let prep = nervusdb::query::prepare(query)?;
        let results: Vec<_> = prep
            .execute_streaming(&snapshot, &Default::default())
            .collect::<Result<Vec<_>, _>>()?;

        // Should have 2 rows: A -> 3, X -> 1
        assert_eq!(results.len(), 2);

        for row in results {
            let n_node = row.get("n").unwrap();
            let count = row
                .get("count(friend)")
                .or_else(|| row.get("count_1"))
                .or_else(|| row.get("agg_1"))
                .unwrap();

            if let Value::NodeId(id) = n_node {
                let name_val = snapshot.node_property(*id, "name").unwrap();
                let name = if let PropertyValue::String(s) = name_val {
                    s
                } else {
                    panic!("not string")
                };

                if name == "A" {
                    assert!(matches!(*count, Value::Int(3) | Value::Float(3.0)));
                } else if name == "X" {
                    assert!(matches!(*count, Value::Int(1) | Value::Float(1.0)));
                }
            }
        }
    }

    // Test 2: Min/Max age of friends
    {
        let query =
            "MATCH (n:Person)-[:KNOWS]->(friend) RETURN n, min(friend.age), max(friend.age)";
        let prep = nervusdb::query::prepare(query)?;
        let results: Vec<_> = prep
            .execute_streaming(&snapshot, &Default::default())
            .collect::<Result<Vec<_>, _>>()?;

        for row in results {
            let n_node = row.get("n").unwrap();
            let min_age = row
                .get("min(friend.age)")
                .or_else(|| row.get("min_1"))
                .or_else(|| row.get("agg_1"))
                .unwrap();
            let max_age = row
                .get("max(friend.age)")
                .or_else(|| row.get("max_2"))
                .or_else(|| row.get("agg_2"))
                .unwrap();

            if let Value::NodeId(id) = n_node {
                let name_val = snapshot.node_property(*id, "name").unwrap();
                let name = if let PropertyValue::String(s) = name_val {
                    s
                } else {
                    panic!("not string")
                };

                if name == "A" {
                    // Friends of A: B(20), C(30), D(20)
                    assert_eq!(*min_age, Value::Int(20));
                    assert_eq!(*max_age, Value::Int(30));
                }
            }
        }
    }

    // Test 3: Collect
    {
        let query = "MATCH (n:Person)-[:KNOWS]->(friend) RETURN n, collect(friend.age)";
        let prep = nervusdb::query::prepare(query)?;
        let results: Vec<_> = prep
            .execute_streaming(&snapshot, &Default::default())
            .collect::<Result<Vec<_>, _>>()?;

        for row in results {
            let n_node = row.get("n").unwrap();
            let ages_val = row
                .get("collect(friend.age)")
                .or_else(|| row.get("collect_1"))
                .or_else(|| row.get("agg_1"))
                .unwrap();

            if let Value::NodeId(id) = n_node {
                let name_val = snapshot.node_property(*id, "name").unwrap();
                let name = if let PropertyValue::String(s) = name_val {
                    s
                } else {
                    panic!("not string")
                };

                if name == "A" {
                    if let Value::List(l) = ages_val {
                        // Should be [20, 30, 20]
                        assert!(l.len() == 3);
                        assert!(l.contains(&Value::Int(20)));
                        assert!(l.contains(&Value::Int(30)));
                    } else {
                        panic!("Expected List, got {:?}", ages_val);
                    }
                }
            }
        }
    }

    Ok(())
}

#[test]
fn test_return6_count_plus_arithmetic_expression() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t152_return6_expr.ndb");
    let db = Db::open(&db_path)?;

    {
        let mut txn = db.begin_write();
        nervusdb::query::prepare("CREATE ({id: 42})")?.execute_write(
            &db.snapshot(),
            &mut txn,
            &Default::default(),
        )?;
        txn.commit()?;
    }

    let snapshot = db.snapshot();
    let query = nervusdb::query::prepare("MATCH (a) RETURN a, count(a) + 3")?;
    let results: Vec<_> = query
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("count(a) + 3"), Some(&Value::Int(4)));
    Ok(())
}

#[test]
fn test_return6_count_division_still_aggregates() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t152_return6_div.ndb");
    let db = Db::open(&db_path)?;

    {
        let mut txn = db.begin_write();
        nervusdb::query::prepare("UNWIND range(0, 7250) AS i CREATE ()")?.execute_write(
            &db.snapshot(),
            &mut txn,
            &Default::default(),
        )?;
        txn.commit()?;
    }

    let snapshot = db.snapshot();
    let query = nervusdb::query::prepare("MATCH (n) RETURN count(n) / 60 / 60 AS count")?;
    let results: Vec<_> = query
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("count"), Some(&Value::Int(2)));
    Ok(())
}

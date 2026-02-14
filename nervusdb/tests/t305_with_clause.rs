use nervusdb::query::{Params, Value};
use nervusdb::{Db, PropertyValue};
use tempfile::tempdir;

#[test]
fn test_with_clause_chaining() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t305_with.ndb");
    let db = Db::open(&db_path)?;

    // Seed
    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person")?;
        let knows = txn.get_or_create_rel_type("KNOWS")?;

        let a = txn.create_node(1, person)?;
        txn.set_node_property(
            a,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )?;
        txn.set_node_property(a, "age".to_string(), PropertyValue::Int(30))?;

        let b = txn.create_node(2, person)?;
        txn.set_node_property(
            b,
            "name".to_string(),
            PropertyValue::String("Bob".to_string()),
        )?;
        txn.set_node_property(b, "age".to_string(), PropertyValue::Int(30))?;

        let c = txn.create_node(3, person)?;
        txn.set_node_property(
            c,
            "name".to_string(),
            PropertyValue::String("Charlie".to_string()),
        )?;
        txn.set_node_property(c, "age".to_string(), PropertyValue::Int(25))?;

        txn.create_edge(a, knows, b);
        txn.create_edge(b, knows, c);

        txn.commit()?;
    }

    let snapshot = db.snapshot();

    // 1. Simple WITH filtering and projection
    {
        let q = "MATCH (n:Person) WITH n WHERE n.age > 28 RETURN n.name";
        let prep = nervusdb::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        // Alice(30), Bob(30) -> 2 results
        assert_eq!(rows.len(), 2);
    }

    // 2. WITH aggregation
    {
        let q = "MATCH (n:Person) WITH n.age as age, count(*) as count WHERE count > 1 RETURN age, count";
        let prep = nervusdb::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        // 30 -> 2
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("age"), Some(&Value::Int(30)));
        assert!(matches!(
            rows[0].get("count"),
            Some(Value::Int(2)) | Some(Value::Float(2.0))
        ));
    }

    // 3. Chained MATCH
    // Find who Alice knows, then find who *they* know
    {
        let q = "MATCH (a:Person {name: 'Alice'})-[:KNOWS]->(b) WITH b MATCH (b)-[:KNOWS]->(c) RETURN c.name";
        let prep = nervusdb::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        // Alice -> Bob -> Charlie. Should return Charlie.
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("c.name"),
            Some(&Value::String("Charlie".to_string()))
        );
    }

    // 4. ORDER BY and LIMIT in WITH
    {
        let q = "MATCH (n:Person) WITH n ORDER BY n.age DESC LIMIT 1 RETURN n.name";
        let prep = nervusdb::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        // Bob or Alice (30). Limit 1.
        assert_eq!(rows.len(), 1);
    }

    Ok(())
}

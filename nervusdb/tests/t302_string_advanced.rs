use nervusdb::query::{Params, Value};
use nervusdb::{Db, GraphSnapshot, PropertyValue};
use tempfile::tempdir;

#[test]
fn test_string_functions() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t302_string.ndb");
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
        txn.set_node_property(
            a,
            "city".to_string(),
            PropertyValue::String(" New York ".to_string()),
        )?;
        txn.commit()?;
    }

    let snapshot = db.snapshot();

    // toLower()
    {
        let q = "MATCH (n:Person) WHERE toLower(n.name) = 'alice' RETURN n";
        let prep = nervusdb::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);
        let n = rows[0].get("n").unwrap();
        let Value::NodeId(iid) = n else {
            panic!("not a node")
        };
        assert_eq!(
            snapshot.node_property(*iid, "name"),
            Some(PropertyValue::String("Alice".to_string()))
        );
    }

    // toUpper()
    {
        let q = "MATCH (n:Person) WHERE toUpper(n.name) = 'ALICE' RETURN n";
        let prep = nervusdb::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);
    }

    // reverse()
    {
        let q = "MATCH (n:Person) WHERE reverse(n.name) = 'ecilA' RETURN n";
        let prep = nervusdb::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);
    }

    // trim()
    {
        let q = "MATCH (n:Person) WHERE trim(n.city) = 'New York' RETURN n";
        let prep = nervusdb::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);
    }

    // substring()
    {
        // substring(string, start, length) or substring(string, start)
        let q = "MATCH (n:Person) WHERE substring(n.name, 1, 3) = 'lic' RETURN n";
        let prep = nervusdb::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);
    }

    // toString()
    {
        let q = "MATCH (n:Person) WHERE toString(123) = '123' RETURN n";
        let prep = nervusdb::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);
    }

    // T303: IN with parameter
    {
        let q = "MATCH (n:Person) WHERE n.name IN $names RETURN n";
        let mut params = Params::default();
        params.insert(
            "names".to_string(),
            Value::List(vec![
                Value::String("Alice".to_string()),
                Value::String("Bob".to_string()),
            ]),
        );
        let prep = nervusdb::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &params)
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1); // Only Alice matches
    }

    Ok(())
}

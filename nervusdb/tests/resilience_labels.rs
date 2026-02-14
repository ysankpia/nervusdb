use nervusdb::Db;
use nervusdb::query::{GraphSnapshot, Params, QueryExt};
use tempfile::tempdir;

#[test]
fn test_string_label_resilience() -> nervusdb::Result<()> {
    // 1. Setup: Open a database in a temporary directory
    let dir = tempdir().map_err(nervusdb::Error::Io)?;
    let db_path = dir.path().join("resilience.ndb");

    // Scope 1: Write initial data and close
    {
        println!("Cycle 1: Opening DB for write...");
        let db = Db::open(&db_path)?;
        let mut txn = db.begin_write();
        let snapshot = db.snapshot();

        // Create Alice -[:KNOWS]-> Bob
        let cypher = "CREATE (a:User {name: 'Alice'})-[:KNOWS]->(b:User {name: 'Bob'})";
        let query = nervusdb::query::prepare(cypher)
            .map_err(|e| nervusdb::Error::Io(std::io::Error::other(e.to_string())))?;

        query
            .execute_write(&snapshot, &mut txn, &Params::default())
            .map_err(|e| nervusdb::Error::Io(std::io::Error::other(e.to_string())))?;

        println!("Cycle 1: Committing...");
        txn.commit()?;
        // db is dropped here, simulating close
    }

    println!("Cycle 1: DB Closed. Re-opening...");

    // Scope 2: Re-open and verify
    {
        println!("Cycle 2: Opening DB for read...");
        let db = Db::open(&db_path)?;
        let snapshot = db.snapshot();

        // Query 1: Find by Label "User"
        let cypher = "MATCH (n:User) RETURN n";
        let rows = snapshot
            .query(cypher, &Params::default())
            .map_err(|e| nervusdb::Error::Io(std::io::Error::other(e.to_string())))?;

        assert_eq!(rows.len(), 2, "Should find 2 users after restart");

        // Query 2: Check Relationship Type persistence (KNOWS)
        let cypher_rel = "MATCH (a)-[r:KNOWS]->(b) RETURN a, b";
        let rel_rows = snapshot
            .query(cypher_rel, &Params::default())
            .map_err(|e| nervusdb::Error::Io(std::io::Error::other(e.to_string())))?;

        assert_eq!(
            rel_rows.len(),
            1,
            "Should find 1 KNOWS relationship after restart"
        );

        // Verify property content
        let alice_id = rel_rows[0].get_node("a").unwrap();
        let bob_id = rel_rows[0].get_node("b").unwrap();

        let alice_name = snapshot.node_property(alice_id, "name").unwrap();
        let bob_name = snapshot.node_property(bob_id, "name").unwrap();

        match alice_name {
            nervusdb::PropertyValue::String(s) => assert_eq!(s, "Alice"),
            _ => panic!("Alice name mismatch"),
        }
        match bob_name {
            nervusdb::PropertyValue::String(s) => assert_eq!(s, "Bob"),
            _ => panic!("Bob name mismatch"),
        }
    }

    println!("Resilience test passed!");
    Ok(())
}

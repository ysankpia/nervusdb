//! # NervusDB Tour (The Matrix)
//!
//! This example demonstrates the core workflow of NervusDB v2:
//! 1. Setting up the database.
//! 2. Defining schema (interning strings).
//! 3. Writing data (Nodes & Edges).
//! 4. Querying data (Cypher).

use nervusdb::query::GraphSnapshot;
use nervusdb::{Db, Result};
use tempfile::tempdir;

fn main() -> Result<()> {
    println!("💊 Welcome to the NervusDB Matrix Tour");

    // 1. Setup: Open a database in a temporary directory
    let dir = tempdir()?;
    let db_path = dir.path().join("matrix.ndb");
    let db = Db::open(&db_path)?;
    println!("✅ Database opened at: {:?}", db_path);

    // 2. Write Data
    // 2. Write Data
    {
        println!("📝 Writing data with Cypher...");
        let mut txn = db.begin_write();
        let snapshot = db.snapshot();

        // Create Keanu -[:ACTED_IN]-> Matrix in one go
        // Demonstrates T65: String labels and relationship types in CREATE
        let cypher =
            "CREATE (k:Person {name: 'Keanu Reeves'})-[:ACTED_IN]->(m:Movie {title: 'The Matrix'})";
        let query = nervusdb::query::prepare(cypher)
            .map_err(|e| nervusdb::Error::Io(std::io::Error::other(e.to_string())))?;

        query
            .execute_write(&snapshot, &mut txn, &nervusdb::query::Params::default())
            .map_err(|e| nervusdb::Error::Io(std::io::Error::other(e.to_string())))?;
        txn.commit()?;
    }

    // 3. Query (Cypher)
    use nervusdb::query::{Params, QueryExt};

    println!("\n🔍 Executing Cypher Query...");

    // "Find people who acted in 'The Matrix'"
    // Demonstrates T65: String labels in MATCH
    let cypher = "MATCH (p:Person)-[r:ACTED_IN]->(m) WHERE m.title = 'The Matrix' RETURN p, r";

    let snapshot = db.snapshot();
    let rows = snapshot
        .query(cypher, &Params::default())
        .map_err(|e| nervusdb::Error::Io(std::io::Error::other(e.to_string())))?;

    println!("Found {} row(s):", rows.len());
    for (i, row) in rows.iter().enumerate() {
        let p_id = row.get_node("p").unwrap();
        let _r_key = row.get_edge("r").unwrap(); // EdgeKey

        let name = snapshot
            .node_property(p_id, "name")
            .unwrap_or(nervusdb::query::PropertyValue::Null);

        // Note: r has property 'role' only if we set it.
        // Our CREATE statement above didn't set 'role' property on the edge (limitation of MVP CREATE parser or just brevity).
        // Let's print what we have to be safe.
        println!("Row {}: Actor={:?}", i + 1, name);
    }

    // 4. Cleanup (optional, tempdir handles it)
    println!("\n✨ Tour completed successfully!");
    Ok(())
}

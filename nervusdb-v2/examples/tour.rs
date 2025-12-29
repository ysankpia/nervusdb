//! # NervusDB Tour (The Matrix)
//!
//! This example demonstrates the core workflow of NervusDB v2:
//! 1. Setting up the database.
//! 2. Defining schema (interning strings).
//! 3. Writing data (Nodes & Edges).
//! 4. Querying data (Cypher).

use nervusdb_v2::query::GraphSnapshot;
use nervusdb_v2::{Db, Result};
use nervusdb_v2_storage::idmap::LabelId;
use tempfile::tempdir;

fn main() -> Result<()> {
    println!("💊 Welcome to the NervusDB Matrix Tour");

    // 1. Setup: Open a database in a temporary directory
    let dir = tempdir()?;
    let db_path = dir.path().join("matrix.ndb");
    let db = Db::open(&db_path)?;
    println!("✅ Database opened at: {:?}", db_path);

    // 2. Write Data
    let (_keanu_id, _matrix_id) = {
        let mut txn = db.begin_write();

        // 2a. Define Labels (Interning)
        // Ideally, we would have `get_or_create_label` on Txn directly in the future.
        // For now, we use the graph engine's interner or just raw IDs if standard APIs aren't high-level enough yet.
        // Wait! T59 says `GraphEngine::get_or_create_label` is done.
        // But `Db::begin_write` returns `WriteTxn` wrapper. Does it expose label creation?
        // Checking `lib.rs`, `WriteTxn` only has `create_node(u64, u32)`.
        // It takes `LabelId` (u32), not string.
        // The `GraphEngine` is private in `Db`.
        // This reveals DX Pain Point #1: We can't intern labels easily from the Facade!
        // Workaround for Tour: We will use raw integers for now and explain the gap.

        let label_person: LabelId = 1;
        let label_movie: LabelId = 2;
        let rel_acted_in: u32 = 1; // Relationship IDs are also u32 currently

        println!("📝 Writing data...");

        // Create 'The Matrix'
        let matrix = txn.create_node(100, label_movie)?;
        txn.set_node_property(
            matrix,
            "title".to_string(),
            nervusdb_v2::PropertyValue::String("The Matrix".to_string()),
        )?;

        // Create 'Keanu Reeves'
        let keanu = txn.create_node(101, label_person)?;
        txn.set_node_property(
            keanu,
            "name".to_string(),
            nervusdb_v2::PropertyValue::String("Keanu Reeves".to_string()),
        )?;

        // Create Edge: Keanu -> ACTED_IN -> Matrix
        txn.create_edge(keanu, rel_acted_in, matrix);
        txn.set_edge_property(
            keanu,
            rel_acted_in,
            matrix,
            "role".to_string(),
            nervusdb_v2::PropertyValue::String("Neo".to_string()),
        )?;

        txn.commit()?;
        (keanu, matrix)
    };
    // 3. Query (Cypher)
    // We use the `QueryExt` trait to get a "SQLite-like" experience.
    use nervusdb_v2::query::{Params, QueryExt};

    println!("\n🔍 Executing Cypher Query...");

    // "Find the name of people who acted in 'The Matrix' and their role"
    // Note: v2 M3 only supports returning nodes/edges, not properties directly yet.
    // So we RETURN p, r and then look up properties.
    let cypher = "MATCH (p)-[r:1]->(m) WHERE m.title = 'The Matrix' RETURN p, r";

    let snapshot = db.snapshot();
    let rows = snapshot
        .query(cypher, &Params::default())
        .map_err(|e| nervusdb_v2::Error::Io(std::io::Error::other(e.to_string())))?;

    println!("Found {} row(s):", rows.len());
    for (i, row) in rows.iter().enumerate() {
        let p_id = row.get_node("p").unwrap();
        let _r_key = row.get_edge("r").unwrap(); // EdgeKey
                                                 // In v2.1, we will have row.get_string("p.name")
                                                 // For now, we use the snapshot API:
        let name = snapshot
            .node_property(p_id, "name")
            .unwrap_or(nervusdb_v2::query::PropertyValue::Null);
        let role = snapshot
            .edge_property(_r_key, "role")
            .unwrap_or(nervusdb_v2::query::PropertyValue::Null);

        println!("Row {}: Actor={:?}, Role={:?}", i + 1, name, role);
    }

    // 4. Cleanup (optional, tempdir handles it)
    println!("\n✨ Tour completed successfully!");
    Ok(())
}

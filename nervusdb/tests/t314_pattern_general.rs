// T314: Generalize Patterns (multi-hop) - TDD Test Suite
// � GREEN Phase: Tests should PASS now that we implemented iterative compilation
//
// Patterns to verify:
// - 2-hop: (a)-[r1]->(b)-[r2]->(c)
// - 3-hop: (a)-[]->(b)-[]->(c)-[]->(d)

use nervusdb::Db;
use tempfile::tempdir;

#[test]
fn test_two_hop_pattern_compile() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t314_two.ndb");
    let _db = Db::open(&db_path)?;

    // MATCH (a)-[:KNOWS]->(b)-[:KNOWS]->(c)
    let query = "MATCH (a:Person)-[:KNOWS]->(b)-[:KNOWS]->(c) RETURN a.name, c.name";

    // Should succeed now
    let res = nervusdb::query::prepare(query);
    assert!(
        res.is_ok(),
        "2-hop pattern compilation failed: {:?}",
        res.err()
    );

    Ok(())
}

#[test]
fn test_three_hop_pattern_compile() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t314_three.ndb");
    let _db = Db::open(&db_path)?;

    // MATCH (a)->(b)->(c)->(d)
    let query = "MATCH (a)-[:link]->(b)-[:link]->(c)-[:link]->(d) RETURN id(d) AS id_d";
    let res = nervusdb::query::prepare(query);
    assert!(
        res.is_ok(),
        "3-hop pattern compilation failed: {:?}",
        res.err()
    );

    Ok(())
}

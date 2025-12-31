use nervusdb_v2::query::Value;
use nervusdb_v2::{Db, PropertyValue};
use tempfile::tempdir;

#[test]
fn test_type_alternation() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t316.ndb");
    let db = Db::open(&db_path)?;

    // Schema:
    // (A)-[:KNOWS]->(B)
    // (A)-[:LIKES]->(C)
    // (A)-[:HATES]->(D)

    let mut txn = db.begin_write();
    // Use inherent methods as per t315
    let person = txn.get_or_create_label("Person")?;

    // External IDs 1, 2, 3, 4
    let a = txn.create_node(1u64.into(), person)?;
    let b = txn.create_node(2u64.into(), person)?;
    let c = txn.create_node(3u64.into(), person)?;
    let d = txn.create_node(4u64.into(), person)?;

    txn.set_node_property(
        a,
        "name".to_string(),
        PropertyValue::String("A".to_string()),
    )?;
    txn.set_node_property(
        b,
        "name".to_string(),
        PropertyValue::String("B".to_string()),
    )?;
    txn.set_node_property(
        c,
        "name".to_string(),
        PropertyValue::String("C".to_string()),
    )?;
    txn.set_node_property(
        d,
        "name".to_string(),
        PropertyValue::String("D".to_string()),
    )?;

    let knows = txn.get_or_create_rel_type("KNOWS")?;
    let likes = txn.get_or_create_rel_type("LIKES")?;
    let hates = txn.get_or_create_rel_type("HATES")?;

    // create_edge returns () (no Result), so no ?
    txn.create_edge(a, knows, b);
    txn.create_edge(a, likes, c);
    txn.create_edge(a, hates, d);

    txn.commit()?;

    // 1. MATCH (n {name: 'A'})-[:KNOWS|LIKES]->(m)
    // Should find B and C
    let query = "MATCH (n {name: 'A'})-[:KNOWS|LIKES]->(m) RETURN m.name ORDER BY m.name";
    let prep = nervusdb_v2::query::prepare(query)?;
    let snapshot = db.snapshot();
    let res: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb_v2::query::error::Result<Vec<_>>>()?;

    assert_eq!(res.len(), 2, "Should find B and C");
    assert_eq!(
        res[0].get("m.name").unwrap(),
        &Value::String("B".to_string())
    );
    assert_eq!(
        res[1].get("m.name").unwrap(),
        &Value::String("C".to_string())
    );

    // 2. MATCH (n {name: 'A'})-[:KNOWS]->(m)
    // Should find B only
    let query = "MATCH (n {name: 'A'})-[:KNOWS]->(m) RETURN m.name";
    let prep = nervusdb_v2::query::prepare(query)?;
    let res: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb_v2::query::error::Result<Vec<_>>>()?;
    assert_eq!(res.len(), 1);
    assert_eq!(
        res[0].get("m.name").unwrap(),
        &Value::String("B".to_string())
    );

    // 3. MATCH (n {name: 'A'})-[:LOVES|HATES]->(m)
    // LOVES doesn't exist, HATES does -> Should find D only
    let query = "MATCH (n {name: 'A'})-[:LOVES|HATES]->(m) RETURN m.name";
    let prep = nervusdb_v2::query::prepare(query)?;
    let res: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb_v2::query::error::Result<Vec<_>>>()?;
    assert_eq!(res.len(), 1);
    assert_eq!(
        res[0].get("m.name").unwrap(),
        &Value::String("D".to_string())
    );

    // 4. Undirected: MATCH (m)<-[:KNOWS|LIKES]-(n {name: 'A'})
    // n->m is A->B (KNOWS) and A->C (LIKES).
    // So m matches B and C.
    // Note: Directionality logic. if (m)<-...(n), then n is source, m is Dest.
    // In db: a(A)->b(B) [KNOWS].
    // Match (m)<-[KNOWS]-(n {name:'A'}). n=A. m should be B.
    // Correct.
    let query = "MATCH (m)<-[:KNOWS|LIKES]-(n {name: 'A'}) RETURN m.name ORDER BY m.name";
    let prep = nervusdb_v2::query::prepare(query)?;
    let res: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb_v2::query::error::Result<Vec<_>>>()?;
    assert_eq!(res.len(), 2, "Incoming match should find B and C");
    assert_eq!(
        res[0].get("m.name").unwrap(),
        &Value::String("B".to_string())
    );

    Ok(())
}

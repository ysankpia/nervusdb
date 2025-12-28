use nervusdb_core::{Database, Fact, Options};
use tempfile::tempdir;

#[test]
fn execute_query_basic_match_return() {
    let tmp = tempdir().unwrap();
    let mut db = Database::open(Options::new(tmp.path())).unwrap();

    db.add_fact(Fact::new("alice", "knows", "bob")).unwrap();
    db.add_fact(Fact::new("bob", "knows", "carol")).unwrap();

    let rows = db.execute_query("MATCH (a)-[r]->(b) RETURN a, b").unwrap();

    // Expect two rows corresponding to two edges
    assert_eq!(rows.len(), 2);
    // Each row should contain node ids for a and b
    for row in rows {
        assert!(row.contains_key("a"));
        assert!(row.contains_key("b"));
    }
}

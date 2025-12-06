use nervusdb_core::{Database, Fact, Options, QueryCriteria};
use tempfile::tempdir;

#[test]
fn test_persistence() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("persist");

    {
        let mut db = Database::open(Options::new(&db_path)).unwrap();
        db.add_fact(Fact::new("alice", "likes", "rust")).unwrap();
        // db is dropped here, closing redb
    }

    {
        let db = Database::open(Options::new(&db_path)).unwrap();
        let results: Vec<_> = db
            .query(QueryCriteria {
                subject_id: db.resolve_id("alice").unwrap(),
                predicate_id: db.resolve_id("likes").unwrap(),
                object_id: db.resolve_id("rust").unwrap(),
            })
            .collect();
        assert_eq!(results.len(), 1);

        let triple = &results[0];
        assert_eq!(db.resolve_str(triple.subject_id).unwrap().unwrap(), "alice");
    }
}

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
                subject_id: db.dictionary().lookup_id("alice"),
                predicate_id: db.dictionary().lookup_id("likes"),
                object_id: db.dictionary().lookup_id("rust"),
            })
            .collect();
        assert_eq!(results.len(), 1);

        let triple = &results[0];
        assert_eq!(
            db.dictionary().lookup_value(triple.subject_id).unwrap(),
            "alice"
        );
    }
}

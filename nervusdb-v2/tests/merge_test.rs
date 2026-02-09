use nervusdb_v2::Db;
use nervusdb_v2_query::prepare;
use tempfile::TempDir;

#[test]
fn test_simple_merge() {
    let dir = TempDir::new().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();
    let mut txn = db.begin_write();

    // Test simple MERGE without properties
    let query = prepare("MERGE (a) RETURN a").unwrap();

    match query.execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new()) {
        Ok(count) => println!("SUCCESS: MERGE created {} items", count),
        Err(e) => {
            println!("ERROR: {:?}", e);
            panic!("MERGE failed: {:?}", e);
        }
    }
}

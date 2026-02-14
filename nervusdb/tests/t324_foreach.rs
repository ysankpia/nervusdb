use nervusdb::Db;
use nervusdb_query::{Params, Result, Value, prepare};
use tempfile::tempdir;

#[test]
fn t324_basic_create_foreach() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Query: FOREACH (x IN [1, 2, 3] | CREATE (:Node {val: x}))
    let q = prepare("FOREACH (x IN [1, 2, 3] | CREATE (:Node {val: x}))").unwrap();

    {
        let mut txn = db.begin_write();
        let count = q
            .execute_write(&db.snapshot(), &mut txn, &crate::Params::default())
            .unwrap();
        txn.commit().unwrap();
        // FOREACH itself returns the input row (which is empty/default here), but doesn't produce new rows.
        // It outputs 1 row because the input was 1 row (default ReturnOne/Unit).
        // execute_write returns modification count.
        assert_eq!(count, 3);
        // Actually execute_write sums up changes.
        // Our query creates 3 nodes.
        // Does FOREACH propagate count?
        // Yes, if ForeachIter delegates to sub-plan which might be Create/Update.
        // Wait, ForeachIter just drains.
        // And execute_write only counts from Plan::Create/Delete/Set.
        // If Plan::Foreach is top level, execute_write calls execute_plan for inputs?
        // No! execute_write handles Plan::Create directly.
        // But what about Plan::Foreach?
        // execute_write doesn't have a match arm for Plan::Foreach!
        // It relies on execute_plan if it's not a write clause?
        // No, execute_write returns Result<u32>.
        // If I pass Plan::Foreach to execute_write, does it handle it?
        // Let's check executor.rs execute_write match.
    }

    // Verify
    {
        let snapshot = db.snapshot();
        let q2 = prepare("MATCH (n:Node) RETURN count(n) as count, sum(n.val) as sum").unwrap();
        let rows: Vec<_> = q2
            .execute_streaming(&snapshot, &Params::new())
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert!(matches!(
            rows[0].get("count"),
            Some(Value::Int(3)) | Some(Value::Float(3.0))
        ));
        assert!(matches!(
            rows[0].get("sum"),
            Some(Value::Int(6)) | Some(Value::Float(6.0))
        )); // 1+2+3
    }
}

#[test]
fn t324_scoped_update() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Setup: Create a node
    {
        let mut txn = db.begin_write();
        prepare("CREATE (:User {id: 1, tags: []})")
            .unwrap()
            .execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    // Update with FOREACH using a parameter list
    let q = prepare(
        "MATCH (u:User) \
         FOREACH (tag IN ['A', 'B'] | SET u.last_tag = tag)", // Note: Real usage often involves append, but M3 might not have list append in SET easily yet,
                                                              // so let's stick to overwriting property to prove iteration happens.
                                                              // The last one 'B' should win.
    )
    .unwrap();

    {
        let mut txn = db.begin_write();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    {
        let snapshot = db.snapshot();
        let rows: Vec<_> = prepare("MATCH (u:User) RETURN u.last_tag as tag")
            .unwrap()
            .execute_streaming(&snapshot, &Params::new())
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(rows[0].get("tag"), Some(&Value::String("B".into())));
    }
}

#[test]
fn t324_nested_foreach() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // FOREACH (x IN [1, 10] | FOREACH (y IN [2, 3] | CREATE (:R {v: x+y})))
    // x=1: y=2(3), y=3(4)
    // x=10: y=2(12), y=3(13)
    // Total 4 nodes: 3, 4, 12, 13
    let q = prepare(
        "FOREACH (x IN [1, 10] | \
             FOREACH (y IN [2, 3] | \
                 CREATE (:R {v: x + y}) \
             ) \
         )",
    )
    .unwrap();

    {
        let mut txn = db.begin_write();
        let count = q
            .execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
        assert_eq!(count, 4);
    }

    {
        let snapshot = db.snapshot();
        let rows: Vec<_> = prepare("MATCH (n:R) RETURN sum(n.v) as total")
            .unwrap()
            .execute_streaming(&snapshot, &Params::new())
            .collect::<Result<Vec<_>>>()
            .unwrap();
        // 3+4+12+13 = 32
        assert!(matches!(
            rows[0].get("total"),
            Some(Value::Int(32)) | Some(Value::Float(32.0))
        ));
    }
}

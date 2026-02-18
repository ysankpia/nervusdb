use nervusdb::Db;
use nervusdb_query::{Params, Result, Row, Value, prepare};
use tempfile::tempdir;

fn collect_rows(db: &Db, cypher: &str) -> Vec<Row> {
    let q = prepare(cypher).unwrap();
    q.execute_streaming(&db.snapshot(), &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap()
}

fn cell_i64(row: &Row, key: &str) -> i64 {
    match row.get(key) {
        Some(Value::Int(v)) => *v,
        other => panic!("expected Int at {key}, got {other:?}"),
    }
}

#[test]
fn t342_match_single_label_subset_on_multi_label_node() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let mut txn = db.begin_write();
        let create = prepare("CREATE (n:Person:Employee:Manager {name: 'Carol'})").unwrap();
        create
            .execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let rows = collect_rows(&db, "MATCH (n:Manager) RETURN count(n) AS c");
    assert_eq!(rows.len(), 1);
    assert_eq!(cell_i64(&rows[0], "c"), 1);
}

#[test]
fn t342_merge_execute_write_respects_bound_nodes_for_relationship() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let mut txn = db.begin_write();
        let seed = prepare("CREATE (:MA {id: 1}), (:MB {id: 2})").unwrap();
        seed.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let merge = prepare("MATCH (a:MA), (b:MB) MERGE (a)-[:LINK]->(b)").unwrap();

    {
        let mut txn = db.begin_write();
        let created = merge
            .execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
        assert_eq!(
            created, 1,
            "first MERGE should create exactly one relationship"
        );
    }

    {
        let mut txn = db.begin_write();
        let created = merge
            .execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
        assert_eq!(created, 0, "second MERGE should be idempotent");
    }

    let rows = collect_rows(&db, "MATCH (a:MA)-[r:LINK]->(b:MB) RETURN count(r) AS c");
    assert_eq!(rows.len(), 1);
    assert_eq!(cell_i64(&rows[0], "c"), 1);
}

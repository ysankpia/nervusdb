use nervusdb::{Db, GraphSnapshot, PropertyValue};
use nervusdb_query::{Params, Value, query_collect};
use tempfile::tempdir;

#[test]
fn regression_smoke_10k_nodes_50k_edges() {
    let nodes = 10_000u64;
    let edges = 50_000u64;
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("s.ndb");
    let wal = ndb.with_extension("wal");

    {
        let db = Db::open_paths(&ndb, &wal).unwrap();
        let mut txn = db.begin_write();
        let p = txn.get_or_create_label("Person").unwrap();
        let k = txn.get_or_create_rel_type("KNOWS").unwrap();

        let mut iids = Vec::with_capacity(nodes as usize);
        for i in 1..=nodes {
            let iid = txn.create_node(i, p).unwrap();
            iids.push(iid);
        }
        for i in 0..edges {
            let src = iids[(i % nodes) as usize];
            let dst = iids[((i + 1) % nodes) as usize];
            txn.create_edge(src, k, dst);
        }
        txn.commit().unwrap();
    }

    let db = Db::open_paths(&ndb, &wal).unwrap();
    let rows = query_collect(
        &db.snapshot(),
        "MATCH (n) RETURN count(*) AS cnt",
        &Params::new(),
    )
    .unwrap();
    assert_eq!(rows[0].columns()[0].1, Value::Int(nodes as i64));

    let rows = query_collect(
        &db.snapshot(),
        "MATCH ()-[r:KNOWS]->() RETURN count(*) AS cnt",
        &Params::new(),
    )
    .unwrap();
    assert_eq!(rows[0].columns()[0].1, Value::Int(edges as i64));
}

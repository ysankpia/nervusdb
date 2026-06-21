use nervusdb::{Db, GraphSnapshot};
use tempfile::tempdir;

#[test]
fn regression_smoke_10k_nodes_50k_edges() {
    let nodes = 10_000u64;
    let edges = 50_000u64;
    let dir = tempdir().unwrap();
    let path = dir.path().join("large");
    let rel;
    let label;

    {
        let db = Db::open(&path).unwrap();
        let mut txn = db.begin_write();
        label = txn.get_or_create_label("Person").unwrap();
        rel = txn.get_or_create_rel_type("KNOWS").unwrap();

        let mut iids = Vec::with_capacity(nodes as usize);
        for i in 1..=nodes {
            let iid = txn.create_node(i, label).unwrap();
            iids.push(iid);
        }
        for i in 0..edges {
            let src_idx = i % nodes;
            let cycle = i / nodes;
            let dst_idx = (src_idx + cycle + 1) % nodes;
            let src = iids[src_idx as usize];
            let dst = iids[dst_idx as usize];
            txn.create_edge(src, rel, dst).unwrap();
        }
        txn.commit().unwrap();
    }

    let db = Db::open(&path).unwrap();
    let snapshot = db.snapshot();
    assert_eq!(snapshot.node_count(Some(label)), nodes);
    assert_eq!(snapshot.edge_count(Some(rel)), edges);
}

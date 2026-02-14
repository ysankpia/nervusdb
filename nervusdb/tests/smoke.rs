use nervusdb::Db;
use tempfile::tempdir;

#[test]
fn t46_smoke_create_and_traverse() {
    let dir = tempdir().unwrap();
    let base = dir.path().join("graph");

    let db = Db::open(&base).unwrap();
    let (a, b) = {
        let mut tx = db.begin_write();
        let a = tx.create_node(10, 1).unwrap();
        let b = tx.create_node(20, 1).unwrap();
        tx.create_edge(a, 7, b);
        tx.commit().unwrap();
        (a, b)
    };

    let r = db.begin_read();
    let edges: Vec<_> = r.neighbors(a, Some(7)).collect();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].dst, b);
}

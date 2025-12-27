//! T62: ORDER BY / SKIP / LIMIT Tests

use nervusdb_v2::Db;
use nervusdb_v2_api::{EdgeKey, GraphSnapshot, InternalNodeId, RelTypeId};
use nervusdb_v2_query::facade::query_collect;
use tempfile::tempdir;

struct DbSnapshot<'a> {
    db: &'a Db,
}

impl<'a> GraphSnapshot for DbSnapshot<'a> {
    type Neighbors<'b>
        = Box<dyn Iterator<Item = EdgeKey> + 'b>
    where
        Self: 'b;
    fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> Self::Neighbors<'_> {
        let snapshot = self.db.begin_read();
        let edges: Vec<EdgeKey> = snapshot
            .neighbors(src, rel)
            .map(|e| EdgeKey {
                src: e.src,
                rel: e.rel,
                dst: e.dst,
            })
            .collect();
        Box::new(edges.into_iter())
    }
    fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        let snapshot = self.db.begin_read();
        let mut nodes = Vec::new();
        for i in 0..100u32 {
            let neighbors: Vec<_> = snapshot.neighbors(i, None).collect();
            if !neighbors.is_empty() {
                nodes.push(i);
            }
        }
        Box::new(nodes.into_iter())
    }
    fn is_tombstoned_node(&self, _iid: InternalNodeId) -> bool {
        false
    }
}

fn get_snapshot(db: &Db) -> impl GraphSnapshot + '_ {
    DbSnapshot { db }
}

#[test]
fn test_order_by_asc() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(3, 0).unwrap();
        let c = txn.create_node(2, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.create_edge(a, 1, c);
        txn.commit().unwrap();
    }
    let snapshot = get_snapshot(&db);
    let rows = query_collect(
        &snapshot,
        "MATCH (n)-[:1]->(m) RETURN m ORDER BY m",
        &Default::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 2);
}

#[test]
fn test_order_by_desc() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(3, 0).unwrap();
        let c = txn.create_node(2, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.create_edge(a, 1, c);
        txn.commit().unwrap();
    }
    let snapshot = get_snapshot(&db);
    let rows = query_collect(
        &snapshot,
        "MATCH (n)-[:1]->(m) RETURN m ORDER BY m DESC",
        &Default::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 2);
}

#[test]
fn test_skip() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        let c = txn.create_node(3, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.create_edge(a, 1, c);
        txn.commit().unwrap();
    }
    let snapshot = get_snapshot(&db);
    let rows = query_collect(
        &snapshot,
        "MATCH (n)-[:1]->(m) RETURN m SKIP 1",
        &Default::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn test_limit() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        for i in 100u32..110u32 {
            let b = txn.create_node(i.into(), 0).unwrap();
            txn.create_edge(a, 1, b);
        }
        txn.commit().unwrap();
    }
    let snapshot = get_snapshot(&db);
    let rows = query_collect(
        &snapshot,
        "MATCH (n)-[:1]->(m) RETURN m LIMIT 5",
        &Default::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 5);
}

#[test]
fn test_distinct() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.create_edge(a, 1, b);
        txn.commit().unwrap();
    }
    let snapshot = get_snapshot(&db);
    let rows = query_collect(
        &snapshot,
        "MATCH (n)-[:1]->(m) RETURN DISTINCT m",
        &Default::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn test_order_by_skip_limit_combined() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        // Create nodes 200, 201, 202, ..., 209 (10 nodes)
        for i in 200u32..210u32 {
            let b = txn.create_node(i.into(), 0).unwrap();
            txn.create_edge(a, 1, b);
        }
        txn.commit().unwrap();
    }
    let snapshot = get_snapshot(&db);
    // 10 edges, ORDER BY m (ascending), SKIP 2, LIMIT 3 -> should return 3 rows (203, 204, 205)
    let rows = query_collect(
        &snapshot,
        "MATCH (n)-[:1]->(m) RETURN m ORDER BY m SKIP 2 LIMIT 3",
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        rows.len(),
        3,
        "Expected 3 rows after SKIP 2 LIMIT 3 on 10 rows"
    );
}

//! T61: Aggregation Tests

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
fn test_simple_match_return() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.commit().unwrap();
    }
    let snapshot = get_snapshot(&db);
    let rows = query_collect(
        &snapshot,
        "MATCH (n)-[:1]->(m) RETURN n, m",
        &Default::default(),
    )
    .unwrap();
    assert!(!rows.is_empty());
}

#[test]
fn test_list_literal() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.commit().unwrap();
    }
    let snapshot = get_snapshot(&db);
    let rows = query_collect(
        &snapshot,
        "MATCH (n)-[:1]->(m) RETURN [1, 2, 3]",
        &Default::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn test_sum_function() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.commit().unwrap();
    }
    let snapshot = get_snapshot(&db);
    let rows = query_collect(
        &snapshot,
        "MATCH (n)-[:1]->(m) RETURN SUM([10, 20, 30])",
        &Default::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn test_avg_function() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.commit().unwrap();
    }
    let snapshot = get_snapshot(&db);
    let rows = query_collect(
        &snapshot,
        "MATCH (n)-[:1]->(m) RETURN AVG([100, 200])",
        &Default::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn test_empty_list() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.commit().unwrap();
    }
    let snapshot = get_snapshot(&db);
    let rows = query_collect(
        &snapshot,
        "MATCH (n)-[:1]->(m) RETURN []",
        &Default::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn test_mixed_functions() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.commit().unwrap();
    }
    let snapshot = get_snapshot(&db);
    let rows = query_collect(
        &snapshot,
        "MATCH (n)-[:1]->(m) RETURN COUNT([1]), SUM([1]), AVG([1])",
        &Default::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns().len(), 3);
}

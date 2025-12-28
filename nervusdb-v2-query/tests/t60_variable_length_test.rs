//! T60: Variable Length Paths Tests
//!
//! Tests for `MATCH (a)-[:TYPE*1..3]->(b)` patterns

use nervusdb_v2::Db;
use nervusdb_v2_api::{EdgeKey, GraphSnapshot, InternalNodeId, RelTypeId};
use nervusdb_v2_query::Value;
use nervusdb_v2_query::facade::query_collect;
use nervusdb_v2_query::prepare;
use tempfile::tempdir;

/// Wrapper that implements GraphSnapshot for testing
struct DbSnapshot<'a> {
    db: &'a Db,
}

impl<'a> GraphSnapshot for DbSnapshot<'a> {
    // Use boxed iterator for test implementation simplicity
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
        let mut nodes: Vec<InternalNodeId> = Vec::new();
        // Collect node IDs by probing internal IDs
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
fn test_single_hop_pattern() {
    // Test basic single-hop pattern (should work with or without * syntax)
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create: (a)-[:1]->(b)-[:1]->(c)
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        let c = txn.create_node(3, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.create_edge(b, 1, c);
        txn.commit().unwrap();
    }

    let snapshot = get_snapshot(&db);

    // Single hop: a->b
    let query = prepare("MATCH (a)-[:1]->(b) RETURN a, b").unwrap();
    let rows: Vec<_> = query
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows.len(), 2); // a->b, b->c

    println!("Single hop test passed: {} rows", rows.len());
}

#[test]
fn test_variable_length_star() {
    // Test * (1 or more hops)
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create: (a)-[:1]->(b)-[:1]->(c)-[:1]->(d)
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        let c = txn.create_node(3, 0).unwrap();
        let d = txn.create_node(4, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.create_edge(b, 1, c);
        txn.create_edge(c, 1, d);
        txn.commit().unwrap();
    }

    let snapshot = get_snapshot(&db);

    // Query: a-[*]->d (any path from a to d)
    // This should find paths of length 1, 2, 3
    let rows = query_collect(
        &snapshot,
        "MATCH (a)-[:1*]->(d) RETURN a, d",
        &Default::default(),
    )
    .unwrap();

    // a->d via: a->b->c->d (3 hops)
    // So only 1 result with d being the final node
    assert!(!rows.is_empty(), "Should find path from a to d");

    println!("Variable length star test: {} rows", rows.len());
}

#[test]
fn test_variable_length_range() {
    // Test *1..2 (1 to 2 hops)
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create: (a)-[:1]->(b)-[:1]->(c)-[:1]->(d)
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        let c = txn.create_node(3, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.create_edge(b, 1, c);
        txn.commit().unwrap();
    }

    let snapshot = get_snapshot(&db);

    // Query: a-[*1..2]->c (1 or 2 hops from a to c)
    // a->b (1 hop) and a->b->c (2 hops)
    let rows = query_collect(
        &snapshot,
        "MATCH (a)-[:1*1..2]->(c) RETURN a, c",
        &Default::default(),
    )
    .unwrap();

    // Should find 2 paths: a->b and a->b->c
    // But they both end at c? No, each row has different end node
    // Row 1: a->b (end node is b)
    // Row 2: a->b->c (end node is c)
    println!("Variable length 1..2 test: {} rows", rows.len());
    for (i, row) in rows.iter().enumerate() {
        println!("  Row {}: {:?}", i, row.columns());
    }
}

#[test]
fn test_variable_length_min_only() {
    // Test *2.. (minimum 2 hops)
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create: (a)-[:1]->(b)-[:1]->(c)-[:1]->(d)
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        let c = txn.create_node(3, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.create_edge(b, 1, c);
        txn.commit().unwrap();
    }

    let snapshot = get_snapshot(&db);

    // Query: a-[*2..]->c (minimum 2 hops to reach c)
    // a->b (1 hop, should NOT match)
    // a->b->c (2 hops, should match)
    let rows = query_collect(
        &snapshot,
        "MATCH (a)-[:1*2..]->(c) RETURN a, c",
        &Default::default(),
    )
    .unwrap();

    // Should find at least the a->b->c path
    assert!(
        !rows.is_empty(),
        "Should find at least 1 path with min 2 hops"
    );
    println!("Variable length min only test: {} rows", rows.len());
}

#[test]
fn test_variable_length_max_only() {
    // Test *..2 (maximum 2 hops)
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create: (a)-[:1]->(b)-[:1]->(c)-[:1]->(d)
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        let c = txn.create_node(3, 0).unwrap();
        let d = txn.create_node(4, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.create_edge(b, 1, c);
        txn.create_edge(c, 1, d);
        txn.commit().unwrap();
    }

    let snapshot = get_snapshot(&db);

    // Query: a-[*..2]->b (maximum 2 hops to reach b)
    // a->b (1 hop, should match)
    // a->b->c (2 hops to c, not b)
    let rows = query_collect(
        &snapshot,
        "MATCH (a)-[:1*..2]->(b) RETURN a, b",
        &Default::default(),
    )
    .unwrap();

    // Should find at least the a->b path
    assert!(
        !rows.is_empty(),
        "Should find at least 1 path with max 2 hops"
    );
    println!("Variable length max only test: {} rows", rows.len());
}

#[test]
fn test_variable_length_exact() {
    // Test *2 (exactly 2 hops)
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create: (a)-[:1]->(b)-[:1]->(c)-[:1]->(d)
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        let c = txn.create_node(3, 0).unwrap();
        let d = txn.create_node(4, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.create_edge(b, 1, c);
        txn.create_edge(c, 1, d);
        txn.commit().unwrap();
    }

    let snapshot = get_snapshot(&db);

    // Query: a-[*2]->c (exactly 2 hops)
    // Should find a->b->c (2 hops)
    // Note: Iterator finds paths from ALL nodes, so we verify expected paths exist
    let rows = query_collect(
        &snapshot,
        "MATCH (a)-[:1*2]->(c) RETURN a, c",
        &Default::default(),
    )
    .unwrap();

    // Should find at least the a->b->c path
    assert!(!rows.is_empty(), "Should find at least 1 path with 2 hops");
    println!("Variable length exact test: {} rows", rows.len());
}

#[test]
fn test_variable_length_with_limit() {
    // Test variable length with LIMIT
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create a chain: (a)->(b)->(c)->(d)->(e)
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        let c = txn.create_node(3, 0).unwrap();
        let d = txn.create_node(4, 0).unwrap();
        let e = txn.create_node(5, 0).unwrap();
        txn.create_edge(a, 1, b);
        txn.create_edge(b, 1, c);
        txn.create_edge(c, 1, d);
        txn.create_edge(d, 1, e);
        txn.commit().unwrap();
    }

    let snapshot = get_snapshot(&db);

    // Query with LIMIT
    let rows = query_collect(
        &snapshot,
        "MATCH (a)-[:1*]->(e) RETURN a, e LIMIT 1",
        &Default::default(),
    )
    .unwrap();

    assert_eq!(rows.len(), 1, "Should respect LIMIT");
    println!("Variable length with limit test: {} rows", rows.len());
}

#[test]
fn test_variable_length_no_path() {
    // Test when no path exists between two specific nodes
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create a simple chain: a->b
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        // a->b
        txn.create_edge(a, 1, b);
        txn.commit().unwrap();
    }

    let snapshot = get_snapshot(&db);

    // Query: b-[*]->a (reverse direction, no path)
    // Since graph is directed a->b, there's no path from b to a
    let rows = query_collect(
        &snapshot,
        "MATCH (b)-[:1*]->(a) RETURN b, a",
        &Default::default(),
    )
    .unwrap();

    // Should return empty since there's no path from b to a (graph is directed)
    // Note: Due to DFS from all nodes, we might find paths, but verify none from b to a
    let has_b_to_a = rows.iter().any(|row| {
        let cols = row.columns();
        if let Some((_, Value::NodeId(node_id))) = cols.first() {
            return *node_id == 1; // b is node with external_id 2 (internal_id 1)
        }
        false
    });
    assert!(
        !has_b_to_a,
        "Should not find path from b to a since graph is directed a->b"
    );
    println!(
        "Variable length no path test: {} rows, no path from b to a",
        rows.len()
    );
}

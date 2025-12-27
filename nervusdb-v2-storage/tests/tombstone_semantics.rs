//! Tombstone semantics tests for v2 storage
//!
//! Tests that verify tombstone behavior:
//! - Tombstoned nodes are not visible in neighbors()
//! - Tombstoned edges are not visible in neighbors()
//! - Compaction properly removes tombstoned data (where implemented)

use nervusdb_v2_api::{GraphSnapshot, GraphStore, PropertyValue as ApiPropertyValue};
use nervusdb_v2_storage::engine::GraphEngine;
use nervusdb_v2_storage::property::PropertyValue;
use tempfile::tempdir;

#[test]
fn t53_tombstoned_node_not_in_neighbors() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    let (a, b) = {
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, 1).unwrap();
        let b = tx.create_node(20, 1).unwrap();
        tx.create_edge(a, 1, b);
        tx.commit().unwrap();
        (a, b)
    };

    // Before tombstone: edge visible
    {
        let snap = engine.begin_read();
        let edges: Vec<_> = snap.neighbors(a, Some(1)).collect();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].dst, b);
    }

    // Tombstone node a
    {
        let mut tx = engine.begin_write();
        tx.tombstone_node(a);
        tx.commit().unwrap();
    }

    // After tombstone: no edges visible
    {
        let snap = engine.begin_read();
        let edges: Vec<_> = snap.neighbors(a, Some(1)).collect();
        assert!(
            edges.is_empty(),
            "tombstoned node should have no outgoing edges"
        );
    }
}

#[test]
fn t54_tombstoned_edge_not_in_neighbors() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    let (a, b) = {
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, 1).unwrap();
        let b = tx.create_node(20, 1).unwrap();
        tx.create_edge(a, 1, b);
        tx.commit().unwrap();
        (a, b)
    };

    // Before tombstone: edge visible
    {
        let snap = engine.begin_read();
        let edges: Vec<_> = snap.neighbors(a, Some(1)).collect();
        assert_eq!(edges.len(), 1);
    }

    // Tombstone edge
    {
        let mut tx = engine.begin_write();
        tx.tombstone_edge(a, 1, b);
        tx.commit().unwrap();
    }

    // After tombstone: edge not visible
    {
        let snap = engine.begin_read();
        let edges: Vec<_> = snap.neighbors(a, Some(1)).collect();
        assert!(edges.is_empty(), "tombstoned edge should not be visible");
    }
}

#[test]
fn t55_tombstoned_node_is_tombstoned() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    let node = {
        let mut tx = engine.begin_write();
        let node = tx.create_node(10, 1).unwrap();
        tx.commit().unwrap();
        node
    };

    let snap = engine.snapshot();
    assert!(!snap.is_tombstoned_node(node));

    // Tombstone node
    {
        let mut tx = engine.begin_write();
        tx.tombstone_node(node);
        tx.commit().unwrap();
    }

    let snap = engine.snapshot();
    assert!(
        snap.is_tombstoned_node(node),
        "tombstoned node should be detected"
    );
}

#[test]
fn t56_tombstoned_node_not_in_nodes_iter() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    let node = {
        let mut tx = engine.begin_write();
        let node = tx.create_node(10, 1).unwrap();
        tx.commit().unwrap();
        node
    };

    // Before tombstone: node visible
    {
        let snap = engine.snapshot();
        let nodes: Vec<_> = snap.nodes().collect();
        assert!(nodes.contains(&node));
    }

    // Tombstone node
    {
        let mut tx = engine.begin_write();
        tx.tombstone_node(node);
        tx.commit().unwrap();
    }

    // After tombstone: node not in nodes() iterator
    {
        let snap = engine.snapshot();
        let nodes: Vec<_> = snap.nodes().collect();
        assert!(
            !nodes.contains(&node),
            "tombstoned node should not appear in nodes()"
        );
    }
}

#[test]
fn t57_crash_recovery_preserves_committed() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    {
        let engine = GraphEngine::open(&ndb, &wal).unwrap();
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, 1).unwrap();
        let b = tx.create_node(20, 1).unwrap();
        tx.create_edge(a, 1, b);
        tx.set_node_property(
            a,
            "name".to_string(),
            PropertyValue::String("Bob".to_string()),
        );
        tx.commit().unwrap();
    }

    // Simulate crash and reopen - committed data should persist
    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    let a = engine.lookup_internal_id(10).unwrap();
    let b = engine.lookup_internal_id(20).unwrap();

    let snap = engine.snapshot();
    let edges: Vec<_> = snap.neighbors(a, Some(1)).collect();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].dst, b);
    assert_eq!(
        snap.node_property(a, "name"),
        Some(ApiPropertyValue::String("Bob".to_string()))
    );
}

#[test]
fn t58_crash_recovery_rolls_back_uncommitted() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    {
        let engine = GraphEngine::open(&ndb, &wal).unwrap();
        let mut tx = engine.begin_write();
        let node = tx.create_node(10, 1).unwrap();
        tx.set_node_property(
            node,
            "name".to_string(),
            PropertyValue::String("Temp".to_string()),
        );
        // No commit - simulates crash
    }

    // Reopen - uncommitted data should be gone
    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    assert!(engine.lookup_internal_id(10).is_none());
}

#[test]
fn t59_compaction_preserves_live_edges() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    let (a, b, c) = {
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, 1).unwrap();
        let b = tx.create_node(20, 1).unwrap();
        let c = tx.create_node(30, 1).unwrap();
        tx.create_edge(a, 1, b);
        tx.commit().unwrap();
        (a, b, c)
    };

    // Add more edges
    {
        let mut tx = engine.begin_write();
        tx.create_edge(a, 1, c);
        tx.commit().unwrap();
    }

    // Tombstone node b
    {
        let mut tx = engine.begin_write();
        tx.tombstone_node(b);
        tx.commit().unwrap();
    }

    // Compact
    engine.compact().unwrap();

    // a -> c edge should still exist (live data preserved)
    {
        let snap = engine.begin_read();
        let edges: Vec<_> = snap.neighbors(a, Some(1)).collect();
        assert_eq!(
            edges.len(),
            1,
            "live edge a->c should be preserved after compaction"
        );
        assert_eq!(edges[0].dst, c);
    }
}

#[test]
fn t60_properties_persist_across_transactions() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    let node = {
        let mut tx = engine.begin_write();
        let node = tx.create_node(10, 1).unwrap();
        tx.set_node_property(
            node,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        );
        tx.commit().unwrap();
        node
    };

    // Update property in new transaction
    {
        let mut tx = engine.begin_write();
        tx.set_node_property(node, "age".to_string(), PropertyValue::Int(30));
        tx.commit().unwrap();
    }

    // Both properties should be accessible
    let snap = engine.snapshot();
    assert_eq!(
        snap.node_property(node, "name"),
        Some(ApiPropertyValue::String("Alice".to_string()))
    );
    assert_eq!(
        snap.node_property(node, "age"),
        Some(ApiPropertyValue::Int(30))
    );
}

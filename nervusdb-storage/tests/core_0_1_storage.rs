use nervusdb_api::{EdgeKey, GraphSnapshot, GraphStore, PropertyValue};
use nervusdb_storage::engine::GraphEngine;
use nervusdb_storage::{Error, STORAGE_FORMAT_EPOCH};
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use tempfile::tempdir;

fn core_paths(dir: &tempfile::TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    (dir.path().join("core.ndb"), dir.path().join("core.wal"))
}

fn assert_core_edge(engine: &GraphEngine, src_ext: u64, rel: u32, dst_ext: u64) {
    let src = engine
        .lookup_internal_id(src_ext)
        .expect("source node should survive reopen");
    let dst = engine
        .lookup_internal_id(dst_ext)
        .expect("destination node should survive reopen");
    let snapshot = engine.begin_read();
    let edges: Vec<_> = snapshot.neighbors(src, Some(rel)).collect();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].src, src);
    assert_eq!(edges[0].rel, rel);
    assert_eq!(edges[0].dst, dst);
}

#[test]
fn core_0_1_committed_graph_survives_reopen() {
    let dir = tempdir().unwrap();
    let (ndb, wal) = core_paths(&dir);

    {
        let engine = GraphEngine::open(&ndb, &wal).unwrap();
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, 1).unwrap();
        let b = tx.create_node(20, 1).unwrap();
        tx.create_edge(a, 7, b);
        tx.commit().unwrap();
    }

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    assert_core_edge(&engine, 10, 7, 20);

    let b = engine.lookup_internal_id(20).unwrap();
    let snapshot = engine.begin_read();
    assert!(
        snapshot.neighbors(b, Some(7)).next().is_none(),
        "directed edge must not appear in reverse traversal"
    );
}

#[test]
fn core_0_1_uncommitted_graph_is_not_visible_after_reopen() {
    let dir = tempdir().unwrap();
    let (ndb, wal) = core_paths(&dir);

    {
        let engine = GraphEngine::open(&ndb, &wal).unwrap();
        let person = engine.get_or_create_label("Person").unwrap();
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, person).unwrap();
        let b = tx.create_node(20, person).unwrap();
        tx.create_edge(a, 7, b);
        tx.set_node_property(
            a,
            "name".to_string(),
            nervusdb_storage::property::PropertyValue::String("Alice".to_string()),
        );
        tx.set_edge_property(
            a,
            7,
            b,
            "since".to_string(),
            nervusdb_storage::property::PropertyValue::Int(2024),
        );
    }

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    assert!(engine.lookup_internal_id(10).is_none());
    assert!(engine.lookup_internal_id(20).is_none());
    let person = engine.get_label_id("Person").unwrap();
    let snapshot = engine.snapshot();
    assert!(snapshot.nodes().next().is_none());
    assert_eq!(snapshot.node_count(Some(person)), 0);
}

#[test]
fn core_0_1_labels_and_rel_types_survive_reopen() {
    let dir = tempdir().unwrap();
    let (ndb, wal) = core_paths(&dir);

    let person;
    let knows;
    {
        let engine = GraphEngine::open(&ndb, &wal).unwrap();
        person = engine.get_or_create_label("Person").unwrap();
        knows = engine.get_or_create_label("KNOWS").unwrap();

        let mut tx = engine.begin_write();
        let a = tx.create_node(10, person).unwrap();
        let b = tx.create_node(20, person).unwrap();
        tx.create_edge(a, knows, b);
        tx.commit().unwrap();
    }

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    assert_eq!(engine.get_label_id("Person"), Some(person));
    assert_eq!(engine.get_label_name(person).as_deref(), Some("Person"));
    assert_eq!(engine.get_label_id("KNOWS"), Some(knows));
    assert_eq!(engine.get_label_name(knows).as_deref(), Some("KNOWS"));
    assert_core_edge(&engine, 10, knows, 20);
}

#[test]
fn core_0_1_node_and_edge_properties_survive_reopen() {
    let dir = tempdir().unwrap();
    let (ndb, wal) = core_paths(&dir);

    let edge_key;
    {
        let engine = GraphEngine::open(&ndb, &wal).unwrap();
        let person = engine.get_or_create_label("Person").unwrap();
        let knows = engine.get_or_create_label("KNOWS").unwrap();
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, person).unwrap();
        let b = tx.create_node(20, person).unwrap();
        tx.create_edge(a, knows, b);
        tx.set_node_property(
            a,
            "name".to_string(),
            nervusdb_storage::property::PropertyValue::String("Alice".to_string()),
        );
        tx.set_node_property(
            a,
            "age".to_string(),
            nervusdb_storage::property::PropertyValue::Int(30),
        );
        tx.set_edge_property(
            a,
            knows,
            b,
            "since".to_string(),
            nervusdb_storage::property::PropertyValue::Int(2024),
        );
        tx.commit().unwrap();
        edge_key = EdgeKey {
            src: a,
            rel: knows,
            dst: b,
        };
    }

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    let a = engine.lookup_internal_id(10).unwrap();
    let snapshot = engine.snapshot();
    assert_eq!(
        snapshot.node_property(a, "name"),
        Some(PropertyValue::String("Alice".to_string()))
    );
    assert_eq!(
        snapshot.node_property(a, "age"),
        Some(PropertyValue::Int(30))
    );
    assert_eq!(
        snapshot.edge_property(edge_key, "since"),
        Some(PropertyValue::Int(2024))
    );
}

#[test]
fn core_0_1_storage_format_epoch_mismatch_fails_fast() {
    let dir = tempdir().unwrap();
    let (ndb, wal) = core_paths(&dir);

    {
        let _engine = GraphEngine::open(&ndb, &wal).unwrap();
    }

    {
        let mut file = OpenOptions::new().write(true).open(&ndb).unwrap();
        // White-box guard for the current pager meta layout. The epoch lives at
        // bytes 84..92 of page 0 and must stay synchronized with storage docs.
        file.seek(SeekFrom::Start(84)).unwrap();
        file.write_all(&(STORAGE_FORMAT_EPOCH + 1).to_le_bytes())
            .unwrap();
        file.flush().unwrap();
    }

    let err = GraphEngine::open(&ndb, &wal).unwrap_err();
    assert!(
        matches!(err, Error::StorageFormatMismatch { .. }),
        "expected storage format mismatch, got {err:?}"
    );
}

#[test]
fn core_0_1_wal_replay_is_repeatable_across_reopen() {
    let dir = tempdir().unwrap();
    let (ndb, wal) = core_paths(&dir);

    {
        let engine = GraphEngine::open(&ndb, &wal).unwrap();
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, 1).unwrap();
        let b = tx.create_node(20, 1).unwrap();
        tx.create_edge(a, 7, b);
        tx.set_node_property(
            a,
            "name".to_string(),
            nervusdb_storage::property::PropertyValue::String("Alice".to_string()),
        );
        tx.commit().unwrap();
    }

    for _ in 0..3 {
        let engine = GraphEngine::open(&ndb, &wal).unwrap();
        assert_core_edge(&engine, 10, 7, 20);
        let a = engine.lookup_internal_id(10).unwrap();
        assert_eq!(
            engine.snapshot().node_property(a, "name"),
            Some(PropertyValue::String("Alice".to_string()))
        );
    }
}

use fjall::{Database, KeyspaceCreateOptions, PersistMode};
use nervusdb_api::{EdgeKey, GraphSnapshot, PropertyValue};
use nervusdb_storage::engine::GraphEngine;
use nervusdb_storage::{Error, STORAGE_FORMAT_EPOCH};
use tempfile::tempdir;

fn db_dir(dir: &tempfile::TempDir) -> std::path::PathBuf {
    dir.path().join("core-db")
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
    assert_eq!(edges[0], EdgeKey { src, rel, dst });

    let incoming: Vec<_> = snapshot.incoming_neighbors(dst, Some(rel)).collect();
    assert_eq!(incoming, edges);
}

#[test]
fn core_0_1_committed_graph_survives_reopen() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);

    let rel;
    {
        let engine = GraphEngine::open(&path).unwrap();
        let person = engine.get_or_create_label("Person").unwrap();
        rel = engine.get_or_create_rel_type("KNOWS").unwrap();
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, person).unwrap();
        let b = tx.create_node(20, person).unwrap();
        tx.create_edge(a, rel, b);
        tx.commit().unwrap();
    }

    let engine = GraphEngine::open(&path).unwrap();
    assert_eq!(engine.storage_dir(), path.as_path());
    assert_core_edge(&engine, 10, rel, 20);

    let src = engine.lookup_internal_id(10).unwrap();
    let dst = engine.lookup_internal_id(20).unwrap();
    let snapshot = engine.begin_read();
    assert!(
        snapshot.neighbors(dst, Some(rel)).next().is_none(),
        "directed edge must not appear in reverse outgoing traversal"
    );
    assert!(
        snapshot.incoming_neighbors(src, Some(rel)).next().is_none(),
        "directed edge must not appear in reverse incoming traversal"
    );
}

#[test]
fn core_0_1_uncommitted_graph_is_not_visible_after_reopen() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);

    {
        let engine = GraphEngine::open(&path).unwrap();
        let person = engine.get_or_create_label("Person").unwrap();
        let knows = engine.get_or_create_rel_type("KNOWS").unwrap();
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, person).unwrap();
        let b = tx.create_node(20, person).unwrap();
        tx.create_edge(a, knows, b);
        tx.set_node_property(
            a,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        );
        tx.set_edge_property(a, knows, b, "since".to_string(), PropertyValue::Int(2024));
    }

    let engine = GraphEngine::open(&path).unwrap();
    assert!(engine.lookup_internal_id(10).is_none());
    assert!(engine.lookup_internal_id(20).is_none());
    let person = engine.get_label_id("Person").unwrap();
    let snapshot = engine.snapshot();
    assert!(snapshot.nodes().next().is_none());
    assert_eq!(snapshot.node_count(Some(person)), 0);
}

#[test]
fn core_0_1_label_and_reltype_namespaces_are_separate() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);

    let person;
    let knows;
    {
        let engine = GraphEngine::open(&path).unwrap();
        person = engine.get_or_create_label("Person").unwrap();
        knows = engine.get_or_create_rel_type("Person").unwrap();
        assert_eq!(person, 1);
        assert_eq!(knows, 1);

        let mut tx = engine.begin_write();
        let a = tx.create_node(10, person).unwrap();
        let b = tx.create_node(20, person).unwrap();
        tx.create_edge(a, knows, b);
        tx.commit().unwrap();
    }

    let engine = GraphEngine::open(&path).unwrap();
    assert_eq!(engine.get_label_id("Person"), Some(person));
    assert_eq!(engine.get_label_name(person).as_deref(), Some("Person"));
    assert_eq!(engine.get_rel_type_id("Person"), Some(knows));
    assert_eq!(engine.get_rel_type_name(knows).as_deref(), Some("Person"));
    assert_core_edge(&engine, 10, knows, 20);
}

#[test]
fn core_0_1_label_scan_uses_label_nodes_keyspace() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);

    let person;
    let company;
    {
        let engine = GraphEngine::open(&path).unwrap();
        person = engine.get_or_create_label("Person").unwrap();
        company = engine.get_or_create_label("Company").unwrap();
        let mut tx = engine.begin_write();
        let alice = tx.create_node(10, person).unwrap();
        let bob = tx.create_node(20, person).unwrap();
        let acme = tx.create_node(30, company).unwrap();
        tx.set_node_property(alice, "name".to_string(), "Alice".into());
        tx.set_node_property(bob, "name".to_string(), "Bob".into());
        tx.set_node_property(acme, "name".to_string(), "Acme".into());
        tx.commit().unwrap();
    }

    let engine = GraphEngine::open(&path).unwrap();
    let snapshot = engine.snapshot();
    let person_nodes: Vec<_> = snapshot.nodes_with_label(person).collect();
    let company_nodes: Vec<_> = snapshot.nodes_with_label(company).collect();
    assert_eq!(person_nodes.len(), 2);
    assert_eq!(company_nodes.len(), 1);
    for iid in person_nodes {
        assert_eq!(snapshot.node_label(iid), Some(person));
    }
}

#[test]
fn core_0_1_node_and_edge_properties_survive_reopen() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);

    let edge_key;
    {
        let engine = GraphEngine::open(&path).unwrap();
        let person = engine.get_or_create_label("Person").unwrap();
        let knows = engine.get_or_create_rel_type("KNOWS").unwrap();
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, person).unwrap();
        let b = tx.create_node(20, person).unwrap();
        tx.create_edge(a, knows, b);
        tx.set_node_property(
            a,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        );
        tx.set_node_property(a, "age".to_string(), PropertyValue::Int(30));
        tx.set_edge_property(a, knows, b, "since".to_string(), PropertyValue::Int(2024));
        tx.commit().unwrap();
        edge_key = EdgeKey {
            src: a,
            rel: knows,
            dst: b,
        };
    }

    let engine = GraphEngine::open(&path).unwrap();
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

    let props = snapshot.node_properties(a).unwrap();
    assert_eq!(
        props.keys().cloned().collect::<Vec<_>>(),
        vec!["age".to_string(), "name".to_string()]
    );
}

#[test]
fn core_0_1_snapshot_isolation_holds_across_commits() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);
    let engine = GraphEngine::open(&path).unwrap();
    let person = engine.get_or_create_label("Person").unwrap();

    {
        let mut tx = engine.begin_write();
        tx.create_node(10, person).unwrap();
        tx.commit().unwrap();
    }

    let old_snapshot = engine.snapshot();
    assert_eq!(old_snapshot.node_count(Some(person)), 1);

    {
        let mut tx = engine.begin_write();
        tx.create_node(20, person).unwrap();
        tx.commit().unwrap();
    }

    assert_eq!(old_snapshot.node_count(Some(person)), 1);
    assert_eq!(engine.snapshot().node_count(Some(person)), 2);
}

#[test]
fn core_0_1_duplicate_edge_is_idempotent_not_parallel() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);
    let engine = GraphEngine::open(&path).unwrap();
    let person = engine.get_or_create_label("Person").unwrap();
    let knows = engine.get_or_create_rel_type("KNOWS").unwrap();

    let mut tx = engine.begin_write();
    let a = tx.create_node(10, person).unwrap();
    let b = tx.create_node(20, person).unwrap();
    tx.create_edge(a, knows, b);
    tx.create_edge(a, knows, b);
    tx.commit().unwrap();

    let edges: Vec<_> = engine.snapshot().neighbors(a, Some(knows)).collect();
    assert_eq!(
        edges,
        vec![EdgeKey {
            src: a,
            rel: knows,
            dst: b
        }]
    );
}

#[test]
fn core_0_1_storage_format_epoch_mismatch_fails_fast() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);

    {
        let _engine = GraphEngine::open(&path).unwrap();
    }
    {
        let db = Database::builder(&path).open().unwrap();
        let meta = db.keyspace("meta", KeyspaceCreateOptions::default).unwrap();
        let mut batch = db.batch().durability(Some(PersistMode::SyncAll));
        batch.insert(
            &meta,
            b"format_epoch",
            (STORAGE_FORMAT_EPOCH + 1).to_be_bytes(),
        );
        batch.commit().unwrap();
    }

    let err = GraphEngine::open(&path).unwrap_err();
    assert!(
        matches!(err, Error::StorageFormatMismatch { .. }),
        "expected storage format mismatch, got {err:?}"
    );
}

#[test]
fn core_0_1_storage_format_epoch_corruption_returns_error() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);

    {
        let _engine = GraphEngine::open(&path).unwrap();
    }
    {
        let db = Database::builder(&path).open().unwrap();
        let meta = db.keyspace("meta", KeyspaceCreateOptions::default).unwrap();
        let mut batch = db.batch().durability(Some(PersistMode::SyncAll));
        batch.insert(&meta, b"format_epoch", vec![1, 2, 3]);
        batch.commit().unwrap();
    }

    let err = GraphEngine::open(&path).unwrap_err();
    assert!(
        matches!(err, Error::StorageCorrupted(_)),
        "expected storage corruption, got {err:?}"
    );
}

#[test]
fn core_0_1_reopen_is_repeatable() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);

    let knows;
    {
        let engine = GraphEngine::open(&path).unwrap();
        let person = engine.get_or_create_label("Person").unwrap();
        knows = engine.get_or_create_rel_type("KNOWS").unwrap();
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, person).unwrap();
        let b = tx.create_node(20, person).unwrap();
        tx.create_edge(a, knows, b);
        tx.set_node_property(
            a,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        );
        tx.commit().unwrap();
    }

    for _ in 0..3 {
        let engine = GraphEngine::open(&path).unwrap();
        assert_core_edge(&engine, 10, knows, 20);
        let a = engine.lookup_internal_id(10).unwrap();
        assert_eq!(
            engine.snapshot().node_property(a, "name"),
            Some(PropertyValue::String("Alice".to_string()))
        );
    }
}

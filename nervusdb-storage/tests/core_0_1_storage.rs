use fjall::{Database, KeyspaceCreateOptions, PersistMode};
use nervusdb::storage::engine::GraphEngine;
use nervusdb::storage::{Error, STORAGE_FORMAT_EPOCH};
use nervusdb::{EdgeKey, GraphSnapshot, PropertyValue};
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
fn storage_epoch_2_database_is_rejected() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);
    std::fs::create_dir_all(&path).unwrap();
    {
        let db = Database::builder(&path).open().unwrap();
        let meta = db.keyspace("meta", KeyspaceCreateOptions::default).unwrap();
        let mut batch = db.batch().durability(Some(PersistMode::SyncAll));
        batch.insert(&meta, b"format_epoch", 2u64.to_be_bytes());
        batch.commit().unwrap();
    }

    let err = GraphEngine::open(&path).unwrap_err();

    assert!(matches!(
        err,
        Error::StorageFormatMismatch {
            expected: STORAGE_FORMAT_EPOCH,
            found: 2
        }
    ));
}

#[test]
fn storage_epoch_3_uses_meta_graph_data_and_adjacency_keyspaces() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);
    {
        let engine = GraphEngine::open(&path).unwrap();
        let person = engine.get_or_create_label("Person").unwrap();
        let mut tx = engine.begin_write();
        let alice = tx.create_node(10, person).unwrap();
        tx.set_node_property(alice, "name".to_string(), "Alice".into())
            .unwrap();
        tx.commit().unwrap();
    }

    let db = Database::builder(&path).open().unwrap();
    let mut names = db
        .list_keyspace_names()
        .into_iter()
        .map(|name| name.to_string())
        .collect::<Vec<_>>();
    names.sort();

    assert_eq!(db.keyspace_count(), 4);
    assert_eq!(
        names,
        vec![
            "adj_in".to_string(),
            "adj_out".to_string(),
            "graph_data".to_string(),
            "meta".to_string()
        ]
    );
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
        tx.create_edge(a, rel, b).unwrap();
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
        tx.create_edge(a, knows, b).unwrap();
        tx.set_node_property(
            a,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )
        .unwrap();
        tx.set_edge_property(a, knows, b, "since".to_string(), PropertyValue::Int(2024))
            .unwrap();
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
        tx.create_edge(a, knows, b).unwrap();
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
        tx.set_node_property(alice, "name".to_string(), "Alice".into())
            .unwrap();
        tx.set_node_property(bob, "name".to_string(), "Bob".into())
            .unwrap();
        tx.set_node_property(acme, "name".to_string(), "Acme".into())
            .unwrap();
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
        tx.create_edge(a, knows, b).unwrap();
        tx.set_node_property(
            a,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )
        .unwrap();
        tx.set_node_property(a, "age".to_string(), PropertyValue::Int(30))
            .unwrap();
        tx.set_edge_property(a, knows, b, "since".to_string(), PropertyValue::Int(2024))
            .unwrap();
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
fn core_0_1_batch_node_ids_are_committed_atomically() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);
    let engine = GraphEngine::open(&path).unwrap();
    let person = engine.get_or_create_label("Person").unwrap();

    {
        let mut tx = engine.begin_write();
        tx.create_node(10, person).unwrap();
        tx.create_node(20, person).unwrap();
    }

    assert!(
        engine.lookup_internal_id(10).is_none(),
        "dropped transaction must not persist staged node ids"
    );

    let mut tx = engine.begin_write();
    let a = tx.create_node(10, person).unwrap();
    let b = tx.create_node(20, person).unwrap();
    tx.commit().unwrap();

    assert_eq!(a, 0);
    assert_eq!(b, 1);
    assert_eq!(engine.lookup_internal_id(10), Some(a));
    assert_eq!(engine.lookup_internal_id(20), Some(b));
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
    for _ in 0..8 {
        tx.create_edge(a, knows, b).unwrap();
    }
    tx.commit().unwrap();

    let snapshot = engine.snapshot();
    let edges: Vec<_> = snapshot.neighbors(a, Some(knows)).collect();
    assert_eq!(
        edges,
        vec![EdgeKey {
            src: a,
            rel: knows,
            dst: b
        }]
    );
    assert_eq!(snapshot.incoming_neighbors(b, Some(knows)).count(), 1);
    assert_eq!(snapshot.edge_count(Some(knows)), 1);
}

#[test]
fn core_0_1_rejects_dangling_edges_and_missing_entity_properties() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);
    let engine = GraphEngine::open(&path).unwrap();
    let person = engine.get_or_create_label("Person").unwrap();
    let knows = engine.get_or_create_rel_type("KNOWS").unwrap();

    let mut tx = engine.begin_write();
    let a = tx.create_node(10, person).unwrap();

    assert!(matches!(
        tx.create_edge(a, knows, 999).unwrap_err(),
        Error::NodeNotFound(999)
    ));
    assert!(matches!(
        tx.create_edge(998, knows, a).unwrap_err(),
        Error::NodeNotFound(998)
    ));
    assert!(matches!(
        tx.set_node_property(999, "name".to_string(), "Ghost".into())
            .unwrap_err(),
        Error::NodeNotFound(999)
    ));
    assert!(matches!(
        tx.set_edge_property(a, knows, 999, "since".to_string(), 2024.into())
            .unwrap_err(),
        Error::EdgeNotFound { .. }
    ));
}

#[test]
fn core_0_1_tombstone_edge_cleans_edge_properties_after_reopen() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);
    let edge;
    let knows;
    {
        let engine = GraphEngine::open(&path).unwrap();
        let person = engine.get_or_create_label("Person").unwrap();
        knows = engine.get_or_create_rel_type("KNOWS").unwrap();
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, person).unwrap();
        let b = tx.create_node(20, person).unwrap();
        tx.create_edge(a, knows, b).unwrap();
        tx.set_edge_property(a, knows, b, "since".to_string(), 2024.into())
            .unwrap();
        tx.commit().unwrap();
        edge = EdgeKey {
            src: a,
            rel: knows,
            dst: b,
        };
    }

    {
        let engine = GraphEngine::open(&path).unwrap();
        let mut tx = engine.begin_write();
        tx.tombstone_edge(edge.src, edge.rel, edge.dst).unwrap();
        tx.commit().unwrap();
    }

    let engine = GraphEngine::open(&path).unwrap();
    let snapshot = engine.snapshot();
    assert!(snapshot.neighbors(edge.src, Some(knows)).next().is_none());
    assert!(
        snapshot
            .incoming_neighbors(edge.dst, Some(knows))
            .next()
            .is_none()
    );
    assert_eq!(snapshot.edge_property(edge, "since"), None);
    assert_eq!(snapshot.edge_properties(edge), None);
    assert_eq!(snapshot.edge_count(Some(knows)), 0);
}

#[test]
fn core_0_1_tombstone_node_detach_cleans_graph_state_after_reopen() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);
    let person;
    let company;
    let knows;
    let alice;
    let bob;
    let carol;
    {
        let engine = GraphEngine::open(&path).unwrap();
        person = engine.get_or_create_label("Person").unwrap();
        company = engine.get_or_create_label("Company").unwrap();
        knows = engine.get_or_create_rel_type("KNOWS").unwrap();
        let mut tx = engine.begin_write();
        alice = tx.create_node(10, person).unwrap();
        bob = tx.create_node(20, person).unwrap();
        carol = tx.create_node(30, person).unwrap();
        tx.add_node_label(alice, company).unwrap();
        tx.set_node_property(alice, "name".to_string(), "Alice".into())
            .unwrap();
        tx.set_node_property(bob, "name".to_string(), "Bob".into())
            .unwrap();
        tx.create_edge(alice, knows, bob).unwrap();
        tx.create_edge(carol, knows, alice).unwrap();
        tx.set_edge_property(alice, knows, bob, "since".to_string(), 2024.into())
            .unwrap();
        tx.set_edge_property(carol, knows, alice, "since".to_string(), 2025.into())
            .unwrap();
        tx.commit().unwrap();
    }

    {
        let engine = GraphEngine::open(&path).unwrap();
        let mut tx = engine.begin_write();
        tx.tombstone_node(alice).unwrap();
        tx.commit().unwrap();
    }

    let engine = GraphEngine::open(&path).unwrap();
    let snapshot = engine.snapshot();
    assert!(engine.lookup_internal_id(10).is_none());
    assert!(snapshot.is_tombstoned_node(alice));
    assert_eq!(snapshot.resolve_external(alice), None);
    assert_eq!(snapshot.node_property(alice, "name"), None);
    assert_eq!(snapshot.node_properties(alice), None);
    assert_eq!(snapshot.resolve_node_labels(alice), None);
    assert!(!snapshot.nodes().any(|node| node == alice));
    assert!(!snapshot.nodes_with_label(person).any(|node| node == alice));
    assert!(!snapshot.nodes_with_label(company).any(|node| node == alice));
    assert!(snapshot.neighbors(alice, Some(knows)).next().is_none());
    assert!(
        snapshot
            .incoming_neighbors(alice, Some(knows))
            .next()
            .is_none()
    );
    assert!(snapshot.neighbors(carol, Some(knows)).next().is_none());
    assert_eq!(
        snapshot.edge_property(
            EdgeKey {
                src: alice,
                rel: knows,
                dst: bob
            },
            "since"
        ),
        None
    );
    assert_eq!(
        snapshot.edge_property(
            EdgeKey {
                src: carol,
                rel: knows,
                dst: alice
            },
            "since"
        ),
        None
    );
    assert_eq!(snapshot.node_count(Some(person)), 2);
    assert_eq!(snapshot.edge_count(Some(knows)), 0);
}

#[test]
fn core_0_1_created_then_tombstoned_node_leaves_no_visible_graph_state() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);
    let engine = GraphEngine::open(&path).unwrap();
    let person = engine.get_or_create_label("Person").unwrap();
    let knows = engine.get_or_create_rel_type("KNOWS").unwrap();

    let mut tx = engine.begin_write();
    let a = tx.create_node(10, person).unwrap();
    let b = tx.create_node(20, person).unwrap();
    tx.create_edge(a, knows, b).unwrap();
    tx.tombstone_node(a).unwrap();
    assert!(matches!(
        tx.set_node_property(a, "name".to_string(), "Alice".into())
            .unwrap_err(),
        Error::NodeNotFound(id) if id == a
    ));
    tx.commit().unwrap();

    let snapshot = engine.snapshot();
    assert!(engine.lookup_internal_id(10).is_none());
    assert_eq!(engine.lookup_internal_id(20), Some(b));
    assert!(!snapshot.nodes_with_label(person).any(|node| node == a));
    assert!(snapshot.neighbors(a, Some(knows)).next().is_none());
    assert!(snapshot.incoming_neighbors(b, Some(knows)).next().is_none());
}

#[test]
fn core_0_1_created_then_tombstoned_edge_leaves_no_visible_graph_state() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);
    let engine = GraphEngine::open(&path).unwrap();
    let person = engine.get_or_create_label("Person").unwrap();
    let knows = engine.get_or_create_rel_type("KNOWS").unwrap();

    let mut tx = engine.begin_write();
    let a = tx.create_node(10, person).unwrap();
    let b = tx.create_node(20, person).unwrap();
    tx.create_edge(a, knows, b).unwrap();
    tx.tombstone_edge(a, knows, b).unwrap();
    assert!(matches!(
        tx.set_edge_property(a, knows, b, "since".to_string(), 2024.into())
            .unwrap_err(),
        Error::EdgeNotFound { .. }
    ));
    tx.commit().unwrap();

    let snapshot = engine.snapshot();
    assert!(snapshot.neighbors(a, Some(knows)).next().is_none());
    assert!(snapshot.incoming_neighbors(b, Some(knows)).next().is_none());
    assert_eq!(snapshot.edge_count(Some(knows)), 0);
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
        tx.create_edge(a, knows, b).unwrap();
        tx.set_node_property(
            a,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )
        .unwrap();
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

#[test]
fn core_0_1_node_property_equality_index_tracks_writes_and_reopen() {
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
        tx.set_node_property(alice, "name".to_string(), "Alice".into())
            .unwrap();
        tx.set_node_property(bob, "name".to_string(), "Alice".into())
            .unwrap();
        tx.set_node_property(acme, "name".to_string(), "Alice".into())
            .unwrap();
        tx.commit().unwrap();
    }

    let engine = GraphEngine::open(&path).unwrap();
    let snapshot = engine.snapshot();
    let mut people: Vec<_> = snapshot
        .nodes_with_label_and_property(person, "name", &"Alice".into())
        .collect();
    people.sort_unstable();
    assert_eq!(people.len(), 2);
    assert_eq!(
        snapshot
            .nodes_with_label_and_property(company, "name", &"Alice".into())
            .count(),
        1
    );
    assert!(
        snapshot
            .nodes_with_label_and_property(person, "name", &"Missing".into())
            .next()
            .is_none()
    );
}

#[test]
fn core_0_1_node_property_equality_index_tracks_property_update_and_remove() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);
    let engine = GraphEngine::open(&path).unwrap();
    let person = engine.get_or_create_label("Person").unwrap();

    let alice = {
        let mut tx = engine.begin_write();
        let alice = tx.create_node(10, person).unwrap();
        tx.set_node_property(alice, "name".to_string(), "Alice".into())
            .unwrap();
        tx.commit().unwrap();
        alice
    };

    {
        let mut tx = engine.begin_write();
        tx.set_node_property(alice, "name".to_string(), "Alicia".into())
            .unwrap();
        tx.commit().unwrap();
    }

    let snapshot = engine.snapshot();
    assert!(
        snapshot
            .nodes_with_label_and_property(person, "name", &"Alice".into())
            .next()
            .is_none()
    );
    assert_eq!(
        snapshot
            .nodes_with_label_and_property(person, "name", &"Alicia".into())
            .collect::<Vec<_>>(),
        vec![alice]
    );
    drop(snapshot);

    {
        let mut tx = engine.begin_write();
        tx.remove_node_property(alice, "name").unwrap();
        tx.commit().unwrap();
    }

    let snapshot = engine.snapshot();
    assert!(
        snapshot
            .nodes_with_label_and_property(person, "name", &"Alicia".into())
            .next()
            .is_none()
    );
}

#[test]
fn core_0_1_node_property_equality_index_tracks_label_and_tombstone_changes() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);
    let engine = GraphEngine::open(&path).unwrap();
    let person = engine.get_or_create_label("Person").unwrap();
    let company = engine.get_or_create_label("Company").unwrap();

    let alice = {
        let mut tx = engine.begin_write();
        let alice = tx.create_node(10, person).unwrap();
        tx.set_node_property(alice, "name".to_string(), "Alice".into())
            .unwrap();
        tx.commit().unwrap();
        alice
    };

    {
        let mut tx = engine.begin_write();
        tx.add_node_label(alice, company).unwrap();
        tx.commit().unwrap();
    }
    let snapshot = engine.snapshot();
    assert_eq!(
        snapshot
            .nodes_with_label_and_property(company, "name", &"Alice".into())
            .collect::<Vec<_>>(),
        vec![alice]
    );
    drop(snapshot);

    {
        let mut tx = engine.begin_write();
        tx.remove_node_label(alice, company).unwrap();
        tx.commit().unwrap();
    }
    let snapshot = engine.snapshot();
    assert!(
        snapshot
            .nodes_with_label_and_property(company, "name", &"Alice".into())
            .next()
            .is_none()
    );
    assert_eq!(
        snapshot
            .nodes_with_label_and_property(person, "name", &"Alice".into())
            .collect::<Vec<_>>(),
        vec![alice]
    );
    drop(snapshot);

    {
        let mut tx = engine.begin_write();
        tx.tombstone_node(alice).unwrap();
        tx.commit().unwrap();
    }
    drop(engine);

    let engine = GraphEngine::open(&path).unwrap();
    let snapshot = engine.snapshot();
    assert!(
        snapshot
            .nodes_with_label_and_property(person, "name", &"Alice".into())
            .next()
            .is_none()
    );
}

#[test]
fn core_0_1_node_property_equality_index_uses_final_same_txn_state() {
    let dir = tempdir().unwrap();
    let path = db_dir(&dir);
    let engine = GraphEngine::open(&path).unwrap();
    let person = engine.get_or_create_label("Person").unwrap();

    {
        let mut tx = engine.begin_write();
        let node = tx.create_node(10, person).unwrap();
        tx.set_node_property(node, "name".to_string(), "Alice".into())
            .unwrap();
        tx.remove_node_property(node, "name").unwrap();
        tx.commit().unwrap();
    }

    let snapshot = engine.snapshot();
    assert!(
        snapshot
            .nodes_with_label_and_property(person, "name", &"Alice".into())
            .next()
            .is_none()
    );
    assert_eq!(snapshot.node_count(Some(person)), 1);
}

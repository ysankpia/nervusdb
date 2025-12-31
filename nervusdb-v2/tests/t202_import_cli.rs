use nervusdb_v2::{Db, GraphSnapshot};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_import_cli_csv() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_csv.ndb");

    let nodes_csv = dir.path().join("nodes.csv");
    let edges_csv = dir.path().join("edges.csv");

    fs::write(
        &nodes_csv,
        "id:ID,:LABEL,name,age:int\n1,User,Alice,30\n2,User,Bob,25\n",
    )
    .unwrap();
    fs::write(
        &edges_csv,
        ":START_ID,:END_ID,:TYPE,since:int\n1,2,FOLLOWS,2024\n",
    )
    .unwrap();

    // Run ndb-import
    let status = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "ndb-import",
            "--",
            "--nodes",
            nodes_csv.to_str().unwrap(),
            "--edges",
            edges_csv.to_str().unwrap(),
            "--output",
            db_path.to_str().unwrap(),
            "--format",
            "csv",
        ])
        .status()
        .expect("Failed to execute ndb-import");

    assert!(status.success());

    // Verify using Db
    let db = Db::open(&db_path).unwrap();
    let snap = db.snapshot();

    // Find internal IDs by searching all nodes
    let nodes: Vec<_> = snap.nodes().collect();
    assert_eq!(nodes.len(), 2);

    let alice_iid = nodes
        .iter()
        .find(|&&iid| snap.resolve_external(iid) == Some(1))
        .cloned()
        .expect("Alice should exist");

    let bob_iid = nodes
        .iter()
        .find(|&&iid| snap.resolve_external(iid) == Some(2))
        .cloned()
        .expect("Bob should exist");

    let neighbors: Vec<_> = snap.neighbors(alice_iid, None).collect();
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].dst, bob_iid);

    // Ensure relationship type name -> id mapping is consistent after bulk import.
    let follows_id = snap
        .resolve_rel_type_id("FOLLOWS")
        .expect("FOLLOWS rel type should exist");
    let neighbors_filtered: Vec<_> = snap.neighbors(alice_iid, Some(follows_id)).collect();
    assert_eq!(neighbors_filtered.len(), 1);
    assert_eq!(neighbors_filtered[0].dst, bob_iid);
}

#[test]
fn test_import_cli_jsonl() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_jsonl.ndb");

    let nodes_jsonl = dir.path().join("nodes.jsonl");
    let edges_jsonl = dir.path().join("edges.jsonl");

    fs::write(
        &nodes_jsonl,
        "{\"id\": 1, \"label\": \"User\", \"properties\": {\"name\": \"Alice\", \"age\": 30}}\n\
         {\"id\": 2, \"label\": \"User\", \"properties\": {\"name\": \"Bob\", \"age\": 25}}\n",
    )
    .unwrap();

    fs::write(
        &edges_jsonl,
        "{\"src\": 1, \"dst\": 2, \"type\": \"FOLLOWS\", \"properties\": {\"since\": 2024}}\n",
    )
    .unwrap();

    // Run ndb-import
    let status = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "ndb-import",
            "--",
            "--nodes",
            nodes_jsonl.to_str().unwrap(),
            "--edges",
            edges_jsonl.to_str().unwrap(),
            "--output",
            db_path.to_str().unwrap(),
            "--format",
            "jsonl",
        ])
        .status()
        .expect("Failed to execute ndb-import");

    assert!(status.success());

    // Verify using Db
    let db = Db::open(&db_path).unwrap();
    let snap = db.snapshot();

    let nodes: Vec<_> = snap.nodes().collect();
    assert_eq!(nodes.len(), 2);

    let alice_iid = nodes
        .iter()
        .find(|&&iid| snap.resolve_external(iid) == Some(1))
        .cloned()
        .expect("Alice should exist");

    let bob_iid = nodes
        .iter()
        .find(|&&iid| snap.resolve_external(iid) == Some(2))
        .cloned()
        .expect("Bob should exist");

    let neighbors: Vec<_> = snap.neighbors(alice_iid, None).collect();
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].dst, bob_iid);

    // Ensure relationship type name -> id mapping is consistent after bulk import.
    let follows_id = snap
        .resolve_rel_type_id("FOLLOWS")
        .expect("FOLLOWS rel type should exist");
    let neighbors_filtered: Vec<_> = snap.neighbors(alice_iid, Some(follows_id)).collect();
    assert_eq!(neighbors_filtered.len(), 1);
    assert_eq!(neighbors_filtered[0].dst, bob_iid);
}

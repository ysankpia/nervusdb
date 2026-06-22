use fjall::{Database, KeyspaceCreateOptions, PersistMode};
use std::process::Command;
use tempfile::tempdir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_nervusdb")
}

#[test]
fn cli_fsck_returns_zero_for_clean_database() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("db");

    let write = Command::new(binary())
        .args([
            "v2",
            "write",
            "--db",
            db_path.to_str().unwrap(),
            "--cypher",
            "CREATE (n:Person {name: 'Alice'})",
        ])
        .output()
        .unwrap();
    assert!(write.status.success(), "{}", stderr(&write));

    let fsck = Command::new(binary())
        .args(["v2", "fsck", "--db", db_path.to_str().unwrap(), "--json"])
        .output()
        .unwrap();

    assert_eq!(fsck.status.code(), Some(0), "{}", stderr(&fsck));
    let json: serde_json::Value = serde_json::from_slice(&fsck.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["issues"].as_array().unwrap().len(), 0);
}

#[test]
fn cli_fsck_repair_rebuilds_derived_property_index() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("db");

    let write = Command::new(binary())
        .args([
            "v2",
            "write",
            "--db",
            db_path.to_str().unwrap(),
            "--cypher",
            "CREATE (n:Person {name: 'Alice'})",
        ])
        .output()
        .unwrap();
    assert!(write.status.success(), "{}", stderr(&write));

    {
        let db = Database::builder(&db_path).open().unwrap();
        let graph_data = db
            .keyspace("graph_data", KeyspaceCreateOptions::default)
            .unwrap();
        let key = node_prop_index_key(1, "name", &encoded_string("Alice"), 0);
        let mut batch = db.batch().durability(Some(PersistMode::SyncAll));
        batch.remove(&graph_data, key);
        batch.commit().unwrap();
    }

    let broken = Command::new(binary())
        .args(["v2", "fsck", "--db", db_path.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert_eq!(broken.status.code(), Some(1), "{}", stderr(&broken));
    let json: serde_json::Value = serde_json::from_slice(&broken.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(
        json["issues"][0]["kind"],
        serde_json::Value::String("missing_node_property_index".to_string())
    );

    let repaired = Command::new(binary())
        .args([
            "v2",
            "fsck",
            "--db",
            db_path.to_str().unwrap(),
            "--repair",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(repaired.status.code(), Some(0), "{}", stderr(&repaired));
    let json: serde_json::Value = serde_json::from_slice(&repaired.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["repaired"], true);
    assert_eq!(
        json["repairs"][1]["kind"],
        serde_json::Value::String("rebuilt_node_property_index".to_string())
    );
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn node_prop_index_key(label: u32, key: &str, encoded_value: &[u8], node: u32) -> Vec<u8> {
    let key_len = u16::try_from(key.len()).unwrap();
    let value_len = u32::try_from(encoded_value.len()).unwrap();
    let mut out = Vec::with_capacity(15 + key.len() + encoded_value.len());
    out.push(0x50);
    out.extend_from_slice(&label.to_be_bytes());
    out.extend_from_slice(&key_len.to_be_bytes());
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(&value_len.to_be_bytes());
    out.extend_from_slice(encoded_value);
    out.extend_from_slice(&node.to_be_bytes());
    out
}

fn encoded_string(value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut out = vec![4];
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
    out
}

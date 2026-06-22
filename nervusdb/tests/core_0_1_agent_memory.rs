use nervusdb::query::{Params, Value, prepare, query_collect};
use nervusdb::{Db, GraphSnapshot};
use tempfile::tempdir;

fn write(db: &Db, cypher: &str) {
    let snapshot = db.snapshot();
    let prepared = prepare(cypher).unwrap();
    let mut txn = db.begin_write();
    prepared
        .execute_write(&snapshot, &mut txn, &Params::new())
        .unwrap();
    txn.commit().unwrap();
}

#[test]
fn core_0_1_agent_memory_smoke_survives_reopen() {
    let dir = tempdir().unwrap();

    {
        let db = Db::open(dir.path()).unwrap();
        write(
            &db,
            "CREATE (alice:Character {name: 'Alice', status: 'draft'})",
        );
        write(
            &db,
            "CREATE (bob:Character {name: 'Bob', status: 'active'})",
        );
        write(
            &db,
            "CREATE (arrival:Event {name: 'Arrival', chapter: 1, kind: 'scene'})",
        );
        write(
            &db,
            "CREATE (secret:Fact {name: 'Secret', kind: 'lore', status: 'open'})",
        );
        write(
            &db,
            "MATCH (a:Character) WHERE a.name = 'Alice' MATCH (b:Character) WHERE b.name = 'Bob' CREATE (a)-[:KNOWS]->(b)",
        );
        write(
            &db,
            "MATCH (a:Character) WHERE a.name = 'Alice' MATCH (e:Event) WHERE e.name = 'Arrival' CREATE (a)-[:APPEARS_IN]->(e)",
        );
        write(
            &db,
            "MATCH (f:Fact) WHERE f.name = 'Secret' MATCH (e:Event) WHERE e.name = 'Arrival' CREATE (f)-[:CAUSES]->(e)",
        );
        write(
            &db,
            "MATCH (e:Event) WHERE e.name = 'Arrival' MATCH (f:Fact) WHERE f.name = 'Secret' CREATE (e)-[:MENTIONS]->(f)",
        );

        let alice = query_collect(
            &db.snapshot(),
            "MATCH (n:Character) WHERE n.name = 'Alice' RETURN n",
            &Params::new(),
        )
        .unwrap();
        assert_eq!(alice.len(), 1);

        let events = query_collect(
            &db.snapshot(),
            "MATCH (c:Character)-[:APPEARS_IN]->(e:Event) WHERE c.name = 'Alice' RETURN e.name LIMIT 10",
            &Params::new(),
        )
        .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].columns()[0].1,
            Value::String("Arrival".to_string())
        );

        write(
            &db,
            "MATCH (n:Character) WHERE n.name = 'Alice' SET n.status = 'active'",
        );
        let updated = query_collect(
            &db.snapshot(),
            "MATCH (n:Character) WHERE n.name = 'Alice' RETURN n.status LIMIT 1",
            &Params::new(),
        )
        .unwrap();
        assert_eq!(updated[0].columns()[0].1, Value::String("active".into()));

        write(
            &db,
            "MATCH (n:Fact) WHERE n.name = 'Secret' DETACH DELETE n",
        );
        let mentions = query_collect(
            &db.snapshot(),
            "MATCH (e:Event)-[:MENTIONS]->(f:Fact) RETURN f.name LIMIT 10",
            &Params::new(),
        )
        .unwrap();
        assert!(mentions.is_empty());
    }

    let db = Db::open(dir.path()).unwrap();
    let alice = query_collect(
        &db.snapshot(),
        "MATCH (n:Character) WHERE n.name = 'Alice' RETURN n.status LIMIT 1",
        &Params::new(),
    )
    .unwrap();
    assert_eq!(alice.len(), 1);
    assert_eq!(alice[0].columns()[0].1, Value::String("active".into()));

    let snapshot = db.snapshot();
    let character = snapshot.resolve_label_id("Character").unwrap();
    assert_eq!(snapshot.node_count(Some(character)), 2);
    drop(snapshot);

    #[cfg(feature = "unstable-admin")]
    {
        drop(db);
        let report =
            nervusdb::admin::fsck(dir.path(), nervusdb::admin::FsckOptions { repair: false })
                .unwrap();
        assert!(report.ok, "{:?}", report.issues);
    }
}

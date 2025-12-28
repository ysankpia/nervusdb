#![cfg(all(feature = "temporal", not(target_arch = "wasm32")))]

use nervusdb_core::{
    EnsureEntityOptions, EpisodeInput, FactWriteInput, TemporalStore, TimelineQuery, TimelineRole,
};
use redb::Database;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tempfile::tempdir;

#[test]
#[ignore] // Benchmark test using deprecated TemporalStore v1 - replaced by TemporalStoreV2
fn benchmark_query_performance() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bench_query.redb");
    let redb = Arc::new(Database::create(&path).unwrap());
    let mut store = TemporalStore::open(redb).unwrap();

    let total_facts = 5_000;

    println!("Seeding {} facts...", total_facts);

    // Create a dummy episode
    let episode = store
        .add_episode(EpisodeInput {
            source_type: "bench".into(),
            payload: Value::Null,
            occurred_at: "2025-01-01T00:00:00Z".into(),
            trace_hash: None,
        })
        .unwrap();

    // Create entities
    let alice = store
        .ensure_entity("person", "alice", EnsureEntityOptions::default())
        .unwrap();
    // Create many other entities to dilute the pool
    for i in 0..1000 {
        store
            .ensure_entity(
                "person",
                &format!("person_{}", i),
                EnsureEntityOptions::default(),
            )
            .unwrap();
    }

    // Insert facts
    // We want 'alice' to have some facts, but not all.
    // Let's give alice 100 facts.
    // And insert 99,900 other facts for other people.

    for i in 0..total_facts {
        let subject_id = if i % 1000 == 0 {
            alice.entity_id
        } else {
            (i % 1000 + 2) as u64
        };

        store
            .upsert_fact(FactWriteInput {
                subject_entity_id: subject_id,
                predicate_key: "action".into(),
                object_entity_id: None,
                object_value: Some(Value::String("something".into())),
                valid_from: Some("2025-01-01T00:00:00Z".into()),
                valid_to: None,
                confidence: None,
                source_episode_id: episode.episode_id,
            })
            .unwrap();
    }

    println!("Seeding complete. Starting query benchmark...");

    let start = Instant::now();
    let results = store
        .query_timeline(&TimelineQuery {
            entity_id: alice.entity_id,
            predicate_key: None,
            role: Some(TimelineRole::Subject),
            ..Default::default()
        })
        .unwrap();
    let duration = start.elapsed();

    println!("Query found {} facts in {:?}", results.len(), duration);

    // With O(N), 100k items might take milliseconds.
    // With O(1) lookup (plus iteration over 100 items), it should be microseconds.
    assert!(
        duration.as_micros() < 500,
        "Query took too long: {:?}. Likely O(N).",
        duration
    );
}

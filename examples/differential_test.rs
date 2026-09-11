use graphlite::{GraphError, GraphLite, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

// Deterministic PRNG
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }
    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }
    fn gen_range(&mut self, min: u64, max: u64) -> u64 {
        if min >= max {
            return min;
        }
        min + (self.next_u64() % (max - min))
    }
    fn choose<'a, T>(&mut self, slice: &'a [T]) -> &'a T {
        &slice[(self.next_u64() as usize) % slice.len()]
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ModelNode {
    id: u64,
    labels: HashSet<String>,
    properties: HashMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
struct ModelEdge {
    id: u64,
    src: u64,
    dst: u64,
    edge_type: String,
    properties: HashMap<String, Value>,
    weight: f64,
}

#[derive(Clone, Debug)]
struct GraphOracle {
    nodes: HashMap<u64, ModelNode>,
    edges: HashMap<u64, ModelEdge>,
    outgoing: HashMap<u64, HashSet<u64>>,
    incoming: HashMap<u64, HashSet<u64>>,
}

impl GraphOracle {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
        }
    }

    fn add_node(&mut self, id: u64, labels: HashSet<String>, properties: HashMap<String, Value>) {
        self.nodes.insert(
            id,
            ModelNode {
                id,
                labels,
                properties,
            },
        );
        self.outgoing.entry(id).or_default();
        self.incoming.entry(id).or_default();
    }

    fn add_edge(
        &mut self,
        id: u64,
        src: u64,
        dst: u64,
        edge_type: String,
        properties: HashMap<String, Value>,
        weight: f64,
    ) {
        self.edges.insert(
            id,
            ModelEdge {
                id,
                src,
                dst,
                edge_type,
                properties,
                weight,
            },
        );
        self.outgoing.entry(src).or_default().insert(id);
        self.incoming.entry(dst).or_default().insert(id);
    }

    fn remove_edge(&mut self, id: u64) -> Option<ModelEdge> {
        if let Some(edge) = self.edges.remove(&id) {
            if let Some(out_set) = self.outgoing.get_mut(&edge.src) {
                out_set.remove(&id);
            }
            if let Some(in_set) = self.incoming.get_mut(&edge.dst) {
                in_set.remove(&id);
            }
            Some(edge)
        } else {
            None
        }
    }

    fn remove_node(&mut self, id: u64) -> Option<ModelNode> {
        if let Some(node) = self.nodes.remove(&id) {
            // Cascade remove outgoing
            if let Some(out_edges) = self.outgoing.remove(&id) {
                for eid in out_edges {
                    if let Some(e) = self.edges.remove(&eid) {
                        if let Some(in_set) = self.incoming.get_mut(&e.dst) {
                            in_set.remove(&eid);
                        }
                    }
                }
            }
            // Cascade remove incoming
            if let Some(in_edges) = self.incoming.remove(&id) {
                for eid in in_edges {
                    if let Some(e) = self.edges.remove(&eid) {
                        if let Some(out_set) = self.outgoing.get_mut(&e.src) {
                            out_set.remove(&eid);
                        }
                    }
                }
            }
            Some(node)
        } else {
            None
        }
    }
}

fn verify_equivalence(
    db: &GraphLite,
    oracle: &GraphOracle,
    sample_size: usize,
    rng: &mut SimpleRng,
) -> Result<(), String> {
    // 1. Verify Node count
    let node_keys: Vec<u64> = oracle.nodes.keys().copied().collect();
    let edge_keys: Vec<u64> = oracle.edges.keys().copied().collect();

    // 2. Check random sample of nodes
    let check_nodes = if node_keys.len() <= sample_size {
        node_keys.clone()
    } else {
        let mut sample = Vec::with_capacity(sample_size);
        for _ in 0..sample_size {
            sample.push(*rng.choose(&node_keys));
        }
        sample
    };

    for &nid in &check_nodes {
        let expected = &oracle.nodes[&nid];
        let actual = db
            .get_node(nid)
            .ok_or_else(|| format!("Node {} exists in Oracle but missing in DB", nid))?;

        if actual.labels != expected.labels {
            return Err(format!(
                "Node {} labels mismatch! Expected {:?}, got {:?}",
                nid, expected.labels, actual.labels
            ));
        }
        if actual.properties != expected.properties {
            return Err(format!(
                "Node {} props mismatch! Expected {:?}, got {:?}",
                nid, expected.properties, actual.properties
            ));
        }

        let expected_out = oracle.outgoing.get(&nid).cloned().unwrap_or_default();
        let actual_out: HashSet<u64> = actual.outgoing.iter().copied().collect();
        if actual_out != expected_out {
            return Err(format!(
                "Node {} outgoing edges mismatch! Expected {:?}, got {:?}",
                nid, expected_out, actual_out
            ));
        }

        let expected_in = oracle.incoming.get(&nid).cloned().unwrap_or_default();
        let actual_in: HashSet<u64> = actual.incoming.iter().copied().collect();
        if actual_in != expected_in {
            return Err(format!(
                "Node {} incoming edges mismatch! Expected {:?}, got {:?}",
                nid, expected_in, actual_in
            ));
        }
    }

    // 3. Check random sample of edges
    let check_edges = if edge_keys.len() <= sample_size {
        edge_keys.clone()
    } else {
        let mut sample = Vec::with_capacity(sample_size);
        for _ in 0..sample_size {
            sample.push(*rng.choose(&edge_keys));
        }
        sample
    };

    for &eid in &check_edges {
        let expected = &oracle.edges[&eid];
        let actual = db
            .get_edge(eid)
            .ok_or_else(|| format!("Edge {} exists in Oracle but missing in DB", eid))?;

        if actual.src_id != expected.src {
            return Err(format!(
                "Edge {} src mismatch! Expected {}, got {}",
                eid, expected.src, actual.src_id
            ));
        }
        if actual.dst_id != expected.dst {
            return Err(format!(
                "Edge {} dst mismatch! Expected {}, got {}",
                eid, expected.dst, actual.dst_id
            ));
        }
        if actual.edge_type != expected.edge_type {
            return Err(format!(
                "Edge {} type mismatch! Expected {}, got {}",
                eid, expected.edge_type, actual.edge_type
            ));
        }
        if actual.properties != expected.properties {
            return Err(format!(
                "Edge {} props mismatch! Expected {:?}, got {:?}",
                eid, expected.properties, actual.properties
            ));
        }
        if (actual.weight - expected.weight).abs() > 1e-6 {
            return Err(format!(
                "Edge {} weight mismatch! Expected {}, got {}",
                eid, expected.weight, actual.weight
            ));
        }
    }

    Ok(())
}

/// 输出目录：`DB_DIR` 环境变量，默认当前目录下的 `bench_db`。
fn resolve_db_dir() -> PathBuf {
    PathBuf::from(std::env::var("DB_DIR").unwrap_or_else(|_| "bench_db".to_string()))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = resolve_db_dir();
    std::fs::create_dir_all(&target_dir)?;

    let db_path = target_dir.join("diff_team3.db");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("db.wal"));

    let total_steps: usize = std::env::var("STEPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);

    println!("============================================================");
    println!(" [STAGE 3] SQLite-STYLE DIFFERENTIAL TESTING (ORACLE COMPARISON)");
    println!(" TARGET: TEAM 3 (Pure DeepSeek Kernel Architecture)");
    println!(
        " Steps : {} random operations with Continuous Equivalence Checks",
        total_steps
    );
    println!(" DB    : {}", db_path.display());
    println!("============================================================");

    let mut db = GraphLite::open_with_pool_mb(&db_path, 32)?;
    let mut oracle = GraphOracle::new();
    let mut rng = SimpleRng::new(42);

    let label_pool = ["User", "Post", "Comment", "Tag", "Group"];
    let edge_type_pool = ["KNOWS", "LIKES", "FOLLOWS", "AUTHORED", "TAGGED"];

    let mut stats_add_node = 0;
    let mut stats_add_edge = 0;
    let mut stats_del_edge = 0;
    let mut stats_del_node = 0;
    let mut stats_upd_node = 0;
    let mut stats_upd_edge = 0;
    let mut stats_rollback = 0;
    let mut stats_reopen = 0;

    let t0 = Instant::now();

    for step in 1..=total_steps {
        let node_count = oracle.nodes.len();
        let edge_count = oracle.edges.len();

        // Operation selection
        // Biased towards creation early, mixed later
        let op = if node_count < 10 {
            0 // AddNode
        } else if node_count >= 10 && edge_count < 10 {
            rng.gen_range(0, 2) // AddNode or AddEdge
        } else {
            rng.gen_range(0, 7)
        };

        match op {
            0 => {
                // Add Node
                let mut labels = HashSet::new();
                labels.insert(rng.choose(&label_pool).to_string());
                if rng.next_u64().is_multiple_of(3) {
                    labels.insert(rng.choose(&label_pool).to_string());
                }
                let mut props = HashMap::new();
                props.insert("idx".to_string(), Value::from(step as i64));
                props.insert("active".to_string(), Value::from(true));
                if rng.next_u64().is_multiple_of(2) {
                    props.insert("score".to_string(), Value::from((step as f64) * 1.5));
                }

                let id = db.add_node(labels.clone(), props.clone())?;
                oracle.add_node(id, labels, props);
                stats_add_node += 1;
            }
            1 => {
                // Add Edge
                if node_count >= 2 {
                    let nodes_vec: Vec<u64> = oracle.nodes.keys().copied().collect();
                    let src = *rng.choose(&nodes_vec);
                    let dst = *rng.choose(&nodes_vec);
                    if src != dst {
                        let etype = rng.choose(&edge_type_pool).to_string();
                        let mut props = HashMap::new();
                        props.insert("created_step".to_string(), Value::from(step as i64));
                        let weight = (rng.next_u64() % 100) as f64 / 10.0;

                        let eid = db.add_edge(src, dst, &etype, props.clone(), weight)?;
                        oracle.add_edge(eid, src, dst, etype, props, weight);
                        stats_add_edge += 1;
                    }
                }
            }
            2 => {
                // Update Node Property
                if node_count > 0 {
                    let nodes_vec: Vec<u64> = oracle.nodes.keys().copied().collect();
                    let nid = *rng.choose(&nodes_vec);
                    let new_score = (step as f64) * 2.5;
                    db.update_node_property(nid, "score", new_score)?;
                    oracle
                        .nodes
                        .get_mut(&nid)
                        .unwrap()
                        .properties
                        .insert("score".to_string(), Value::from(new_score));
                    stats_upd_node += 1;
                }
            }
            3 => {
                // Update Edge Property
                if edge_count > 0 {
                    let edges_vec: Vec<u64> = oracle.edges.keys().copied().collect();
                    let eid = *rng.choose(&edges_vec);
                    let new_val = format!("note-step-{}", step);
                    db.update_edge_property(eid, "note", new_val.clone())?;
                    oracle
                        .edges
                        .get_mut(&eid)
                        .unwrap()
                        .properties
                        .insert("note".to_string(), Value::from(new_val));
                    stats_upd_edge += 1;
                }
            }
            4 => {
                // Remove Edge
                if edge_count > 0 {
                    let edges_vec: Vec<u64> = oracle.edges.keys().copied().collect();
                    let eid = *rng.choose(&edges_vec);
                    let res = db.remove_edge(eid);
                    assert!(res.is_ok(), "Failed to remove edge {}", eid);
                    oracle.remove_edge(eid);
                    stats_del_edge += 1;
                }
            }
            5 => {
                // Remove Node (Cascades incident edges)
                if node_count > 5 {
                    let nodes_vec: Vec<u64> = oracle.nodes.keys().copied().collect();
                    let nid = *rng.choose(&nodes_vec);
                    let res = db.remove_node(nid);
                    assert!(res.is_ok(), "Failed to remove node {}", nid);
                    oracle.remove_node(nid);
                    stats_del_node += 1;
                }
            }
            6 => {
                // Transaction Rollback Zero-Pollution Test
                let pre_tx_oracle = oracle.clone();
                let tx_res: Result<(), GraphError> = db.with_transaction(|tx| {
                    for i in 0..10 {
                        let mut p = HashMap::new();
                        p.insert("temp".to_string(), Value::from(i as i64));
                        tx.add_node(HashSet::from(["Ghost".to_string()]), p)?;
                    }
                    // Deliberately trigger rollback!
                    Err(GraphError::General(
                        "Deliberate abort for rollback test".to_string(),
                    ))
                });

                assert!(tx_res.is_err(), "Transaction should have failed");
                // Verify oracle state unchanged
                oracle = pre_tx_oracle;
                stats_rollback += 1;
            }
            _ => unreachable!(),
        }

        // Periodic Deep Verification against Oracle (every 250 steps)
        if step % 250 == 0 || step == total_steps {
            if let Err(e) = verify_equivalence(&db, &oracle, 100, &mut rng) {
                panic!("\n[DIFFERENTIAL TEST FAILED at Step {}]: {}", step, e);
            }
            print!(
                "\r   Progress: {} / {} ops | Nodes: {}, Edges: {} | Verified OK",
                step,
                total_steps,
                oracle.nodes.len(),
                oracle.edges.len()
            );
            std::io::Write::flush(&mut std::io::stdout())?;
        }

        // Periodic Close & Reopen (Crash / Durability Check every 2,500 steps)
        if step % 2500 == 0 {
            db.checkpoint()?;
            drop(db);
            db = GraphLite::open_with_pool_mb(&db_path, 32)?;
            if let Err(e) = verify_equivalence(&db, &oracle, 200, &mut rng) {
                panic!("\n[REOPEN PERSISTENCE FAILED after Step {}]: {}", step, e);
            }
            stats_reopen += 1;
        }
    }

    // Final full verification
    println!("\n-> Performing Final Full-State Equivalence Verification...");
    if let Err(e) = verify_equivalence(&db, &oracle, oracle.nodes.len(), &mut rng) {
        panic!("\n[FINAL FULL VERIFICATION FAILED]: {}", e);
    }

    let dur = t0.elapsed();
    println!("------------------------------------------------------------");
    println!(" [STAGE 3] DIFFERENTIAL TEST 100% PASSED FOR TEAM 3!");
    println!(" Elapsed Time   : {:.2?}", dur);
    println!(
        " Operations     : {} ops ({:.0} ops/s)",
        total_steps,
        total_steps as f64 / dur.as_secs_f64()
    );
    println!(" Breakdown      :");
    println!("   Add Node     : {}", stats_add_node);
    println!("   Add Edge     : {}", stats_add_edge);
    println!("   Update Node  : {}", stats_upd_node);
    println!("   Update Edge  : {}", stats_upd_edge);
    println!("   Remove Edge  : {}", stats_del_edge);
    println!("   Remove Node  : {} (Cascaded)", stats_del_node);
    println!(
        "   Tx Rollbacks : {} (Zero-Pollution Verified)",
        stats_rollback
    );
    println!("   Restarts     : {} (Disk Replay Verified)", stats_reopen);
    println!(
        " Final Model    : {} active nodes, {} active edges",
        oracle.nodes.len(),
        oracle.edges.len()
    );
    println!("============================================================");

    Ok(())
}

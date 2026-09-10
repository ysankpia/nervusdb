use graphlite::{Direction, GraphError, GraphLite, Value};
use std::collections::{HashMap, HashSet};

fn main() -> Result<(), GraphError> {
    println!("============================================================");
    println!("       GraphLite-RS: The SQLite of Graph Databases          ");
    println!("============================================================");

    let db_path = "graphlite_demo.db";
    let db = GraphLite::open(db_path)?;
    println!(
        "[1] Successfully opened/initialized graph database: {}",
        db_path
    );

    // 1. 事务批量插入：构建社交与知识图谱
    let mut tx = db.begin_transaction()?;
    println!("\n[2] Starting Transaction #{}...", tx.tx_id());

    let mut alice_props = HashMap::new();
    alice_props.insert("name".to_string(), Value::from("Alice"));
    alice_props.insert("age".to_string(), Value::from(28));
    let mut alice_labels = HashSet::new();
    alice_labels.insert("Person".to_string());
    let alice = tx.add_node(alice_labels, alice_props)?;

    let mut bob_props = HashMap::new();
    bob_props.insert("name".to_string(), Value::from("Bob"));
    bob_props.insert("age".to_string(), Value::from(32));
    let mut bob_labels = HashSet::new();
    bob_labels.insert("Person".to_string());
    let bob = tx.add_node(bob_labels, bob_props)?;

    let mut charlie_props = HashMap::new();
    charlie_props.insert("name".to_string(), Value::from("Charlie"));
    charlie_props.insert("age".to_string(), Value::from(24));
    let mut charlie_labels = HashSet::new();
    charlie_labels.insert("Person".to_string());
    let charlie = tx.add_node(charlie_labels, charlie_props)?;

    let mut dave_props = HashMap::new();
    dave_props.insert("name".to_string(), Value::from("Dave"));
    dave_props.insert("age".to_string(), Value::from(35));
    let mut dave_labels = HashSet::new();
    dave_labels.insert("Person".to_string());
    let dave = tx.add_node(dave_labels, dave_props)?;

    // 建立关系 (Alice) -[KNOWS: 1.5]-> (Bob) -[KNOWS: 2.0]-> (Charlie) -[KNOWS: 1.0]-> (Dave)
    // 以及快捷路径 (Alice) -[KNOWS: 6.0]-> (Dave)
    tx.add_edge(alice, bob, "KNOWS", HashMap::new(), 1.5)?;
    tx.add_edge(bob, charlie, "KNOWS", HashMap::new(), 2.0)?;
    tx.add_edge(charlie, dave, "KNOWS", HashMap::new(), 1.0)?;
    tx.add_edge(alice, dave, "KNOWS", HashMap::new(), 6.0)?;

    tx.commit()?;
    println!("    Transaction committed successfully!");
    println!(
        "    Current graph stats: {} nodes, {} edges.",
        db.node_count(),
        db.edge_count()
    );

    // 2. 模式匹配查询 DSL
    println!("\n[3] Querying: match (Person) -[KNOWS]-> (Person) where age > 25 ...");
    let results = db
        .query()
        .match_pattern("Person", "KNOWS", "Person")
        .filter_prop("age", |v| v.as_i64().unwrap_or(0) > 25)
        .execute();

    for path in results.paths() {
        let src_name = path
            .src
            .get_prop("name")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let dst_name = path
            .dst
            .get_prop("name")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        println!(
            "    Found Path: ({}) -[{}]-> ({}) [Weight: {}]",
            src_name, path.edge.edge_type, dst_name, path.edge.weight
        );
    }

    // 3. 2度社交关系多跳遍历 (2-Hop Friends Recommendation)
    println!(
        "\n[4] 2-Hop Traversal: Friends of friends for Alice (Node ID: {})...",
        alice
    );
    let two_hop = db
        .query()
        .traverse(alice, "KNOWS", Direction::Outgoing, 2)
        .execute();

    for p in two_hop.multi_hop_paths() {
        let hop_names: Vec<String> = p
            .nodes
            .iter()
            .map(|n| {
                n.get_prop("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string()
            })
            .collect();
        println!("    2-Hop Path: {}", hop_names.join(" -> "));
    }

    // 4. Dijkstra 最短路径算法 (Alice -> Dave)
    println!("\n[5] Weighted Shortest Path (Dijkstra) from Alice to Dave...");
    if let Some((cost, path)) = db.dijkstra(alice, dave, Some("KNOWS")) {
        println!(
            "    Shortest Path Node IDs: {:?} with total weight: {}",
            path, cost
        );
    }

    // 5. Checkpoint 刷盘
    println!("\n[6] Performing Checkpoint to flush snapshot to single file...");
    db.checkpoint()?;
    println!("    Checkpoint completed successfully!");

    println!("\nGraphLite-RS demo finished cleanly.");
    Ok(())
}

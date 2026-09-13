use nervusdb::{Direction, GraphError, NervusDb, Value};
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use tempfile::tempdir;

// =========================================================================
// 测试 1: 基础节点、边 CRUD 及属性更新测试
// =========================================================================
#[test]
fn test_01_crud_and_properties() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("crud_test.db");
    let db = NervusDb::open(&db_path)?;

    // 1. 添加带有不同类型属性的节点
    let mut props_n1 = HashMap::new();
    props_n1.insert("name".to_string(), Value::from("Alice"));
    props_n1.insert("age".to_string(), Value::from(28));
    props_n1.insert("score".to_string(), Value::from(95.5));
    props_n1.insert("is_active".to_string(), Value::from(true));

    let mut labels_n1 = HashSet::new();
    labels_n1.insert("User".to_string());
    labels_n1.insert("Admin".to_string());

    let n1 = db.add_node(labels_n1, props_n1)?;
    assert_eq!(db.node_count(), 1);

    let node1 = db.get_node(n1).expect("Node 1 should exist");
    assert_eq!(node1.id, n1);
    assert!(node1.has_label("User"));
    assert!(node1.has_label("Admin"));
    assert_eq!(
        node1.get_prop("name").and_then(|v| v.as_str()),
        Some("Alice")
    );
    assert_eq!(node1.get_prop("age").and_then(|v| v.as_i64()), Some(28));
    assert_eq!(node1.get_prop("score").and_then(|v| v.as_f64()), Some(95.5));
    assert_eq!(
        node1.get_prop("is_active").and_then(|v| v.as_bool()),
        Some(true)
    );

    // 2. 添加第二个节点
    let mut props_n2 = HashMap::new();
    props_n2.insert("name".to_string(), Value::from("Bob"));
    props_n2.insert("age".to_string(), Value::from(30));
    let mut labels_n2 = HashSet::new();
    labels_n2.insert("User".to_string());
    let n2 = db.add_node(labels_n2, props_n2)?;
    assert_eq!(db.node_count(), 2);

    // 3. 更新节点属性
    db.update_node_property(n1, "age", 29)?;
    db.update_node_property(n1, "city", "Beijing")?;
    let updated_n1 = db.get_node(n1).unwrap();
    assert_eq!(
        updated_n1.get_prop("age").and_then(|v| v.as_i64()),
        Some(29)
    );
    assert_eq!(
        updated_n1.get_prop("city").and_then(|v| v.as_str()),
        Some("Beijing")
    );

    // 4. 添加边并验证免索引邻接 (Index-free Adjacency)
    let mut edge_props = HashMap::new();
    edge_props.insert("since".to_string(), Value::from(2023));
    let e1 = db.add_edge(n1, n2, "FOLLOWS", edge_props, 2.5)?;
    assert_eq!(db.edge_count(), 1);

    let edge = db.get_edge(e1).expect("Edge 1 should exist");
    assert_eq!(edge.src_id, n1);
    assert_eq!(edge.dst_id, n2);
    assert_eq!(edge.edge_type, "FOLLOWS");
    assert_eq!(edge.weight, 2.5);
    assert_eq!(edge.get_prop("since").and_then(|v| v.as_i64()), Some(2023));

    // 验证邻接列表
    let src_node = db.get_node(n1).unwrap();
    let dst_node = db.get_node(n2).unwrap();
    assert!(src_node.outgoing.contains(&e1));
    assert!(dst_node.incoming.contains(&e1));

    // 5. 更新边属性
    db.update_edge_property(e1, "since", 2024)?;
    let updated_edge = db.get_edge(e1).unwrap();
    assert_eq!(
        updated_edge.get_prop("since").and_then(|v| v.as_i64()),
        Some(2024)
    );

    // 6. 删除边并验证对端邻接列表清理
    let removed_edge = db.remove_edge(e1)?;
    assert_eq!(removed_edge.id, e1);
    assert_eq!(db.edge_count(), 0);

    let src_after = db.get_node(n1).unwrap();
    let dst_after = db.get_node(n2).unwrap();
    assert!(!src_after.outgoing.contains(&e1));
    assert!(!dst_after.incoming.contains(&e1));

    // 7. 级联删除节点：先重新建立边再删除节点
    let e2 = db.add_edge(n1, n2, "FOLLOWS", HashMap::new(), 1.0)?;
    assert_eq!(db.edge_count(), 1);
    let removed_n1 = db.remove_node(n1)?;
    assert_eq!(removed_n1.id, n1);
    assert_eq!(db.node_count(), 1);
    assert_eq!(db.edge_count(), 0); // 关联边已被级联删除

    let remaining_dst = db.get_node(n2).unwrap();
    assert!(!remaining_dst.incoming.contains(&e2));

    Ok(())
}

// =========================================================================
// 测试 2: 复杂社交关系网络多跳遍历测试（如 2 度好友推荐）
// =========================================================================
#[test]
fn test_02_social_network_multi_hop_traversal() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("social_test.db");
    let db = NervusDb::open(&db_path)?;

    // 构建社交图谱:
    // Alice(1) -> Bob(2), Charlie(3)
    // Bob(2) -> Dave(4), Eve(5)
    // Charlie(3) -> Eve(5), Frank(6)
    // Frank(6) -> Grace(7)
    let make_person = |name: &str, age: i64, city: &str| {
        let mut props = HashMap::new();
        props.insert("name".to_string(), Value::from(name));
        props.insert("age".to_string(), Value::from(age));
        props.insert("city".to_string(), Value::from(city));
        let mut labels = HashSet::new();
        labels.insert("Person".to_string());
        (labels, props)
    };

    let (l, p) = make_person("Alice", 25, "Shanghai");
    let alice = db.add_node(l, p)?;
    let (l, p) = make_person("Bob", 28, "Beijing");
    let bob = db.add_node(l, p)?;
    let (l, p) = make_person("Charlie", 26, "Shanghai");
    let charlie = db.add_node(l, p)?;
    let (l, p) = make_person("Dave", 31, "Beijing");
    let dave = db.add_node(l, p)?;
    let (l, p) = make_person("Eve", 22, "Shanghai");
    let eve = db.add_node(l, p)?;
    let (l, p) = make_person("Frank", 27, "Shenzhen");
    let frank = db.add_node(l, p)?;
    let (l, p) = make_person("Grace", 29, "Guangzhou");
    let grace = db.add_node(l, p)?;

    // 建立单向关注关系
    db.add_edge(alice, bob, "FRIEND", HashMap::new(), 1.0)?;
    db.add_edge(alice, charlie, "FRIEND", HashMap::new(), 1.0)?;
    db.add_edge(bob, dave, "FRIEND", HashMap::new(), 1.0)?;
    db.add_edge(bob, eve, "FRIEND", HashMap::new(), 1.0)?;
    db.add_edge(charlie, eve, "FRIEND", HashMap::new(), 1.0)?;
    db.add_edge(charlie, frank, "FRIEND", HashMap::new(), 1.0)?;
    db.add_edge(frank, grace, "FRIEND", HashMap::new(), 1.0)?;

    // 1. 2度好友遍历 (2 Hops from Alice)
    let res_2hop = db
        .query()
        .traverse(alice, "FRIEND", Direction::Outgoing, 2)
        .execute();

    let target_names: HashSet<String> = res_2hop
        .dst_nodes()
        .iter()
        .filter_map(|n| {
            n.get_prop("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    // Alice 的 2 度好友应该是 Dave, Eve, Frank（不包括直接好友 Bob, Charlie，也不包括 Alice 自己或 Grace）
    assert!(target_names.contains("Dave"));
    assert!(target_names.contains("Eve"));
    assert!(target_names.contains("Frank"));
    assert!(!target_names.contains("Alice"));
    assert!(!target_names.contains("Bob"));
    assert!(!target_names.contains("Charlie"));
    assert!(!target_names.contains("Grace"));

    // 2. 带属性过滤的 2 度好友推荐（例如城市在 Shanghai 的 2 度好友，或者年龄 > 25）
    let res_filtered = db
        .query()
        .traverse(alice, "FRIEND", Direction::Outgoing, 2)
        .filter_dst(|n| n.get_prop("city").and_then(|v| v.as_str()) == Some("Shanghai"))
        .execute();

    let filtered_names: Vec<String> = res_filtered
        .dst_nodes()
        .iter()
        .filter_map(|n| {
            n.get_prop("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    assert_eq!(filtered_names, vec!["Eve".to_string()]);

    // 3. 模式匹配结合属性过滤
    let pattern_matches = db
        .query()
        .match_pattern("Person", "FRIEND", "Person")
        .filter_prop("age", |v| v.as_i64().unwrap_or(0) >= 30)
        .execute();

    // 涉及 age >= 30 的边只有 (Bob:28) -> (Dave:31)
    assert_eq!(pattern_matches.paths().len(), 1);
    let matched_path = &pattern_matches.paths()[0];
    assert_eq!(
        matched_path.src.get_prop("name").and_then(|v| v.as_str()),
        Some("Bob")
    );
    assert_eq!(
        matched_path.dst.get_prop("name").and_then(|v| v.as_str()),
        Some("Dave")
    );

    Ok(())
}

// =========================================================================
// 测试 3: 带权最短路径（Dijkstra）与环路检测算法准确性测试
// =========================================================================
#[test]
fn test_03_dijkstra_and_cycle_detection() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("algo_test.db");
    let db = NervusDb::open(&db_path)?;

    // 构造经典 Dijkstra 测试图：
    // A(1) -> B(2) [weight: 4]
    // A(1) -> C(3) [weight: 2]
    // C(3) -> B(2) [weight: 1]  (A->C->B 权重为 2+1=3 < 4)
    // B(2) -> D(4) [weight: 5]
    // C(3) -> E(5) [weight: 10]
    // B(2) -> E(5) [weight: 3]  (A->C->B->E 权重为 2+1+3=6 < 12)
    // E(5) -> D(4) [weight: 2]  (A->C->B->E->D 权重为 2+1+3+2=8 < 4+5=9)
    let a = db.add_node(HashSet::new(), HashMap::new())?;
    let b = db.add_node(HashSet::new(), HashMap::new())?;
    let c = db.add_node(HashSet::new(), HashMap::new())?;
    let d = db.add_node(HashSet::new(), HashMap::new())?;
    let e = db.add_node(HashSet::new(), HashMap::new())?;

    db.add_edge(a, b, "ROAD", HashMap::new(), 4.0)?;
    db.add_edge(a, c, "ROAD", HashMap::new(), 2.0)?;
    db.add_edge(c, b, "ROAD", HashMap::new(), 1.0)?;
    db.add_edge(b, d, "ROAD", HashMap::new(), 6.0)?;
    db.add_edge(c, e, "ROAD", HashMap::new(), 10.0)?;
    db.add_edge(b, e, "ROAD", HashMap::new(), 3.0)?;
    db.add_edge(e, d, "ROAD", HashMap::new(), 2.0)?;

    // 1. 测试 Dijkstra 最短路径 (A -> D)
    let (cost, path) = db
        .dijkstra(a, d, Some("ROAD"))
        .expect("Should find shortest path");
    assert_eq!(cost, 8.0);
    assert_eq!(path, vec![a, c, b, e, d]);

    // 2. 测试无权 BFS 最短路径 (寻找最少跳数，A -> B -> D 仅需 2 跳)
    let bfs_path = db.bfs(a, d, Some("ROAD")).expect("Should find BFS path");
    assert_eq!(bfs_path.len(), 3);
    assert_eq!(bfs_path, vec![a, b, d]);

    // 3. 环路检测测试：当前为有向无环图 (DAG)
    assert!(!db.has_cycle());
    assert!(db.find_cycles().is_empty());

    // 添加一条边形成回路: D -> A
    let loop_edge = db.add_edge(d, a, "ROAD", HashMap::new(), 1.0)?;
    assert!(db.has_cycle());
    let cycles = db.find_cycles();
    assert!(!cycles.is_empty());
    // 验证环路闭合
    let first_cycle = &cycles[0];
    assert_eq!(first_cycle.first(), first_cycle.last());

    // 移除回路边，应恢复为无环图
    db.remove_edge(loop_edge)?;
    assert!(!db.has_cycle());

    Ok(())
}

// =========================================================================
// 测试 4: 事务原子性与 Rollback 测试（回滚后图状态彻底复原）
// =========================================================================
#[test]
fn test_04_transaction_atomicity_and_rollback() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("tx_test.db");
    let db = NervusDb::open(&db_path)?;

    // 初始状态：插入一个基准节点
    let base_node = db.add_node(HashSet::new(), HashMap::new())?;
    assert_eq!(db.node_count(), 1);
    assert_eq!(db.edge_count(), 0);

    // 1. 开启事务，执行一系列增删改，然后主动 Rollback
    let mut tx = db.begin_transaction()?;
    let n2 = tx.add_node(HashSet::new(), HashMap::new())?;
    let n3 = tx.add_node(HashSet::new(), HashMap::new())?;
    tx.add_edge(base_node, n2, "REL", HashMap::new(), 1.0)?;
    tx.add_edge(n2, n3, "REL", HashMap::new(), 2.0)?;
    tx.update_node_property(base_node, "temp_prop", "temporary_val")?;

    // 主动回滚
    tx.rollback()?;

    // 断言：图状态彻底复原，无任何改动
    assert_eq!(db.node_count(), 1);
    assert_eq!(db.edge_count(), 0);
    assert!(db.get_node(n2).is_none());
    assert!(db.get_node(n3).is_none());
    let re_read_base = db.get_node(base_node).unwrap();
    assert!(re_read_base.get_prop("temp_prop").is_none());

    // 2. 模拟事务对象未调用 commit 直接 drop（隐式回滚）
    {
        let mut uncommitted_tx = db.begin_transaction()?;
        uncommitted_tx.add_node(HashSet::new(), HashMap::new())?;
        uncommitted_tx.add_node(HashSet::new(), HashMap::new())?;
        // 离开作用域自动触发 Drop
    }
    assert_eq!(db.node_count(), 1);

    // 3. 正常提交事务 Commit
    let mut commit_tx = db.begin_transaction()?;
    let c1 = commit_tx.add_node(HashSet::new(), HashMap::new())?;
    commit_tx.add_edge(base_node, c1, "VALID", HashMap::new(), 3.0)?;
    commit_tx.commit()?;

    assert_eq!(db.node_count(), 2);
    assert_eq!(db.edge_count(), 1);
    assert!(db.get_node(c1).is_some());

    Ok(())
}

// =========================================================================
// 测试 5: 模拟突发断电崩溃测试（不关闭直接销毁实例，新实例从 WAL 完整恢复 100% 数据）
// =========================================================================
#[test]
fn test_05_crash_recovery_from_wal() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("crash_recovery.db");

    let node_a_id;
    let node_b_id;
    let edge_ab_id;

    // 作用域 1：创建数据库，写入关键业务数据，不调用优雅退出或 checkpoint，模拟突发断电
    {
        let db = NervusDb::open(&db_path)?;

        let mut a_props = HashMap::new();
        a_props.insert("name".to_string(), Value::from("ServerNodeA"));
        a_props.insert("ip".to_string(), Value::from("192.168.1.100"));
        let mut a_labels = HashSet::new();
        a_labels.insert("Server".to_string());
        node_a_id = db.add_node(a_labels, a_props)?;

        let mut b_props = HashMap::new();
        b_props.insert("name".to_string(), Value::from("ServerNodeB"));
        b_props.insert("ip".to_string(), Value::from("192.168.1.101"));
        let mut b_labels = HashSet::new();
        b_labels.insert("Server".to_string());
        node_b_id = db.add_node(b_labels, b_props)?;

        let mut edge_props = HashMap::new();
        edge_props.insert("bandwidth".to_string(), Value::from(10000));
        edge_ab_id = db.add_edge(node_a_id, node_b_id, "CONNECTS", edge_props, 0.5)?;

        // 使用事务再写入一个节点与关联边
        let mut tx = db.begin_transaction()?;
        let mut c_props = HashMap::new();
        c_props.insert("name".to_string(), Value::from("ServerNodeC"));
        let c_id = tx.add_node(HashSet::new(), c_props)?;
        tx.add_edge(node_b_id, c_id, "CONNECTS", HashMap::new(), 0.8)?;
        tx.commit()?;

        // 模拟突发断电：直接 drop(db)
        drop(db);
    }

    // 模拟灾难现场：在 WAL 文件末尾追加损坏的垃圾半帧数据（模拟写入到一半突发断电导致帧残缺）
    //
    // 这里**不能**用 `db.wal_path()`：`db` 在上面的作用域末尾被刻意 drop 掉了
    // （模拟突发断电），句柄已不存在。因此只能按同一条规则手工拼出路径——
    // 这是本测试唯一必须这么做的地方，其余测试一律用访问器。
    let wal_path = {
        let mut s = db_path.as_os_str().to_os_string();
        s.push(".wal");
        s
    };
    {
        let mut wal_file = OpenOptions::new().append(true).open(&wal_path)?;
        // 写入不完整的半个魔数或损坏的帧
        wal_file.write_all(b"GWAL\x05\x00\x00\x00\x12\x34\x56\x78BAD_PAYLOAD_TRUNCATED")?;
        wal_file.flush()?;
    }

    // 作用域 2：重启系统，由新实例从 WAL 自动自愈回放恢复
    {
        let recovered_db = NervusDb::open(&db_path)?;

        // 验证全部 3 个节点和 2 条边 100% 完整复原，垃圾帧被安全丢弃
        assert_eq!(recovered_db.node_count(), 3);
        assert_eq!(recovered_db.edge_count(), 2);

        let node_a = recovered_db
            .get_node(node_a_id)
            .expect("Node A must be recovered");
        assert_eq!(
            node_a.get_prop("name").and_then(|v| v.as_str()),
            Some("ServerNodeA")
        );
        assert_eq!(
            node_a.get_prop("ip").and_then(|v| v.as_str()),
            Some("192.168.1.100")
        );

        let node_b = recovered_db
            .get_node(node_b_id)
            .expect("Node B must be recovered");
        assert_eq!(
            node_b.get_prop("name").and_then(|v| v.as_str()),
            Some("ServerNodeB")
        );

        let edge = recovered_db
            .get_edge(edge_ab_id)
            .expect("Edge AB must be recovered");
        assert_eq!(edge.src_id, node_a_id);
        assert_eq!(edge.dst_id, node_b_id);
        assert_eq!(edge.weight, 0.5);
        assert_eq!(
            edge.get_prop("bandwidth").and_then(|v| v.as_i64()),
            Some(10000)
        );

        // 验证恢复后的免索引邻接关系完全正确
        assert!(node_a.outgoing.contains(&edge_ab_id));
        assert!(node_b.incoming.contains(&edge_ab_id));

        // 验证恢复后能够继续进行正常的读写和自增 ID 分配
        let node_d_id = recovered_db.add_node(HashSet::new(), HashMap::new())?;
        assert!(node_d_id > node_b_id);
        assert_eq!(recovered_db.node_count(), 4);
    }

    Ok(())
}

// =========================================================================
// 测试 6: 20 个 std::thread 并发读写与遍历压力测试（零死锁、零数据竞争）
// =========================================================================
#[test]
fn test_06_concurrent_read_write_stress_test() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("concurrent_stress.db");
    let db = NervusDb::open(&db_path)?;

    // 预热图数据：创建中心枢纽节点
    let hub_node = db.add_node(HashSet::new(), HashMap::new())?;

    let total_threads = 20;
    let ops_per_thread = 50;

    let write_success_counter = Arc::new(AtomicUsize::new(0));
    let read_success_counter = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::with_capacity(total_threads);

    for thread_idx in 0..total_threads {
        let db_clone = db.clone();
        let write_cnt = Arc::clone(&write_success_counter);
        let read_cnt = Arc::clone(&read_success_counter);

        let handle = thread::spawn(move || {
            let is_writer = thread_idx % 2 == 0; // 10 个写线程，10 个读线程

            if is_writer {
                for i in 0..ops_per_thread {
                    let mut props = HashMap::new();
                    props.insert(
                        "thread_origin".to_string(),
                        Value::from(format!("T{}-{}", thread_idx, i)),
                    );
                    props.insert("val".to_string(), Value::from(i as i64));

                    let mut labels = HashSet::new();
                    labels.insert("WorkerNode".to_string());

                    // 交替使用事务写入与单操作写入
                    if i % 2 == 0 {
                        let mut tx = db_clone.begin_transaction().expect("Begin tx failed");
                        let new_node = tx.add_node(labels, props).expect("Tx add node failed");
                        tx.add_edge(hub_node, new_node, "DISPATCH", HashMap::new(), 1.0)
                            .expect("Tx add edge failed");
                        tx.commit().expect("Tx commit failed");
                    } else {
                        let new_node = db_clone
                            .add_node(labels, props)
                            .expect("Direct add node failed");
                        let _ =
                            db_clone.add_edge(new_node, hub_node, "REPORT", HashMap::new(), 0.5);
                    }

                    write_cnt.fetch_add(1, Ordering::SeqCst);
                }
            } else {
                for _ in 0..ops_per_thread {
                    // 并发执行只读遍历与模式匹配
                    let res = db_clone
                        .query()
                        .match_pattern("WorkerNode", "REPORT", "")
                        .execute();
                    let _ = res.paths().len();

                    // 并发执行算法
                    let _ = db_clone.has_cycle();

                    read_cnt.fetch_add(1, Ordering::SeqCst);
                }
            }
        });

        handles.push(handle);
    }

    // 等待全部 20 个线程正常 join，验证零死锁
    for handle in handles {
        handle.join().expect("Worker thread panicked!");
    }

    let total_writes = write_success_counter.load(Ordering::SeqCst);
    let total_reads = read_success_counter.load(Ordering::SeqCst);

    assert_eq!(total_writes, 10 * ops_per_thread);
    assert_eq!(total_reads, 10 * ops_per_thread);

    // 校验最终图数据：1 个中心节点 + 500 个写操作生成的节点 = 501 个节点
    assert_eq!(db.node_count(), 1 + 10 * ops_per_thread);

    // 执行一次全量 Checkpoint
    db.checkpoint()?;

    // 验证 Checkpoint 后数据一致无损
    assert_eq!(db.node_count(), 1 + 10 * ops_per_thread);

    Ok(())
}

// =========================================================================
// 测试 7: 4KB 缓冲池受限大图压测 (2MB 限制，20,000 节点，50,000 边高频换页)
// =========================================================================
#[test]
fn test_07_buffer_pool_eviction_large_graph_stress() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("large_graph.db");

    // 严格限制 Buffer Pool 为 512 帧 (512 * 4KB = 2MB 内存硬约束)
    let db = NervusDb::open_with_pool_size(&db_path, 512)?;

    let num_nodes: u64 = 20_000;
    let num_edges: usize = 50_000;

    // 1. 事务批量插入 20,000 个节点
    let mut tx = db.begin_transaction()?;
    for i in 1..=num_nodes {
        let mut props = HashMap::new();
        props.insert("idx".to_string(), Value::from(i as i64));
        props.insert("val".to_string(), Value::from((i * 3) as i64));
        let mut labels = HashSet::new();
        labels.insert("LargeNode".to_string());
        tx.add_node(labels, props)?;
    }
    tx.commit()?;
    assert_eq!(db.node_count(), num_nodes as usize);

    // 2. 批量建立 50,000 条边
    let mut tx_edge = db.begin_transaction()?;
    // 首先建立主拓扑链 1 -> 2 -> ... -> 20000
    for i in 1..num_nodes {
        tx_edge.add_edge(i, i + 1, "NEXT", HashMap::new(), 1.0)?;
    }
    // 建立跨度连接
    for i in 1..=num_nodes {
        let jump = (i + 17) % num_nodes + 1;
        if i != jump {
            tx_edge.add_edge(i, jump, "JUMP", HashMap::new(), 2.5)?;
        }
    }
    // 补充剩余边数
    let current_edges = (num_nodes - 1) as usize + num_nodes as usize;
    let remaining = num_edges.saturating_sub(current_edges);
    for i in 0..remaining {
        let src = ((i * 13) as u64 % num_nodes) + 1;
        let dst = ((i * 37) as u64 % num_nodes) + 1;
        if src != dst {
            let _ = tx_edge.add_edge(src, dst, "CROSS", HashMap::new(), 3.0);
        }
    }
    tx_edge.commit()?;
    assert!(db.edge_count() >= 39_000);

    // 3. 验证换页机制运转：容量仅 512 页，插入数据远超 512 页，产生大量磁盘写入
    let stats = db.buffer_stats();
    assert_eq!(stats.capacity_frames, 512);

    // 4. 在 2MB 限制下执行多跳遍历
    let res = db
        .query()
        .traverse(1, "NEXT", Direction::Outgoing, 3)
        .execute();
    assert!(!res.multi_hop_paths().is_empty());

    // 5. 在 2MB 限制下执行 Dijkstra 最短路径算法 (1 -> 50)
    let dijkstra_res = db.dijkstra(1, 50, Some("NEXT"));
    assert!(dijkstra_res.is_some());
    let (cost, path) = dijkstra_res.unwrap();
    assert_eq!(cost, 49.0);
    assert_eq!(path.len(), 50);

    // 6. 随机抽样读取节点，验证频繁置换下数据属性的绝对准确
    let node_1234 = db.get_node(1234).expect("Node 1234 should exist");
    assert_eq!(
        node_1234.get_prop("val").and_then(|v| v.as_i64()),
        Some(1234 * 3)
    );

    let node_19999 = db.get_node(19999).expect("Node 19999 should exist");
    assert_eq!(
        node_19999.get_prop("val").and_then(|v| v.as_i64()),
        Some(19999 * 3)
    );

    Ok(())
}

// =========================================================================
// 测试 8: 原生 Cypher 字符串查询引擎端到端测试
// =========================================================================
#[test]
fn test_08_cypher_engine_end_to_end() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("cypher_test.db");
    let db = NervusDb::open(&db_path)?;

    // 1. CREATE 复杂路径模式
    let create_sql = "CREATE (a:Person {name: 'Alice', age: 28})-[:KNOWS {weight: 1.5}]->(b:Person {name: 'Bob', age: 32})";
    let res = db.execute(create_sql)?;
    assert_eq!(res.nodes_created, 2);
    assert_eq!(res.edges_created, 1);

    // 2. CREATE 独立单节点
    db.execute("CREATE (c:Person {name: 'Charlie', age: 18})")?;
    db.execute("CREATE (d:Person {name: 'David', age: 40})")?;

    assert_eq!(db.node_count(), 4);

    // 3. MATCH ... WHERE ... RETURN ... LIMIT 查询
    let query_res = db.query_cypher(
        "MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE b.age > 30 RETURN a.name, b.name, b.age LIMIT 10",
    )?;

    assert_eq!(query_res.row_count(), 1);
    assert_eq!(query_res.columns, vec!["a.name", "b.name", "b.age"]);
    let row = &query_res.rows[0];
    assert_eq!(row.values[0], Value::from("Alice"));
    assert_eq!(row.values[1], Value::from("Bob"));
    assert_eq!(row.values[2], Value::from(32));

    // 4. MATCH + DETACH DELETE 级联删除
    let del_res = db.execute("MATCH (c:Person {name: 'Charlie'}) DETACH DELETE c")?;
    assert_eq!(del_res.nodes_deleted, 1);
    assert_eq!(db.node_count(), 3);

    Ok(())
}

// =========================================================================
// 测试 9: 二级索引加速有效性测试 (Label Index & Property Index)
// =========================================================================
#[test]
fn test_09_secondary_index_acceleration() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("index_test.db");
    let db = NervusDb::open(&db_path)?;

    // 插入 500 个带索引节点
    for i in 1..=500 {
        let mut props = HashMap::new();
        props.insert("username".to_string(), Value::from(format!("user_{}", i)));
        props.insert("score".to_string(), Value::from(i as i64 * 10));
        let mut labels = HashSet::new();
        labels.insert("Member".to_string());
        db.add_node(labels, props)?;
    }

    // 验证二级索引自动注册
    assert!(db.index_labels().contains(&"Member".to_string()));
    assert!(db
        .index_properties()
        .contains(&("Member".to_string(), "username".to_string())));

    // 通过 Cypher 执行属性等值定位 (命中属性索引 O(1) 检索)
    let res =
        db.query_cypher("MATCH (m:Member {username: 'user_250'}) RETURN m.username, m.score")?;

    assert_eq!(res.row_count(), 1);
    assert_eq!(res.rows[0].values[0], Value::from("user_250"));
    assert_eq!(res.rows[0].values[1], Value::from(2500));

    // 属性更新后索引自适应更新
    let target_node = db
        .query_cypher("MATCH (m:Member {username: 'user_1'}) RETURN m.username")?
        .rows[0]
        .values[0]
        .clone();
    assert_eq!(target_node, Value::from("user_1"));

    db.update_node_property(1, "username", "admin_vip")?;
    let res_updated =
        db.query_cypher("MATCH (m:Member {username: 'admin_vip'}) RETURN m.username")?;
    assert_eq!(res_updated.row_count(), 1);

    Ok(())
}

// =========================================================================
// 测试 10: 纯磁盘真外存压测 (1MB 极小内存限制，10,000 节点，20,000 边)
// =========================================================================
#[test]
fn test_10_pure_out_of_core_stress() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("pure_out_of_core.db");

    // 严苛限制：仅允许 256 个 4KB 页帧（总驻留内存硬约束为 1MB）
    let db = NervusDb::open_with_pool_size(&db_path, 256)?;

    let total_nodes: u64 = 10_000;

    // 1. 批量插入 10,000 个节点
    let mut tx = db.begin_transaction()?;
    for i in 1..=total_nodes {
        let mut props = HashMap::new();
        props.insert("idx".to_string(), Value::from(i as i64));
        props.insert("metric".to_string(), Value::from((i * 7) as i64));
        let mut labels = HashSet::new();
        labels.insert("OutCoreNode".to_string());
        tx.add_node(labels, props)?;
    }
    tx.commit()?;
    assert_eq!(db.node_count(), total_nodes as usize);

    // 2. 批量建立 20,000 条边
    let mut tx_edge = db.begin_transaction()?;
    for i in 1..total_nodes {
        tx_edge.add_edge(i, i + 1, "PIPE", HashMap::new(), 1.0)?;
    }
    // 跨度跳跃连接
    for i in 1..=total_nodes {
        let target = (i + 31) % total_nodes + 1;
        if i != target {
            let _ = tx_edge.add_edge(i, target, "CROSS", HashMap::new(), 2.0);
        }
    }
    tx_edge.commit()?;
    assert!(db.edge_count() >= 19_000);

    // 3. 验证 Buffer Pool 内存受控性：容量绝不超过 256 帧（1MB）
    let stats = db.buffer_stats();
    assert_eq!(stats.capacity_frames, 256);
    assert!(stats.used_frames <= 256);

    // 4. 在 1MB 内存限制下执行纯磁盘 Cypher 模式匹配查询
    let query_res = db.query_cypher(
        "MATCH (a:OutCoreNode)-[:PIPE]->(b:OutCoreNode) RETURN a.idx, b.idx LIMIT 5;",
    )?;
    assert_eq!(query_res.row_count(), 5);

    // 5. 在 1MB 内存限制下执行 Dijkstra 最短路径算法 (1 -> 100)
    let sp = db.dijkstra(1, 100, Some("PIPE"));
    assert!(sp.is_some());
    let (cost, path) = sp.unwrap();
    assert_eq!(cost, 99.0);
    assert_eq!(path.len(), 100);

    // 6. 执行全量 Checkpoint 并验证数据完整性
    db.checkpoint()?;
    assert_eq!(db.node_count(), total_nodes as usize);

    Ok(())
}

#[test]
fn test_sqlite_compact_file_size() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("compact.db");

    // 1. 初始化仅含 1 个节点 1 条边的全新数据库
    {
        let db = NervusDb::open(&db_path)?;
        let mut props = HashMap::new();
        props.insert("name".to_string(), Value::from("Alice"));
        let mut labels = HashSet::new();
        labels.insert("Person".to_string());
        let n1 = db.add_node(labels, props)?;

        let _ = db.add_edge(n1, n1, "SELF", HashMap::new(), 1.0)?;
        db.checkpoint()?;
    }

    // 2. 检验磁盘文件实际物理大小：严格必须小于等于 16KB (杜绝 600MB 稀疏物理空洞)
    let file_len = std::fs::metadata(&db_path)?.len();
    assert!(
        file_len <= 16 * 1024,
        "Database file size is {} bytes, which strictly exceeds 16KB limit!",
        file_len
    );
    assert!(file_len > 0);

    Ok(())
}

#[test]
fn test_12_transaction_rollback_and_crash_consistency() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("rollback_consistency.db");

    // 1. 打开数据库并记录初始状态
    let initial_node_count;
    {
        let db = NervusDb::open(&db_path)?;
        let mut p = HashMap::new();
        p.insert("init".to_string(), Value::from(1));
        let mut l = HashSet::new();
        l.insert("Base".to_string());
        db.add_node(l, p)?;
        initial_node_count = db.node_count();
        assert_eq!(initial_node_count, 1);

        // 开启事务尝试写入一批数据，然后显式 rollback
        let mut tx = db.begin_transaction()?;
        for i in 1..=50 {
            let mut props = HashMap::new();
            props.insert("temp_val".to_string(), Value::from(i));
            let mut labels = HashSet::new();
            labels.insert("TempNode".to_string());
            tx.add_node(labels, props)?;
        }
        tx.rollback()?;

        // 回滚后验证内存状态与自增序列
        assert_eq!(db.node_count(), initial_node_count);
    }

    // 2. 冷重启重新打开数据库，验证物理磁盘零污染
    {
        let db = NervusDb::open(&db_path)?;
        assert_eq!(db.node_count(), initial_node_count);

        // 再次添加节点，其 ID 单调递增分配，未提交事务分配的临时 ID 作废不回拨
        let mut p = HashMap::new();
        p.insert("second".to_string(), Value::from(2));
        let mut l = HashSet::new();
        l.insert("Base".to_string());
        let n2 = db.add_node(l, p)?;
        assert!(
            n2 > initial_node_count as u64,
            "Sequence number must be monotonic and not rewound"
        );
        assert_eq!(db.node_count(), 2);
    }

    Ok(())
}

#[test]
fn test_13_secondary_index_stale_read_and_start_pattern_filtering() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("index_filtering.db");
    let db = NervusDb::open(&db_path)?;

    // 1. 创建节点 (n:User {account: "old_val", role: "admin"})
    let mut props = HashMap::new();
    props.insert("account".to_string(), Value::from("old_val"));
    props.insert("role".to_string(), Value::from("admin"));
    let mut labels = HashSet::new();
    labels.insert("User".to_string());
    let uid = db.add_node(labels, props)?;

    // 2. 更新属性 account = "new_val"
    db.update_node_property(uid, "account", "new_val")?;

    // 3. 验证旧索引项已被剔除：查询旧值必须返回 0 行
    let res_old = db.query_cypher("MATCH (n:User {account: 'old_val'}) RETURN n.account")?;
    assert_eq!(
        res_old.row_count(),
        0,
        "Stale index read detected: old property value still returned!"
    );

    // 查询新值必须命中 1 行
    let res_new = db.query_cypher("MATCH (n:User {account: 'new_val'}) RETURN n.account")?;
    assert_eq!(res_new.row_count(), 1);
    assert_eq!(res_new.rows[0].values[0], Value::from("new_val"));

    // 4. 验证模式匹配起始节点过滤：role 未建立索引，查询 role: "guest" 必须严格过滤返回 0 行
    let res_role_mismatch = db.query_cypher("MATCH (n:User {role: 'guest'}) RETURN n.account")?;
    assert_eq!(
        res_role_mismatch.row_count(),
        0,
        "Start node inline pattern filtering failed: role was not checked!"
    );

    let res_role_match = db.query_cypher("MATCH (n:User {role: 'admin'}) RETURN n.account")?;
    assert_eq!(res_role_match.row_count(), 1);

    Ok(())
}

#[test]
fn test_14_chained_overflow_pages_and_generic_projection() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("chained_overflow.db");
    let db = NervusDb::open(&db_path)?;

    // 1. 构造一个超过 10KB 的超长字符串属性 (单页 4096 字节，至少占用 3 个溢出物理页)
    let large_text = "NervusDbEngineRobustMultiPagePayload".repeat(300); // ~11KB
    assert!(large_text.len() > 10_000);

    let mut props = HashMap::new();
    props.insert("title".to_string(), Value::from("LargeDoc"));
    props.insert("content".to_string(), Value::from(large_text.clone()));
    let mut labels = HashSet::new();
    labels.insert("Document".to_string());

    let doc_id = db.add_node(labels, props)?;

    // 2. 完整读取节点，验证多页溢出链表无截断反序列化
    let doc_node = db.get_node(doc_id).expect("Doc node must exist");
    assert_eq!(
        doc_node.get_prop("content").and_then(|v| v.as_str()),
        Some(large_text.as_str())
    );

    // 3. 验证通用投影 (无 name 属性实体的 RETURN n 与 RETURN *)
    let res_proj = db.query_cypher("MATCH (d:Document) RETURN d")?;
    assert_eq!(res_proj.row_count(), 1);
    let val_str = res_proj.rows[0].values[0].as_str().unwrap();
    assert!(
        val_str.contains("LargeDoc"),
        "Generic RETURN projection must contain properties JSON"
    );

    let res_all = db.query_cypher("MATCH (d:Document) RETURN *")?;
    assert_eq!(res_all.row_count(), 1);
    assert!(res_all.columns.contains(&"d".to_string()));

    // 4. 删除节点验证多页溢出链表完整回收复用
    let del_node = db.remove_node(doc_id)?;
    assert_eq!(del_node.id, doc_id);

    // 再次插入新节点复用释放的溢出页
    let mut p2 = HashMap::new();
    p2.insert("msg".to_string(), Value::from("RecycledPayload"));
    let mut l2 = HashSet::new();
    l2.insert("Recycled".to_string());
    let new_id = db.add_node(l2, p2)?;
    assert_eq!(new_id, doc_id); // 复用了 freelist 中的节点槽位

    let rec_node = db.get_node(new_id).expect("Recycled node must exist");
    assert_eq!(
        rec_node.get_prop("msg").and_then(|v| v.as_str()),
        Some("RecycledPayload")
    );

    Ok(())
}

#[test]
fn test_in_memory_mode() -> Result<(), GraphError> {
    let db = NervusDb::open(":memory:")?;

    // 1. 基础 CRUD
    let mut p = HashMap::new();
    p.insert("mem_key".to_string(), Value::from("mem_val"));
    let mut l = HashSet::new();
    l.insert("MemNode".to_string());
    let n1 = db.add_node(l.clone(), p.clone())?;
    let n2 = db.add_node(l, p)?;

    let _edge_id = db.add_edge(n1, n2, "MEM_EDGE", HashMap::new(), 2.0)?;
    assert_eq!(db.node_count(), 2);
    assert_eq!(db.edge_count(), 1);

    let node = db.get_node(n1).expect("Memory node should exist");
    assert_eq!(
        node.get_prop("mem_key").and_then(|v| v.as_str()),
        Some("mem_val")
    );

    // 2. 事务支持
    let mut tx = db.begin_transaction()?;
    tx.add_node(HashSet::new(), HashMap::new())?;
    tx.commit()?;
    assert_eq!(db.node_count(), 3);

    // 3. Cypher 执行
    let res = db.query_cypher("MATCH (a:MemNode)-[:MEM_EDGE]->(b:MemNode) RETURN a.mem_key")?;
    assert_eq!(res.row_count(), 1);
    assert_eq!(res.rows[0].values[0], Value::from("mem_val"));

    // 4. 确保文件系统中未生成任何物理文件
    assert!(!std::path::Path::new(":memory:").exists());
    assert!(!std::path::Path::new(":memory:.wal").exists());

    Ok(())
}

#[test]
fn test_mrsw_concurrency() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("mrsw.db");
    let db = NervusDb::open(&db_path)?;

    // 初始数据
    for i in 1..=20 {
        let mut p = HashMap::new();
        p.insert("name".to_string(), Value::from(format!("User_{}", i)));
        let mut l = HashSet::new();
        l.insert("User".to_string());
        db.add_node(l, p)?;
    }

    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let mut handles = Vec::new();

    // 1 个写线程：高频写入数据
    {
        let db_writer = db.clone();
        let running_writer = running.clone();
        handles.push(thread::spawn(move || {
            let mut id_seq = 1000u64;
            while running_writer.load(Ordering::Relaxed) {
                let mut p = HashMap::new();
                p.insert(
                    "name".to_string(),
                    Value::from(format!("NewUser_{}", id_seq)),
                );
                let mut l = HashSet::new();
                l.insert("User".to_string());
                let _ = db_writer.add_node(l, p);
                id_seq += 1;
                thread::sleep(std::time::Duration::from_millis(2));
            }
        }));
    }

    // 8 个并发读线程：高频执行 Cypher MATCH 查询
    let reader_queries = Arc::new(AtomicUsize::new(0));
    for _ in 0..8 {
        let db_reader = db.clone();
        let running_reader = running.clone();
        let q_counter = reader_queries.clone();
        handles.push(thread::spawn(move || {
            while running_reader.load(Ordering::Relaxed) {
                let res = db_reader.query_cypher("MATCH (n:User) RETURN n.name LIMIT 5");
                assert!(res.is_ok(), "Concurrent read query failed!");
                q_counter.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    // 持续运行 3 秒
    thread::sleep(std::time::Duration::from_secs(3));
    running.store(false, Ordering::Relaxed);

    for h in handles {
        h.join().expect("Worker thread panicked!");
    }

    let total_queries = reader_queries.load(Ordering::Relaxed);
    assert!(
        total_queries > 50,
        "Read threads executed {} queries, expected high concurrency",
        total_queries
    );

    Ok(())
}

#[test]
fn test_cypher_variable_hops() -> Result<(), GraphError> {
    let db = NervusDb::open(":memory:")?;

    // 建立链路: (a:Person {name: 'A'}) -> (b:Person {name: 'B'}) -> (c:Person {name: 'C'}) -> (d:Person {name: 'D'}) -> (e:Person {name: 'E'})
    db.execute(
        "CREATE (a:Person {name: 'A'})-[:KNOWS]->(b:Person {name: 'B'})-[:KNOWS]->(c:Person {name: 'C'})-[:KNOWS]->(d:Person {name: 'D'})-[:KNOWS]->(e:Person {name: 'E'})",
    )?;

    // 验证 MATCH (a {name: 'A'})-[:KNOWS*1..3]->(b) RETURN b.name
    let res =
        db.query_cypher("MATCH (a:Person {name: 'A'})-[:KNOWS*1..3]->(b:Person) RETURN b.name")?;

    // 应该匹配 1跳(B), 2跳(C), 3跳(D) 共有 3 个目标节点
    assert_eq!(res.row_count(), 3);
    let names: Vec<_> = res
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"B".to_string()));
    assert!(names.contains(&"C".to_string()));
    assert!(names.contains(&"D".to_string()));
    assert!(
        !names.contains(&"E".to_string()),
        "Hop 4 (E) should not be matched for *1..3"
    );

    Ok(())
}

#[test]
fn test_no_stale_age_name_hardcode() -> Result<(), GraphError> {
    let db = NervusDb::open(":memory:")?;

    // 插入无 name 属性（只有 title, price）的商品实体
    db.execute("CREATE (p:Product {title: 'RustInAction', price: 59.9})")?;

    // 1. RETURN p 投影完整属性字典 JSON
    let res_entity = db.query_cypher("MATCH (p:Product) RETURN p")?;
    assert_eq!(res_entity.row_count(), 1);
    let val_str = res_entity.rows[0].values[0]
        .as_str()
        .expect("Must be string JSON");
    assert!(
        val_str.contains("RustInAction"),
        "Entity projection must contain 'RustInAction'"
    );
    assert!(
        val_str.contains("59.9"),
        "Entity projection must contain price 59.9"
    );

    // 2. RETURN p.title, p.price
    let res_props = db.query_cypher("MATCH (p:Product) RETURN p.title, p.price")?;
    assert_eq!(res_props.row_count(), 1);
    assert_eq!(res_props.rows[0].values[0], Value::from("RustInAction"));
    assert_eq!(res_props.rows[0].values[1], Value::from(59.9));

    // 3. 空属性实体验证：返回 "{}" 而非 "null"
    db.execute("CREATE (e:EmptyNode)")?;
    let res_empty = db.query_cypher("MATCH (e:EmptyNode) RETURN e")?;
    assert_eq!(res_empty.row_count(), 1);
    assert_eq!(res_empty.rows[0].values[0], Value::from("{}"));

    Ok(())
}

#[test]
fn test_c_abi_interface() {
    use nervusdb::{nervusdb_close, nervusdb_execute, nervusdb_free_string, nervusdb_open};
    use std::ffi::{CStr, CString};
    use std::ptr;

    unsafe {
        let mut db_ptr = ptr::null_mut();
        let path = CString::new(":memory:").unwrap();
        let rc = nervusdb_open(path.as_ptr(), &mut db_ptr);
        assert_eq!(rc, 0);
        assert!(!db_ptr.is_null());

        let cypher_create = CString::new("CREATE (u:User {nickname: 'Ferris'})").unwrap();
        let mut result_json = ptr::null_mut();
        let rc_create = nervusdb_execute(db_ptr, cypher_create.as_ptr(), &mut result_json);
        assert_eq!(rc_create, 0);
        assert!(!result_json.is_null());
        nervusdb_free_string(result_json);

        let cypher_query = CString::new("MATCH (u:User) RETURN u.nickname").unwrap();
        let mut query_json = ptr::null_mut();
        let rc_query = nervusdb_execute(db_ptr, cypher_query.as_ptr(), &mut query_json);
        assert_eq!(rc_query, 0);
        assert!(!query_json.is_null());

        let res_str = CStr::from_ptr(query_json).to_str().unwrap();
        assert!(res_str.contains("Ferris"));
        nervusdb_free_string(query_json);

        let rc_close = nervusdb_close(db_ptr);
        assert_eq!(rc_close, 0);
    }
}

#[test]
fn test_true_no_steal_enforcement() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("no_steal.db");

    // 1. 初始化数据库，预写入基准节点并落盘
    {
        let db = NervusDb::open(&db_path)?;
        let mut p = HashMap::new();
        p.insert("base".to_string(), Value::from("v1"));
        let mut l = HashSet::new();
        l.insert("Base".to_string());
        db.add_node(l, p)?;
        db.checkpoint()?;
    }

    // 记录初始物理文件大小与内容
    let original_bytes = std::fs::read(&db_path)?;
    assert!(!original_bytes.is_empty());

    // 2. 以极小缓冲池 (4 帧 = 16KB) 打开数据库
    let db = NervusDb::open_with_pool_size(&db_path, 4)?;

    // 3. 开启事务，构造修改 5 个不同物理页的动作（远超 4 帧容量且全为未提交脏页）
    let mut tx = db.begin_transaction()?;
    for i in 1..=5 {
        let mut p = HashMap::new();
        p.insert(format!("big_key_{}", i), Value::from("A".repeat(3000)));
        let mut l = HashSet::new();
        l.insert("NoSteal".to_string());
        tx.add_node(l, p)?;
    }

    // 4. 提交时触发向 DiskGraph 写入，试图在 4 帧缓冲池中分配第 5 个未提交物理页
    let commit_res = tx.commit();

    // 5. 核心断言：必须被 NO-STEAL 防线严厉拒绝，返回 Buffer pool capacity exceeded 异常
    assert!(
        commit_res.is_err(),
        "Transaction commit must fail due to NO-STEAL enforcement"
    );
    let err_str = commit_res.unwrap_err().to_string();
    assert!(
        err_str.contains("NO-STEAL"),
        "Error message must explicitly mention NO-STEAL, got: {}",
        err_str
    );

    // 6. 物理断电零污染断言：主数据文件绝对未被未提交脏页污染，文件内容与初始 100% 一致！
    let current_bytes = std::fs::read(&db_path)?;
    assert_eq!(
        original_bytes, current_bytes,
        "Database physical file was corrupted by uncommitted dirty pages (STEAL violation)!"
    );

    Ok(())
}

#[test]
fn test_tx_rollback_concurrency_isolation() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("tx_isolation.db");
    let db = NervusDb::open(&db_path)?;

    // 初始节点 ID = 1
    let n1 = db.add_node(HashSet::new(), HashMap::new())?;
    assert_eq!(n1, 1);

    // 线程 1 开启事务分配 ID，暂不提交
    let mut tx1 = db.begin_transaction()?;
    let tx1_nid = tx1.add_node(HashSet::new(), HashMap::new())?;
    assert_eq!(tx1_nid, 2);

    // 线程 2 写入并提交新节点
    let mut props2 = HashMap::new();
    props2.insert("owner".to_string(), Value::from("thread_2"));
    let n2 = db.add_node(HashSet::new(), props2)?;
    assert_eq!(n2, 3);
    assert_eq!(
        db.get_node(n2).unwrap().get_prop("owner").unwrap(),
        &Value::from("thread_2")
    );

    // 线程 1 回滚
    tx1.rollback()?;

    // 线程 3 写入新节点
    let mut props3 = HashMap::new();
    props3.insert("owner".to_string(), Value::from("thread_3"));
    let n3 = db.add_node(HashSet::new(), props3)?;

    // 核心断言：线程 3 分配的 ID 绝不与线程 2 冲突（单调自增不回退），且线程 2 写入的数据完全完好
    assert!(
        n3 >= 4,
        "New node id should be >= 4 (monotonically increasing), got {}",
        n3
    );
    assert_ne!(
        n3, n2,
        "New node id must not conflict with committed node id"
    );

    let node2 = db.get_node(n2).expect("Node 2 must exist and be preserved");
    assert_eq!(node2.get_prop("owner").unwrap(), &Value::from("thread_2"));

    let node3 = db.get_node(n3).expect("Node 3 must exist");
    assert_eq!(node3.get_prop("owner").unwrap(), &Value::from("thread_3"));

    Ok(())
}

#[test]
fn test_cypher_edge_variable_and_loop_traversal() -> Result<(), GraphError> {
    let db = NervusDb::open(":memory:")?;

    // 1. 建立带有权重与属性的关系边: (a)-[r:KNOWS {weight: 9.9, note: 'bff'}]->(b)
    let n1 = db.add_node(HashSet::from(["Person".to_string()]), HashMap::new())?;
    let n2 = db.add_node(HashSet::from(["Person".to_string()]), HashMap::new())?;
    let mut edge_props = HashMap::new();
    edge_props.insert("note".to_string(), Value::from("bff"));
    let _ = db.add_edge(n1, n2, "KNOWS", edge_props, 9.9)?;

    // 查询边变量与边属性: MATCH (a)-[r:KNOWS]->(b) RETURN r, r.weight, r.note
    let res = db.query_cypher("MATCH (a)-[r:KNOWS]->(b) RETURN r, r.weight, r.note")?;
    assert_eq!(res.row_count(), 1);
    let row = &res.rows[0];

    // r 必须返回属性 JSON，绝不可为 null
    let r_str = row.values[0].as_str().expect("r must be JSON string");
    assert!(r_str.contains("bff"), "r JSON must contain 'bff'");

    // r.weight 必须精准返回浮点数 9.9
    assert_eq!(row.values[1], Value::from(9.9));
    assert_eq!(row.values[2], Value::from("bff"));

    // 2. 环路闭环遍历: A -> B -> A
    let db_loop = NervusDb::open(":memory:")?;
    let mut p_a = HashMap::new();
    p_a.insert("name".to_string(), Value::from("A"));
    let a_id = db_loop.add_node(HashSet::from(["Node".to_string()]), p_a)?;

    let mut p_b = HashMap::new();
    p_b.insert("name".to_string(), Value::from("B"));
    let b_id = db_loop.add_node(HashSet::from(["Node".to_string()]), p_b)?;

    db_loop.add_edge(a_id, b_id, "EDGE", HashMap::new(), 1.0)?;
    db_loop.add_edge(b_id, a_id, "EDGE", HashMap::new(), 1.0)?;

    // 2 跳路径展开: MATCH (a:Node {name: 'A'})-[:EDGE*2..2]->(target) RETURN target.name
    let loop_res = db_loop
        .query_cypher("MATCH (a:Node {name: 'A'})-[:EDGE*2..2]->(target) RETURN target.name")?;
    assert_eq!(
        loop_res.row_count(),
        1,
        "Loop traversal should find exactly 1 target at hop 2"
    );
    assert_eq!(loop_res.rows[0].values[0], Value::from("A"));

    Ok(())
}

#[test]
fn test_c_api_errmsg() {
    use nervusdb::{nervusdb_close, nervusdb_errmsg, nervusdb_execute, nervusdb_open};
    use std::ffi::{CStr, CString};
    use std::ptr;

    unsafe {
        let mut db_ptr = ptr::null_mut();
        let path = CString::new(":memory:").unwrap();
        let rc = nervusdb_open(path.as_ptr(), &mut db_ptr);
        assert_eq!(rc, 0);

        // 传入非法 Cypher 语句触发错误
        let cypher_bad = CString::new("INVALID CYPHER QUERY SYNTAX !!!").unwrap();
        let mut result_json = ptr::null_mut();
        let rc_err = nervusdb_execute(db_ptr, cypher_bad.as_ptr(), &mut result_json);
        assert_eq!(rc_err, -1);
        assert!(result_json.is_null());

        // 读取错误详细信息
        let err_ptr = nervusdb_errmsg(db_ptr);
        assert!(!err_ptr.is_null());
        let err_msg = CStr::from_ptr(err_ptr).to_str().unwrap();
        assert!(
            err_msg.contains("Unexpected")
                || err_msg.contains("Invalid")
                || err_msg.contains("syntax")
                || err_msg.contains("General"),
            "Error message must contain diagnostic detail, got: {}",
            err_msg
        );

        nervusdb_close(db_ptr);
    }
}

#[test]
fn test_cold_reboot_index_consistency() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("index_reboot.db");

    // Session 1: 插入 5 个 User {account: "admin"}，正常关闭数据库
    {
        let db = NervusDb::open(&db_path)?;
        for i in 1..=5 {
            let mut props = HashMap::new();
            props.insert("account".to_string(), Value::from("admin"));
            props.insert("uid".to_string(), Value::from(i as i64));
            let mut labels = HashSet::new();
            labels.insert("User".to_string());
            db.add_node(labels, props)?;
        }
        db.checkpoint()?;
    }

    // Session 2: 重新打开数据库，插入第 6 个 User {account: "admin"}
    {
        let db = NervusDb::open(&db_path)?;
        let mut props = HashMap::new();
        props.insert("account".to_string(), Value::from("admin"));
        props.insert("uid".to_string(), Value::from(6i64));
        let mut labels = HashSet::new();
        labels.insert("User".to_string());
        db.add_node(labels, props)?;

        // 执行 MATCH 查询，断言必须精确返回 6 行，历史 5 个节点绝不可隐形蒸发！
        let res = db.query_cypher("MATCH (u:User {account: 'admin'}) RETURN u")?;
        assert_eq!(
            res.row_count(),
            6,
            "Cold reboot index must return all 6 users, not just the newly inserted one"
        );
    }

    Ok(())
}

#[test]
fn test_cypher_delete_edge_safe_isolation() -> Result<(), GraphError> {
    let db = NervusDb::open(":memory:")?;

    // 节点 A (ID=1) 与节点 B (ID=2)，创建一条边连接它们（边 ID 恰好为 1）
    let a = db.add_node(HashSet::from(["Node".to_string()]), HashMap::new())?;
    let b = db.add_node(HashSet::from(["Node".to_string()]), HashMap::new())?;
    let r = db.add_edge(a, b, "KNOWS", HashMap::new(), 1.0)?;
    assert_eq!(a, 1);
    assert_eq!(r, 1);

    // 执行 MATCH (a)-[r:KNOWS]->(b) DELETE r
    let del_res = db.execute("MATCH (a)-[r:KNOWS]->(b) DELETE r")?;
    assert_eq!(del_res.edges_deleted, 1);
    assert_eq!(del_res.nodes_deleted, 0);

    // 断言：边被删除，而节点 1（ID 恰好等于边的 ID）必须完好无损保留！
    assert!(db.get_edge(r).is_none(), "Edge 1 must be deleted");
    assert!(
        db.get_node(a).is_some(),
        "Node 1 must NOT be deleted when deleting edge r=1"
    );
    assert_eq!(db.node_count(), 2);
    assert_eq!(db.edge_count(), 0);

    Ok(())
}

#[test]
fn test_tx_commit_failure_memory_cleanup() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("commit_fail.db");
    let db = NervusDb::open(&db_path)?;

    // 先正常写入 1 个基准节点落盘
    let n1 = db.add_node(HashSet::from(["Base".to_string()]), HashMap::new())?;
    db.checkpoint()?;
    assert_eq!(db.node_count(), 1);

    // 开启事务 1：修改数据并在 commit 过程中故意注入错误（例如负权重边）
    let mut tx1 = db.begin_transaction()?;
    let mut p = HashMap::new();
    p.insert("taint".to_string(), Value::from("dirty_val"));
    let n2 = tx1.add_node(HashSet::from(["TaintNode".to_string()]), p)?;
    // 添加一条非法负权重边，使 commit 时在 apply ops 阶段返回 InvalidWeight 错误
    tx1.add_edge(n1, n2, "BAD_EDGE", HashMap::new(), -5.0)?;

    let commit_res = tx1.commit();
    assert!(
        commit_res.is_err(),
        "Commit must fail due to negative weight"
    );

    // 验证失败后，内存中缓冲池未提交页已被干净剔除
    // 开启事务 2 并正常写入一个新节点并提交
    let mut tx2 = db.begin_transaction()?;
    let mut clean_p = HashMap::new();
    clean_p.insert("clean".to_string(), Value::from("pure_val"));
    tx2.add_node(HashSet::from(["CleanNode".to_string()]), clean_p)?;
    tx2.commit()?;

    // 断言：全库绝对没有夹带上次失败事务的脏数据落盘
    assert_eq!(db.node_count(), 2);
    let taint_search = db.query_cypher("MATCH (t:TaintNode) RETURN t")?;
    assert_eq!(
        taint_search.row_count(),
        0,
        "Failed transaction data must never be committed or visible"
    );

    // 重启数据库验证冷重启一致性
    drop(db);
    let db_reopened = NervusDb::open(&db_path)?;
    assert_eq!(db_reopened.node_count(), 2);
    let taint_search_reopened = db_reopened.query_cypher("MATCH (t:TaintNode) RETURN t")?;
    assert_eq!(taint_search_reopened.row_count(), 0);

    Ok(())
}

// =========================================================================
// #16：链式查询中此前从未被调用的三个构建方法
// =========================================================================

/// `limit` / `filter_src` / `filter_edge` 必须真正生效。
///
/// ## 为什么这条测试存在
///
/// #16 把 `QueryBuilder` 与 `GraphQuery` 两套重复实现合并成一套（原先字段相同、
/// 九个方法逐行等价，只有「借用 vs 拥有」的差别）。合并的**风险**在于转发写错：
/// 某个方法没接到 `builder` 上，调用它就静默变成空操作，而现有测试不会发现——
/// 因为这三个方法全项目**从来没有被调用过**。
///
/// 因此这条测试的价值不是「覆盖一个新功能」，而是**给转发补上可观测点**：
/// 一旦某个方法的转发断掉，这里的断言就会失败。
#[test]
fn test_query_chain_limit_filter_src_filter_edge() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = NervusDb::open(dir.path().join("query_chain.db"))?;

    // 图：hub 出发 3 条边，指向 A / B / C；每条边带 kind 属性
    let hub = db.add_node(HashSet::from(["Hub".to_string()]), {
        let mut p = HashMap::new();
        p.insert("name".to_string(), Value::from("hub"));
        p
    })?;
    for (i, (name, kind)) in [("A", "keep"), ("B", "keep"), ("C", "drop")]
        .iter()
        .enumerate()
    {
        let n = db.add_node(HashSet::from(["Leaf".to_string()]), {
            let mut p = HashMap::new();
            p.insert("name".to_string(), Value::from(*name));
            p.insert("idx".to_string(), Value::from(i as i64));
            p
        })?;
        let mut ep = HashMap::new();
        ep.insert("kind".to_string(), Value::from(*kind));
        db.add_edge(hub, n, "LINK", ep, 1.0)?;
    }

    // 1) traverse 不带筛选：3 条路径
    let all = db
        .query()
        .traverse(hub, "LINK", Direction::Outgoing, 1)
        .execute();
    assert_eq!(all.multi_hop_paths().len(), 3, "all three edges must match");

    // 2) limit=2：必须只返回 2 条 —— 这条断言使 `limit` 的转发可观测
    let limited = db
        .query()
        .traverse(hub, "LINK", Direction::Outgoing, 1)
        .limit(2)
        .execute();
    assert_eq!(
        limited.multi_hop_paths().len(),
        2,
        "`limit` must actually truncate; a broken forward would return all 3"
    );

    // 3) filter_edge：只留 kind == "keep" 的边 —— 共 2 条
    let kept = db
        .query()
        .traverse(hub, "LINK", Direction::Outgoing, 1)
        .filter_edge(|e| e.get_prop("kind").and_then(|v| v.as_str()) == Some("keep"))
        .execute();
    assert_eq!(
        kept.multi_hop_paths().len(),
        2,
        "`filter_edge` must actually filter; a broken forward would return all 3"
    );

    // 4) filter_src + match_pattern：起点名必须是 hub
    let by_src = db
        .query()
        .match_pattern("Hub", "LINK", "Leaf")
        .filter_src(|n| n.get_prop("name").and_then(|v| v.as_str()) == Some("hub"))
        .execute();
    assert_eq!(
        by_src.paths().len(),
        3,
        "`filter_src` must keep matching sources"
    );

    // 5) filter_src 收窄到不存在的名字 → 0 条（证明它真的在筛，而不是恒真）
    let none = db
        .query()
        .match_pattern("Hub", "LINK", "Leaf")
        .filter_src(|n| n.get_prop("name").and_then(|v| v.as_str()) == Some("nope"))
        .execute();
    assert_eq!(
        none.paths().len(),
        0,
        "`filter_src` must be able to reject; a no-op forward would keep all 3"
    );

    Ok(())
}

/// 回归：`filter_src` / `filter_edge` 在 traverse 路径上曾经是**静默空操作**。
///
/// ## 这是一个真实缺陷，不是重构的副产品
///
/// `execute_traverse` 此前只应用 `dst_filters` 与 `prop_filters`，**完全忽略**
/// `src_filters` 与 `edge_filters`（`execute_pattern` 四个都应用）。因此：
///
/// ```text
/// db.query().traverse(hub, "LINK", Outgoing, 1)
///           .filter_edge(|e| e.get_prop("kind") == Some("keep"))
///           .execute()
/// ```
///
/// 会返回**全部** 3 条边，而不是 2 条——筛选被丢弃且不报错。调用方看到的是
/// 「查出来比预期多」，而没有任何信号说明筛选没生效。
///
/// 该缺陷在 `main` 上同样存在（已用 `git show origin/main:src/query.rs` 确认），
/// 是既有问题。它长期存活的**原因**正是 #16 指出的：这两个方法改造前全项目
/// 没有任何调用点，所以没人撞上。
#[test]
fn test_traverse_honors_edge_and_src_filters() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = NervusDb::open(dir.path().join("traverse_filters.db"))?;

    let hub = db.add_node(
        HashSet::from(["Hub".to_string()]),
        HashMap::from([("name".to_string(), Value::from("hub"))]),
    )?;
    for (name, kind) in [("A", "keep"), ("B", "keep"), ("C", "drop")] {
        let n = db.add_node(
            HashSet::from(["Leaf".to_string()]),
            HashMap::from([("name".to_string(), Value::from(name))]),
        )?;
        db.add_edge(
            hub,
            n,
            "LINK",
            HashMap::from([("kind".to_string(), Value::from(kind))]),
            1.0,
        )?;
    }

    // 不加筛选：3 条
    let all = db
        .query()
        .traverse(hub, "LINK", Direction::Outgoing, 1)
        .execute();
    assert_eq!(all.multi_hop_paths().len(), 3, "sanity: three edges exist");

    // 边筛选：只剩 kind == "keep" 的两条
    let kept = db
        .query()
        .traverse(hub, "LINK", Direction::Outgoing, 1)
        .filter_edge(|e| e.get_prop("kind").and_then(|v| v.as_str()) == Some("keep"))
        .execute();
    assert_eq!(
        kept.multi_hop_paths().len(),
        2,
        "`filter_edge` must be applied on the traverse path; returning 3 means it was \
         silently ignored (the pre-fix behaviour)"
    );

    // 起点筛选：hub 满足 → 3 条；换成不存在的名字 → 0 条
    let ok = db
        .query()
        .traverse(hub, "LINK", Direction::Outgoing, 1)
        .filter_src(|n| n.get_prop("name").and_then(|v| v.as_str()) == Some("hub"))
        .execute();
    assert_eq!(
        ok.multi_hop_paths().len(),
        3,
        "matching src filter keeps paths"
    );

    let rejected = db
        .query()
        .traverse(hub, "LINK", Direction::Outgoing, 1)
        .filter_src(|n| n.get_prop("name").and_then(|v| v.as_str()) == Some("other"))
        .execute();
    assert_eq!(
        rejected.multi_hop_paths().len(),
        0,
        "`filter_src` must be applied on the traverse path; returning 3 means it was \
         silently ignored (the pre-fix behaviour)"
    );

    Ok(())
}
// =========================================================================
// #19：把「刻意对外提供」的接口补上可观测点
// =========================================================================
//
// 审出 20 多个公开函数零调用。分诊结论里，这一类**保留**：它们是刻意提供的
// 接口（文档写过、或被 SDK 作为能力入口），只是当时没有测试。删掉它们会移除
// 用户真正需要的能力，所以按 #19 的第三种处置——补测试。
//
// 没有测试的公开 API 与不存在的 API 在实践上很难区分：改错没人发现，删掉也
// 没人发现。这条测试就是那个「有人用」的证据。

/// `Value` 的辅助判定必须与内部表示一致。
#[test]
fn test_value_helpers_match_their_representation() -> Result<(), GraphError> {
    // `Value::null()` 是 `Value::Null` 的构造器，两者必须相等
    assert_eq!(Value::null(), Value::Null);
    assert!(Value::null().is_null());
    assert!(!Value::from(1).is_null());

    // `is_list` 只对 `List` 为真
    assert!(Value::List(vec![Value::from(1)]).is_list());
    assert!(!Value::from(1).is_list());
    assert!(!Value::Null.is_list());
    assert!(!Value::from("x").is_list());

    // `is_storable` 与 `is_list`/`is_null` 的关系：后两者都不可落盘
    assert!(!Value::Null.is_storable());
    assert!(!Value::List(vec![]).is_storable());
    assert!(Value::from(1).is_storable());
    assert!(Value::from("x").is_storable());
    assert!(Value::from(true).is_storable());
    Ok(())
}

/// `VacuumReport::summary` 必须把关键数字都带上（它是给人看的单行摘要）。
#[test]
fn test_vacuum_summary_reports_the_counts() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = NervusDb::open(dir.path().join("vacuum_summary.db"))?;

    // 建一些数据，确保数字不是全零（全零会让「什么都对」）
    for i in 0..10 {
        db.add_node(
            HashSet::from(["V".to_string()]),
            HashMap::from([("i".to_string(), Value::from(i as i64))]),
        )?;
    }
    db.checkpoint()?;

    let report = db.vacuum()?;
    let text = report.summary();

    // 摘要必须包含它承诺的每一项
    for needle in ["node(s)", "edge(s)", "bytes", "reusable property page(s)"] {
        assert!(
            text.contains(needle),
            "vacuum summary must mention `{needle}`, got: {text}"
        );
    }
    // 节点数是实测的 10，摘要里应当出现
    assert!(
        text.contains("10 node"),
        "the summary must report the live node count, got: {text}"
    );
    Ok(())
}

/// `QueryResult` 的取值方法与 `MultiHopPath` 的访问器必须自洽。
#[test]
fn test_query_result_and_path_accessors() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = NervusDb::open(dir.path().join("qr_accessors.db"))?;

    // 链：a -> b -> c，另有 d 作为端点
    db.execute("CREATE (a:T {name: 'a'})-[:R]->(b:T {name: 'b'})-[:R]->(c:T {name: 'c'})")?;
    db.execute("CREATE (d:T {name: 'd'})")?;
    let a = db
        .query_cypher("MATCH (n:T {name: 'a'}) RETURN id(n)")?
        .rows[0]
        .values[0]
        .as_i64()
        .expect("id must be an integer") as u64;

    // --- 单跳：paths() / nodes() / edges() / count() ---
    let single = db.query().match_pattern("T", "R", "T").execute();
    assert_eq!(single.paths().len(), 2, "two :R edges exist in the chain");
    assert_eq!(single.count(), 2, "count() must equal the path count");
    assert_eq!(
        single.edges().len(),
        2,
        "edges() must deduplicate to 2 edges"
    );
    // 涉及 3 个不同节点（a、b、c）
    let node_names: std::collections::BTreeSet<String> = single
        .nodes()
        .iter()
        .filter_map(|n| {
            n.get_prop("name")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .collect();
    assert_eq!(
        node_names,
        ["a", "b", "c"].iter().map(|s| s.to_string()).collect(),
        "nodes() must return every distinct endpoint"
    );
    // src_nodes 只含起点：a 与 b
    let srcs: std::collections::BTreeSet<String> = single
        .src_nodes()
        .iter()
        .filter_map(|n| {
            n.get_prop("name")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .collect();
    assert_eq!(
        srcs,
        ["a", "b"].iter().map(|s| s.to_string()).collect(),
        "src_nodes() must return only the sources"
    );

    // --- 多跳：multi_hop_paths() 与 MultiHopPath 的访问器 ---
    let multi = db
        .query()
        .traverse(a, "R", Direction::Outgoing, 2)
        .execute();
    assert_eq!(
        multi.multi_hop_paths().len(),
        1,
        "only one 2-hop path from a"
    );
    let path = &multi.multi_hop_paths()[0];
    assert_eq!(path.hop_count(), 2, "hop_count() must equal the edge count");
    assert_eq!(
        path.start_node()
            .and_then(|n| n.get_prop("name"))
            .and_then(|v| v.as_str()),
        Some("a"),
        "start_node() must be the traversal start"
    );
    assert_eq!(
        path.end_node()
            .and_then(|n| n.get_prop("name"))
            .and_then(|v| v.as_str()),
        Some("c"),
        "end_node() must be the far end"
    );
    // 多跳结果没有单跳 paths，因此 count() 回落到多跳计数
    assert_eq!(
        multi.count(),
        1,
        "count() must fall back to the multi-hop count"
    );
    assert!(!multi.is_empty());

    // 空结果集的两个取值方法都应给 0 / true
    let empty = db.query().match_pattern("Nope", "R", "Nope").execute();
    assert!(empty.is_empty());
    assert_eq!(empty.count(), 0);
    assert!(empty.nodes().is_empty());

    Ok(())
}

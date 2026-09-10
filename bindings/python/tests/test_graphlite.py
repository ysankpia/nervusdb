"""GraphLite-RS Python SDK 端到端测试。

覆盖：基础 CRUD、Cypher 1.0 全语法（SET / DETACH DELETE / ORDER BY / SKIP / LIMIT / 聚合）、
最短路与环检测、PageRank / 弱连通分量 / K-Hop 子图、Buffer Pool 指标与 Checkpoint。
"""

import os
import tempfile

import graphlite


def test_graphlite_python():
    temp_dir = tempfile.mkdtemp()
    db_path = os.path.join(temp_dir, "py_test.db")

    print(f"Testing GraphLite Python SDK at {db_path}...")
    db = graphlite.GraphLite.open(db_path, 512)

    # 1. API: add_node & add_edge
    alice = db.add_node(["Person"], {"name": "Alice", "age": 28})
    bob = db.add_node(["Person"], {"name": "Bob", "age": 32})
    edge = db.add_edge(alice, bob, "KNOWS", {"since": 2023}, 1.5)

    assert alice > 0
    assert bob > 0
    assert edge > 0
    print(f"Created nodes: alice={alice}, bob={bob}, edge={edge}")

    # 2. Cypher execute
    res = db.execute(
        "CREATE (c:Person {name: 'Charlie', age: 24})-[:KNOWS {weight: 2.0}]->(d:Person {name: 'David', age: 40});"
    )
    assert res["nodes_created"] == 2
    assert res["edges_created"] == 1
    print(f"Cypher execute OK: {res}")

    # 3. Cypher query
    rows = db.query(
        "MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE b.age > 30 RETURN a.name, b.name, b.age;"
    )
    print(f"Cypher query rows: {rows}")
    assert len(rows) >= 1
    for r in rows:
        assert "b.age" in r
        assert r["b.age"] > 30

    # 4. Dijkstra algorithm
    res_sp = db.dijkstra(alice, bob, "KNOWS")
    assert res_sp is not None
    cost, path = res_sp
    assert cost == 1.5
    assert path == [alice, bob]
    print(f"Dijkstra result: cost={cost}, path={path}")

    # 5. BFS & cycle detection
    bfs_path = db.bfs(alice, bob, "KNOWS")
    assert bfs_path == [alice, bob]
    assert db.has_cycle() is False
    print(f"BFS result: {bfs_path}, has_cycle={db.has_cycle()}")

    # 6. SET / 追加标签
    set_res = db.execute("MATCH (a:Person {name: 'Alice'}) SET a.age = 29;")
    assert set_res["properties_set"] == 1
    tagged = db.execute("MATCH (a:Person {name: 'Alice'}) SET a:Admin;")
    assert tagged["properties_set"] == 1
    admin_rows = db.query("MATCH (a:Admin) RETURN a.name;")
    assert len(admin_rows) == 1
    assert admin_rows[0]["a.name"] == "Alice"
    print(f"SET results: {set_res}, {tagged}")

    # 7. ORDER BY / SKIP / LIMIT
    paged = db.query(
        "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.age DESC SKIP 1 LIMIT 2;"
    )
    assert len(paged) == 2
    assert paged[0]["p.age"] >= paged[1]["p.age"]
    print(f"Paging result: {paged}")

    # 8. 聚合函数
    agg = db.query(
        "MATCH (p:Person) RETURN count(p), sum(p.age), avg(p.age), min(p.age), max(p.age);"
    )
    assert agg[0]["count(p)"] == 4
    assert agg[0]["min(p.age)"] == 24
    assert agg[0]["max(p.age)"] == 40
    print(f"Aggregation result: {agg}")

    # 9. PageRank（分数归一、按分数降序）
    ranks = db.pagerank(0.85, 100, 1e-9)
    assert len(ranks) == 4
    total = sum(score for _, score in ranks)
    assert abs(total - 1.0) < 1e-6
    assert all(ranks[i][1] >= ranks[i + 1][1] for i in range(len(ranks) - 1))
    print(f"PageRank result: {ranks}")

    # 10. 弱连通分量（两条互不相连的边 => 2 个分量）
    wcc = db.weakly_connected_components()
    assert len(wcc) == 2, f"expected 2 components, got {wcc}"
    assert any(alice in c and bob in c for c in wcc)
    print(f"WCC result: {wcc}")

    # 11. K-Hop 子图提取
    sub = db.k_hop_subgraph(alice, 1, "outgoing", "KNOWS")
    assert alice in sub["nodes"]
    assert len(sub["nodes"]) == 2
    assert len(sub["edges"]) == 1
    print(f"K-Hop subgraph: {sub}")

    # 12. DETACH DELETE 级联删除
    del_res = db.execute("MATCH (d:Person {name: 'David'}) DETACH DELETE d;")
    assert del_res["nodes_deleted"] == 1
    assert del_res["edges_deleted"] == 1
    print(f"DETACH DELETE result: {del_res}")

    # 13. 图模式 introspect 与 dump
    assert "Person" in db.labels()
    assert "KNOWS" in db.edge_types()
    dump = db.dump_cypher()
    assert "CREATE" in dump and "Alice" in dump
    print(f"Labels: {db.labels()}, EdgeTypes: {db.edge_types()}")

    # 14. 显式批量事务：一次 commit 恰好一次 fsync
    fsync_before = db.stats()["wal_fsync_count"]
    batch = 20_000
    with db.begin_transaction() as tx:
        for i in range(batch):
            tx.add_node(["Batch"], {"idx": i, "name": f"bulk-{i}"})
    after = db.stats()
    fsync_delta = after["wal_fsync_count"] - fsync_before
    assert fsync_delta == 1, f"batch commit must fsync exactly once, got {fsync_delta}"
    assert len(db.query("MATCH (t:Batch) RETURN t;")) == batch
    print(f"Batch transaction OK: {batch} nodes, fsync delta = {fsync_delta}")

    # 15. 事务回滚零污染
    tx = db.begin_transaction()
    for i in range(1_000):
        tx.add_node(["Taint"], {"idx": i})
    tx.rollback()
    assert len(db.query("MATCH (t:Taint) RETURN t;")) == 0
    print("Transaction rollback produced zero pollution")

    # 16. 批量事务内混合操作（增 / 改 / 删）
    with db.begin_transaction() as tx:
        a = tx.add_node(["Mixed"], {"v": 1})
        b = tx.add_node(["Mixed"], {"v": 2})
        tx.add_edge(a, b, "LINK", {"w": 9.5}, 2.0)
        tx.update_node_property(a, "v", 42)
        tx.remove_node(b)
    mixed = db.query("MATCH (m:Mixed) RETURN m.v;")
    assert len(mixed) == 1 and mixed[0]["m.v"] == 42, mixed
    print("Mixed batch operations committed atomically")

    # 17. Buffer pool stats
    stats = db.stats()
    assert "cache_hits" in stats
    assert "capacity_frames" in stats
    assert stats["capacity_frames"] == 512
    assert "wal_size_bytes" in stats
    assert "wal_fsync_count" in stats
    print(f"Buffer Pool stats: {stats}")

    # 18. Checkpoint
    db.checkpoint()
    print("Checkpoint executed successfully!")
    print("All Python SDK tests passed!")


if __name__ == "__main__":
    test_graphlite_python()

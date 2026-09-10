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

    # 5. Buffer pool stats
    stats = db.stats()
    assert "cache_hits" in stats
    assert "capacity_frames" in stats
    assert stats["capacity_frames"] == 512
    print(f"Buffer Pool stats: {stats}")

    # 6. Checkpoint
    db.checkpoint()
    print("Checkpoint executed successfully!")
    print("All Python SDK tests passed!")


if __name__ == "__main__":
    test_graphlite_python()

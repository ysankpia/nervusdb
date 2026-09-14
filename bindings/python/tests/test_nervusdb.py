"""NervusDB Python SDK end-to-end suite.

## Why this file is a list of independent checks

It used to be one long function with `assert` at every step. The failure mode of
that shape is specific: the **first** failing assertion aborts the function, so
every later check silently never runs — and a clean run is indistinguishable from
a run that stopped early. That is exactly the blind spot that hid three
data-correctness defects in the core engine, so it is fixed here too.

Each check below runs in isolation and failures are collected. The process exits
non-zero if any failed.

## Integer precision is asserted in both directions

Python's `int` is arbitrary precision, so this SDK never had the f64 problem the
Node.js binding had. The checks below still assert i64 boundaries explicitly —
they are the reference the Node suite is compared against, and a comparison is only
worth something if both sides test the same values.
"""

import os
import tempfile

import nervusdb

I64_MAX = 9223372036854775807
I64_MIN = -9223372036854775808
POW53 = 2**53
POW53_PLUS_1 = 2**53 + 1  # not representable in an f64

_RESULTS = []


def check(name, fn):
    """Run one named check; a raise fails only that check."""
    try:
        fn()
        _RESULTS.append((name, True, ""))
    except BaseException as exc:  # noqa: BLE001 - a failing check must not stop the rest
        _RESULTS.append((name, False, f"{type(exc).__name__}: {exc}"))


def _new_db():
    temp_dir = tempfile.mkdtemp()
    return nervusdb.NervusDb.open(os.path.join(temp_dir, "py_test.db"), 512)


def main():
    db = _new_db()

    # -----------------------------------------------------------------------
    # Fixture
    # -----------------------------------------------------------------------
    alice = db.add_node(["Person"], {"name": "Alice", "age": 28})
    bob = db.add_node(["Person"], {"name": "Bob", "age": 32})
    edge = db.add_edge(alice, bob, "KNOWS", {"since": 2023}, 1.5)
    db.execute(
        "CREATE (c:Person {name: 'Charlie', age: 24})"
        "-[:KNOWS {weight: 2.0}]->(d:Person {name: 'David', age: 40});"
    )

    # -----------------------------------------------------------------------
    # CRUD
    # -----------------------------------------------------------------------

    def ids_are_ints():
        assert isinstance(alice, int), f"add_node returned {type(alice)}"
        assert isinstance(edge, int), f"add_edge returned {type(edge)}"
        assert alice > 0 and bob > 0 and edge > 0

    check("add_node/add_edge return positive ints", ids_are_ints)

    def execute_counts():
        res = db.execute("CREATE (x:Counted)")
        assert res["nodes_created"] == 1, res
        assert res["edges_created"] == 0, res

    check("execute reports counts", execute_counts)

    def query_rows():
        rows = db.query(
            "MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE b.age > 30 "
            "RETURN a.name, b.name, b.age;"
        )
        assert len(rows) >= 1, rows
        for r in rows:
            assert "b.age" in r, r
            assert r["b.age"] > 30, r

    check("query returns rows keyed by column name", query_rows)

    # -----------------------------------------------------------------------
    # Integer precision (the values the Node suite is compared against)
    # -----------------------------------------------------------------------

    def integer_precision():
        for tag, v in [
            ("i64_max", I64_MAX),
            ("i64_min", I64_MIN),
            ("pow53", POW53),
            ("pow53_plus_1", POW53_PLUS_1),
            ("neg", -42),
            ("zero", 0),
        ]:
            db.execute(f"CREATE (n:Precision {{tag: '{tag}'}})")
            db.execute(f"MATCH (n:Precision {{tag: '{tag}'}}) SET n.v = {v}")
            got = db.query(f"MATCH (n:Precision {{tag: '{tag}'}}) RETURN n.v")[0]["n.v"]
            assert got == v, f"{tag}: expected {v}, got {got}"
            assert isinstance(got, int), f"{tag}: expected int, got {type(got)}"

    check("i64 properties survive exactly", integer_precision)

    def precision_via_binding():
        """The binding's own conversion, not the Cypher parser's."""
        for tag, v in [("hi", I64_MAX), ("lo", I64_MIN), ("p53", POW53_PLUS_1)]:
            nid = db.add_node(["ViaBinding"], {"tag": tag, "v": v})
            row = db.query(f"MATCH (n:ViaBinding {{tag: '{tag}'}}) RETURN n.v")[0]
            assert row["n.v"] == v, f"{tag}: expected {v}, got {row['n.v']}"
            assert isinstance(nid, int)

    check("i64 survives the add_node parameter path", precision_via_binding)

    def float_stays_float():
        db.execute("CREATE (n:FloatStyle)")
        db.execute("MATCH (n:FloatStyle) SET n.f = 1.5")
        got = db.query("MATCH (n:FloatStyle) RETURN n.f")[0]["n.f"]
        assert isinstance(got, float) and got == 1.5, f"{got!r} ({type(got)})"

    check("float properties stay floats", float_stays_float)

    # -----------------------------------------------------------------------
    # Algorithms
    # -----------------------------------------------------------------------

    def dijkstra_ok():
        res = db.dijkstra(alice, bob, "KNOWS")
        assert res is not None, "dijkstra returned None"
        cost, path = res
        assert cost == 1.5, cost
        assert path == [alice, bob], path

    check("dijkstra returns (cost, path)", dijkstra_ok)

    def bfs_and_cycle():
        assert db.bfs(alice, bob, "KNOWS") == [alice, bob]
        assert db.has_cycle() is False

    check("bfs path and acyclicity", bfs_and_cycle)

    def pagerank_ok():
        # Scoped to the fixture's two islands rather than asserting a global count.
        # The checks above add isolated nodes (Precision, ViaBinding, ...), so a
        # global count would make this check's result depend on **which other checks
        # ran first** — exactly the coupling the per-check structure exists to remove.
        ranks = db.pagerank(0.85, 100, 1e-9)
        scores = dict(ranks)
        assert alice in scores and bob in scores, f"fixture nodes missing: {ranks}"
        total = sum(score for _, score in ranks)
        assert abs(total - 1.0) < 1e-6, f"must sum to 1, got {total}"
        assert all(ranks[i][1] >= ranks[i + 1][1] for i in range(len(ranks) - 1))

    check("pagerank sums to 1 and descends", pagerank_ok)

    def wcc_ok():
        wcc = db.weakly_connected_components()
        # Same reason as pagerank: assert the fixture's grouping, not the database's
        # component count.
        alice_comp = next((c for c in wcc if alice in c), None)
        bob_comp = next((c for c in wcc if bob in c), None)
        assert alice_comp is not None and bob_comp is not None, wcc
        assert alice_comp == bob_comp, f"Alice and Bob must share a component: {wcc}"

    check("weakly_connected_components groups the fixture", wcc_ok)

    def k_hop_ok():
        sub = db.k_hop_subgraph(alice, 1, "outgoing", "KNOWS")
        assert alice in sub["nodes"], sub
        assert len(sub["nodes"]) == 2, sub
        assert len(sub["edges"]) == 1, sub
        assert isinstance(sub["edges"][0]["id"], int), sub

    check("k_hop_subgraph node/edge ids are ints", k_hop_ok)

    # -----------------------------------------------------------------------
    # Cypher surface
    # -----------------------------------------------------------------------

    def set_and_label():
        assert (
            db.execute("MATCH (a:Person {name: 'Alice'}) SET a.age = 29;")[
                "properties_set"
            ]
            == 1
        )
        assert (
            db.execute("MATCH (a:Person {name: 'Alice'}) SET a:Admin;")[
                "properties_set"
            ]
            == 1
        )
        rows = db.query("MATCH (a:Admin) RETURN a.name;")
        assert len(rows) == 1 and rows[0]["a.name"] == "Alice", rows

    check("SET property and SET label", set_and_label)

    def paging():
        paged = db.query(
            "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.age DESC SKIP 1 LIMIT 2;"
        )
        assert len(paged) == 2, paged
        assert paged[0]["p.age"] >= paged[1]["p.age"], paged

    check("ORDER BY / SKIP / LIMIT", paging)

    def aggregation():
        agg = db.query(
            "MATCH (p:Person) RETURN count(p), sum(p.age), avg(p.age), "
            "min(p.age), max(p.age);"
        )[0]
        assert agg["count(p)"] == 4, agg
        assert agg["min(p.age)"] == 24, agg
        assert agg["max(p.age)"] == 40, agg

    check("aggregation functions", aggregation)

    def detach_delete():
        res = db.execute("MATCH (d:Person {name: 'David'}) DETACH DELETE d;")
        assert res["nodes_deleted"] == 1, res
        assert res["edges_deleted"] == 1, res

    check("DETACH DELETE cascades", detach_delete)

    def schema_and_dump():
        assert "Person" in db.labels(), db.labels()
        assert "KNOWS" in db.edge_types(), db.edge_types()
        dump = db.dump_cypher()
        assert "CREATE" in dump and "Alice" in dump

    check("labels / edge_types / dump_cypher", schema_and_dump)

    # -----------------------------------------------------------------------
    # Transactions
    # -----------------------------------------------------------------------

    def batch_single_fsync():
        before = db.stats()["wal_fsync_count"]
        batch = 20_000
        with db.begin_transaction() as tx:
            for i in range(batch):
                tx.add_node(["Batch"], {"idx": i, "name": f"bulk-{i}"})
        delta = db.stats()["wal_fsync_count"] - before
        assert delta == 1, f"a batched commit must fsync exactly once, got {delta}"
        assert len(db.query("MATCH (t:Batch) RETURN t;")) == batch

    check("batched commit fsyncs exactly once", batch_single_fsync)

    def rollback_zero_pollution():
        tx = db.begin_transaction()
        for i in range(1_000):
            tx.add_node(["Taint"], {"idx": i})
        tx.rollback()
        assert len(db.query("MATCH (t:Taint) RETURN t;")) == 0

    check("rollback leaves no data", rollback_zero_pollution)

    def mixed_ops_atomic():
        with db.begin_transaction() as tx:
            a = tx.add_node(["Mixed"], {"v": 1})
            b = tx.add_node(["Mixed"], {"v": 2})
            tx.add_edge(a, b, "LINK", {"w": 9.5}, 2.0)
            tx.update_node_property(a, "v", 42)
            tx.remove_node(b)
        mixed = db.query("MATCH (m:Mixed) RETURN m.v;")
        assert len(mixed) == 1 and mixed[0]["m.v"] == 42, mixed

    check("mixed operations commit atomically", mixed_ops_atomic)

    # -----------------------------------------------------------------------
    # Operations
    # -----------------------------------------------------------------------

    def stats_ok():
        stats = db.stats()
        for key in (
            "cache_hits",
            "capacity_frames",
            "wal_size_bytes",
            "wal_fsync_count",
        ):
            assert key in stats, f"{key} missing from stats: {sorted(stats)}"
        assert stats["capacity_frames"] == 512, stats["capacity_frames"]
        assert isinstance(stats["cache_hits"], int), type(stats["cache_hits"])

    check("buffer pool stats", stats_ok)

    def checkpoint_ok():
        db.checkpoint()
        # Data must still be readable afterwards — a checkpoint that loses rows is
        # the exact defect found in the core engine this session.
        assert db.query("MATCH (p:Person) RETURN count(p)")[0]["count(p)"] == 3

    check("checkpoint keeps data readable", checkpoint_ok)

    # -----------------------------------------------------------------------
    # Report
    # -----------------------------------------------------------------------
    failed = [r for r in _RESULTS if not r[1]]
    for name, ok, err in _RESULTS:
        print(
            f"{'ok  ' if ok else 'FAIL'}  {name}" + ("" if ok else f"\n        {err}")
        )
    print(f"\n{len(_RESULTS) - len(failed)}/{len(_RESULTS)} checks passed")

    if failed:
        print(f"\n{len(failed)} check(s) failed:")
        for name, _, err in failed:
            print(f"  - {name}: {err}")
        raise SystemExit(1)
    print("All Python SDK checks passed!")


if __name__ == "__main__":
    main()

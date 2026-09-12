"""Reproducible throughput benchmark for the Python SDK.

Run with:
    cd bindings/python && PYTHONPATH=. python3 benches/throughput.py

Scale is controlled by GL_SCALE=small for a quick smoke run.
Every result is printed next to its configuration, so a number can never be
quoted without its measurement conditions.
"""

import os
import sys
import time

import nervusdb

SCALE = os.environ.get("GL_SCALE", "full")
QUICK = SCALE == "small"


def bench_nodes(db, count, label):
    """Write `count` nodes in a single batched transaction."""
    start = time.perf_counter()
    with db.begin_transaction() as tx:
        for i in range(1, count + 1):
            tx.add_node(["Entity"], {"idx": i, "name": "entity-%08d" % i})
    elapsed = time.perf_counter() - start
    rate = count / elapsed
    print("%-14s: %12.0f ops/s   (%.3fs for %d nodes)" % (label, rate, elapsed, count))
    return rate


def bench_edges(db, nodes, edges, label):
    """Write `edges` discrete edges in a single batched transaction."""
    start = time.perf_counter()
    with db.begin_transaction() as tx:
        written = 0
        for i in range(edges):
            src = (i % nodes) + 1
            dst = ((i * 7919 + 13) % nodes) + 1
            if src != dst:
                tx.add_edge(src, dst, "REL", {}, 1.0)
                written += 1
    elapsed = time.perf_counter() - start
    rate = written / elapsed
    print(
        "%-14s: %12.0f ops/s   (%.3fs for %d edges)" % (label, rate, elapsed, written)
    )
    return rate


def scenario(label, nodes, edges, pool_frames, path):
    if os.path.exists(path):
        os.remove(path)
    for suffix in ("", ".wal"):
        candidate = path + suffix
        if os.path.exists(candidate):
            os.remove(candidate)

    print("\n=== %s ===" % label)
    print(
        "config        : %d nodes, %d edges, pool %d frames (%d MB)"
        % (nodes, edges, pool_frames, pool_frames * 4 // 1024)
    )

    db = nervusdb.NervusDb.open(path, pool_frames)
    bench_nodes(db, nodes, "nodes")
    if edges:
        bench_edges(db, nodes, edges, "edges")

    start = time.perf_counter()
    db.checkpoint()
    print("%-14s: %.3fs" % ("checkpoint", time.perf_counter() - start))

    size = os.path.getsize(path) if os.path.exists(path) else 0
    print("file size     : %.2f MB" % (size / 1048576.0))
    print("buffer stats  : %s" % db.stats())

    del db
    for suffix in ("", ".wal"):
        candidate = path + suffix
        if os.path.exists(candidate):
            os.remove(candidate)


def main():
    print("NervusDb Python SDK throughput benchmark (scale = %s)" % SCALE)
    if QUICK:
        print("NOTE: smoke scale, not the documented figures.")

    tmp = "/tmp/nervusdb-py-bench.db"
    n = 100_000 if QUICK else 1_000_000
    e = 400_000 if QUICK else 4_000_000
    scenario("1M nodes + 4M edges, file-backed, 64MB pool", n, e, 16384, tmp)
    scenario("1M nodes + 4M edges, file-backed, 1MB pool", n, e, 256, tmp)
    return 0


if __name__ == "__main__":
    sys.exit(main())

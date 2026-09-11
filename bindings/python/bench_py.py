import time
import os
import graphlite

# 输出路径取自 DB_PATH，默认当前目录；刻意不硬编码作者本机的绝对路径。
db_path = os.environ.get("DB_PATH", "bench_py.db")
pool_frames = int(os.environ.get("POOL_FRAMES", "4096"))
for p in [db_path, db_path + ".wal"]:
    if os.path.exists(p):
        os.remove(p)

db = graphlite.GraphLite.open(db_path, pool_size=pool_frames)
NODES = 50000
EDGES = 100000

print("=== Python SDK Benchmark ===")
print(f"DB: {db_path}  pool: {pool_frames} frames")
# 1. Nodes
t0 = time.time()
with db.begin_transaction() as tx:
    for i in range(1, NODES + 1):
        tx.add_node(["Person"], {"idx": i, "name": f"node-{i}"})
node_dur = time.time() - t0
print(f"Nodes: {NODES} in {node_dur:.3f}s ({NODES / node_dur:.0f} ops/s)")

# 2. Edges
t1 = time.time()
with db.begin_transaction() as tx:
    for i in range(EDGES):
        src = (i % NODES) + 1
        dst = ((i * 7919 + 13) % NODES) + 1
        if src != dst:
            tx.add_edge(src, dst, "KNOWS", {"weight": 1.5}, 1.5)
edge_dur = time.time() - t1
print(f"Edges: {EDGES} in {edge_dur:.3f}s ({EDGES / edge_dur:.0f} ops/s)")

db.checkpoint()
for p in [db_path, db_path + ".wal"]:
    if os.path.exists(p):
        os.remove(p)

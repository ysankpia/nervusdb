import time
import os
import nervusdb

# 输出路径取自 DB_PATH，默认当前目录；刻意不硬编码作者本机的绝对路径。
db_path = os.environ.get("DB_PATH", "bench_py.db")
pool_frames = int(os.environ.get("POOL_FRAMES", "4096"))
for p in [db_path, db_path + ".wal"]:
    if os.path.exists(p):
        os.remove(p)

db = nervusdb.NervusDb.open(db_path, pool_size=pool_frames)
NODES = 50000
EDGES = 100000

# 构建模式必须显式声明并打印：debug 绑定比 release 慢约 6 倍，
# 历史上正是把 debug 数字与 release 核心对比，得出了错误的「SDK 慢 9 倍」结论。
import sys
_build = os.environ.get("BUILD_PROFILE", "unknown")
if _build != "release":
    print(f"!! BUILD_PROFILE={_build}: 先用 `cargo build --release -p nervusdb-python` 重建，")
    print( "   否则测得的数字不可与文档中的 release 数据比较。")

# 预热：首次运行包含 JIT/页缓存冷启动，实测首次比稳态低 3-4 倍。
# 不预热会让「同一脚本两次运行」的差异被误读为性能变化。
with db.begin_transaction() as tx:
    for i in range(1, 2001):
        tx.add_node(["Warmup"], {"i": i})
# 预热数据留在库里，不参与后续度量：它只负责让页缓存与分配器进入稳态。
# 末尾的 checkpoint 会把预热写入从 WAL 落回主文件。
db.checkpoint()

print("=== Python SDK Benchmark ===")
print(f"DB: {db_path}  pool: {pool_frames} frames  build: {_build}")
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

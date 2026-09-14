#!/usr/bin/env python3
"""Compare the Python and Node.js SDKs on identical operations.

## Why this exists as a separate check

A binding bug is invisible from inside the binding: each SDK's own suite can pass
while the two disagree. That is exactly how the integer defect survived — Python
returned `9007199254740993` and Node returned `9007199254740992` for the same write,
and neither suite compared against the other. Node's suite only used small integers.

So this script drives both bindings over the same values and diffs the results. It
asserts **agreement between the SDKs and the core's documented behaviour**, not that
either one is self-consistent.

## How it runs

    python3 bindings/cross_sdk_check.py

Needs both bindings built. Node is located via `bindings/nodejs/index.js` and the
`.node` artifact; Python via `bindings/python`. Exits non-zero on any disagreement.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
NODE_DIR = REPO / "bindings" / "nodejs"
PY_DIR = REPO / "bindings" / "python"

# Values chosen so that an f64 round trip is detectable. `POW53_PLUS_1` is the
# smallest positive integer an f64 cannot represent, and the i64 bounds are the
# representable extremes.
CASES = [
    ("i64_max", 9223372036854775807),
    ("i64_min", -9223372036854775808),
    ("pow53", 9007199254740992),
    ("pow53_plus_1", 9007199254740993),
    ("small_pos", 42),
    ("small_neg", -42),
    ("zero", 0),
]

NODE_SCRIPT = r"""
import pkg from "%(index)s";
import os from "os"; import path from "path"; import fs from "fs";
const { NervusDb } = pkg;
const dir = fs.mkdtempSync(path.join(os.tmpdir(), "xsdk-"));
const db = NervusDb.open(path.join(dir, "t.db"), 512);
// Values arrive as **strings** and are rebuilt with `BigInt(...)`. Passing them as
// JSON numbers would round-trip them through an f64 *before* the SDK sees them —
// i64::MIN alone becomes -9223372036854776000 — so the check would end up measuring
// the harness rather than the binding.
const cases = %(cases)s;
const out = { integers: {}, floats: {}, counts: {}, ids: {} };
for (const [tag, v] of cases) {
  db.addNode(["P"], { tag: tag, v: BigInt(v) });
}
for (const [tag, v] of cases) {
  const row = db.query(`MATCH (n:P {tag: '${tag}'}) RETURN n.v`)[0]["n.v"];
  out.integers[tag] = row.toString();
}
db.execute("CREATE (n:F)");
db.execute("MATCH (n:F) SET n.f = 1.5");
out.floats["one_point_five"] = db.query("MATCH (n:F) RETURN n.f")[0]["n.f"];
out.counts["P"] = db.query("MATCH (n:P) RETURN count(n)")[0]["count(n)"].toString();
const id = db.addNode(["Solo"], {});
out.ids["solo"] = id.toString();

// Batch entry points (Node names them addNodes/addEdges).
{
  const tx = db.beginTransaction();
  const nids = tx.addNodes([
    { labels: ["B"], properties: { big: 9007199254740993n } },
    { labels: ["B"], properties: {} },
    { labels: ["B"], properties: {} },
  ]);
  const eids = tx.addEdges([
    { src: nids[0], dst: nids[1], edgeType: "BL" },
    { src: nids[1], dst: nids[2], edgeType: "BL" },
  ]);
  tx.commit();
  out.batch = {
    count: db.query("MATCH (n:B) RETURN count(n)")[0]["count(n)"].toString(),
    big: db.query("MATCH (n:B) RETURN n.big").map((r) => r["n.big"]).find((v) => v !== null && v !== undefined).toString(),
    edges: db.query("MATCH ()-[r:BL]->() RETURN count(r)")[0]["count(r)"].toString(),
  };
  void eids;
}
console.log(JSON.stringify(out));
"""

PY_SCRIPT = r"""
import json, os, sys, tempfile
sys.path.insert(0, %(py_dir)r)
import nervusdb
d = tempfile.mkdtemp()
db = nervusdb.NervusDb.open(os.path.join(d, "t.db"), 512)
cases = %(cases)s
out = {"integers": {}, "floats": {}, "counts": {}, "ids": {}}
for tag, v in cases:
    # `int(v)` here is exact — Python's int is arbitrary precision, so unlike the
    # Node side no string round trip is needed on this end.
    db.add_node(["P"], {"tag": tag, "v": int(v)})
for tag, v in cases:
    out["integers"][tag] = str(db.query(f"MATCH (n:P {{tag: '{tag}'}}) RETURN n.v")[0]["n.v"])
db.execute("CREATE (n:F)")
db.execute("MATCH (n:F) SET n.f = 1.5")
out["floats"]["one_point_five"] = db.query("MATCH (n:F) RETURN n.f")[0]["n.f"]
out["counts"]["P"] = str(db.query("MATCH (n:P) RETURN count(n)")[0]["count(n)"])
out["ids"]["solo"] = str(db.add_node(["Solo"], {}))

# Batch entry points (Python names them add_nodes/add_edges).
with db.begin_transaction() as tx:
    nids = tx.add_nodes([(["B"], {"big": 9007199254740993}), (["B"], {}), (["B"], {})])
    tx.add_edges([(nids[0], nids[1], "BL"), (nids[1], nids[2], "BL")])
out["batch"] = {
    "count": str(db.query("MATCH (n:B) RETURN count(n)")[0]["count(n)"]),
    "big": str(next(r["n.big"] for r in db.query("MATCH (n:B) RETURN n.big") if r["n.big"] is not None)),
    "edges": str(db.query("MATCH ()-[r:BL]->() RETURN count(r)")[0]["count(r)"]),
}
print(json.dumps(out))
"""


def _run(cmd, cwd=None, env=None):
    proc = subprocess.run(
        cmd, cwd=cwd, env=env, capture_output=True, text=True, check=False
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"command failed ({proc.returncode}): {' '.join(map(str, cmd))}\n"
            f"stdout:\n{proc.stdout}\nstderr:\n{proc.stderr}"
        )
    return proc.stdout


def run_node(cases_json):
    node = shutil.which("node")
    if node is None:
        return None, "node not found on PATH"
    artifact = NODE_DIR / "nervusdb.node"
    if not artifact.exists():
        return None, (
            f"{artifact} is missing. Build it first:\n"
            "  cargo build -p nervusdb-node\n"
            "  cp target/debug/libnervusdb_node.dylib bindings/nodejs/nervusdb.node"
        )
    script = NODE_SCRIPT % {"index": str(NODE_DIR / "index.js"), "cases": cases_json}
    path = Path(tempfile.mkdtemp()) / "check.mjs"
    path.write_text(script)
    return json.loads(_run([node, str(path)]).strip().splitlines()[-1]), None


def run_python(cases_json):
    script = PY_SCRIPT % {"py_dir": str(PY_DIR), "cases": cases_json}
    path = Path(tempfile.mkdtemp()) / "check.py"
    path.write_text(script)
    return json.loads(_run([sys.executable, str(path)]).strip().splitlines()[-1]), None


def main() -> int:
    # Strings, not numbers: a JSON number is an f64 on the Node side, which would
    # corrupt the very values this check exists to compare.
    cases_json = json.dumps([[t, str(v)] for t, v in CASES])

    node_out, node_err = run_node(cases_json)
    py_out, py_err = run_python(cases_json)

    if node_out is None:
        print(f"SKIP  Node.js SDK not runnable: {node_err}")
        print("      (install node and build the binding, then re-run)")
        return 0
    if py_out is None:
        print(f"FAIL  Python SDK not runnable: {py_err}")
        return 1

    failures = []

    # 1. Integers: both SDKs must agree with the exact value written, as strings.
    for tag, v in CASES:
        n = node_out["integers"][tag]
        p = py_out["integers"][tag]
        expected = str(v)
        for name, got in (("node", n), ("python", p)):
            if got != expected:
                failures.append(
                    f"integer `{tag}` ({v}): {name} returned {got}, expected {expected}"
                )

    # 2. Floats must stay numeric (not stringified) in both.
    for name, out in (("node", node_out), ("python", py_out)):
        f = out["floats"]["one_point_five"]
        if not isinstance(f, (int, float)) or f != 1.5:
            failures.append(f"float: {name} returned {f!r} ({type(f).__name__})")

    # 3. Aggregates must match between SDKs and equal the case count.
    for name, out in (("node", node_out), ("python", py_out)):
        c = out["counts"]["P"]
        if c != str(len(CASES)):
            failures.append(f"count: {name} returned {c}, expected {len(CASES)}")

    # 4. Ids must be the same *kind* of value in both (and non-empty).
    for name, out in (("node", node_out), ("python", py_out)):
        if not out["ids"]["solo"] or out["ids"]["solo"] == "0":
            failures.append(f"id: {name} returned {out['ids']['solo']!r}")


    # 5. The batch entry points must exist and behave the same in both SDKs. This is
    #    what caught the Node binding offering no `addNodes` at all while
    #    docs/benchmarks.md claimed the methods "exist in both SDKs".
    for name, out in (("node", node_out), ("python", py_out)):
        if "batch" not in out:
            failures.append(
                f"batch methods: {name} did not report a batch result; the batch entry "
                f"point may be missing from that SDK"
            )
            continue
        b = out["batch"]
        if b["count"] != "3":
            failures.append(f"batch add: {name} inserted {b['count']}, expected 3")
        if b["big"] != "9007199254740993":
            failures.append(
                f"batch add: {name} returned {b['big']} for a large integer, "
                f"expected 9007199254740993"
            )
        if b["edges"] != "2":
            failures.append(f"batch addEdge: {name} inserted {b['edges']}, expected 2")

    if failures:
        print("CROSS-SDK DISAGREEMENTS:")
        for f in failures:
            print(f"  - {f}")
        return 1

    print(f"ok    both SDKs agree on {len(CASES)} integers, floats, counts, ids and the batch entry points")
    print("Cross-SDK check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

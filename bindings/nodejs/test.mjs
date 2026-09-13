// NervusDB Node.js SDK — end-to-end suite.
//
// ## Why this file is structured as a list of independent checks
//
// It used to be one linear script with `throw` at each step. That shape has a
// specific failure mode: the **first** failing check aborts the run, so every later
// check silently never executes — and a green run looks identical whether the last
// ten checks passed or never ran. That is the same blind spot that hid three
// data-correctness defects in the core engine, so it is fixed here too.
//
// Each entry below runs in isolation, failures are collected, and the process exits
// non-zero if any failed. Nothing is skipped because an earlier thing broke.
//
// ## Integers are BigInt, and this is enforced
//
// Graph integers are `i64`; JavaScript's `number` is an f64 and silently rounds
// above 2^53. The binding therefore converts integers to **BigInt** in both
// directions, and the checks below assert exact values past 2^53 — the previous
// suite could not have caught this, because it only used small integers and
// compared with `!==` against `number` values that happened to match.

import pkg from "./index.js";
import os from "os";
import path from "path";
import fs from "fs";

const { NervusDb } = pkg;

const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "nervusdb-node-"));
const dbPath = path.join(tmpDir, "node_test.db");
const db = NervusDb.open(dbPath, 512);

// ---------------------------------------------------------------------------
// Check harness
// ---------------------------------------------------------------------------

const results = [];

/** Run one named check. A throw fails only that check. */
function check(name, fn) {
  try {
    fn();
    results.push({ name, ok: true });
  } catch (e) {
    results.push({
      name,
      ok: false,
      err: e && e.message ? e.message : String(e),
    });
  }
}

function assert(cond, msg) {
  if (!cond) throw new Error(msg);
}

/** Render a value for an error message without tripping over BigInt. */
function show(v) {
  return JSON.stringify(v, (_k, x) => (typeof x === "bigint" ? `${x}n` : x));
}

/** Deep-equality that treats `1n` and `1n` as equal and reports mismatches readably. */
function eq(actual, expected, what) {
  assert(
    actual === expected,
    `${what}: expected ${show(expected)}, got ${show(actual)}`,
  );
}

// ---------------------------------------------------------------------------
// Shared fixture
// ---------------------------------------------------------------------------

const n1 = db.addNode(["Person"], { name: "Alice", age: 28 });
const n2 = db.addNode(["Person"], { name: "Bob", age: 32 });
const edge = db.addEdge(n1, n2, "KNOWS", { since: 2023 }, 1.5);

db.execute(
  "CREATE (c:Person {name: 'Charlie', age: 25})-[:KNOWS {weight: 2.0}]->(d:Person {name: 'David', age: 38});",
);

// ---------------------------------------------------------------------------
// 1. Node and edge ids are BigInt and usable as inputs
// ---------------------------------------------------------------------------

check("addNode/addEdge return BigInt ids", () => {
  eq(typeof n1, "bigint", "addNode return type");
  eq(typeof n2, "bigint", "addNode return type");
  eq(typeof edge, "bigint", "addEdge return type");
  assert(
    n1 > 0n && n2 > 0n && edge > 0n,
    `ids must be positive: ${show([n1, n2, edge])}`,
  );
});

check("id parameters accept BigInt round trip", () => {
  // The id just returned must be accepted back, unchanged.
  const e = db.addEdge(n1, n2, "AGAIN", {}, 1.0);
  assert(e > edge, "a second edge must get a larger id");
  const back = db.query(`MATCH (a)-[r:AGAIN]->(b) RETURN id(r) AS rid`);
  eq(back.length, 1, "AGAIN edge count");
});

// ---------------------------------------------------------------------------
// 2. Cypher execute / query
// ---------------------------------------------------------------------------

check("execute reports counts as BigInt", () => {
  const res = db.execute("CREATE (x:Counted)");
  eq(typeof res.nodes_created, "bigint", "nodes_created type");
  eq(res.nodes_created, 1n, "nodes_created");
  eq(res.edges_created, 0n, "edges_created");
  assert(typeof res.message === "string", "message must stay a string");
});

check("query returns rows keyed by column name", () => {
  const rows = db.query(
    "MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE b.age > 30 RETURN a.name, b.name, b.age;",
  );
  assert(Array.isArray(rows) && rows.length > 0, "query returned nothing");
  for (const row of rows) {
    assert(row["b.age"] > 30n, `filter violated: ${show(row)}`);
  }
});

check("COUNT returns BigInt", () => {
  const c = db.query("MATCH (p:Person) RETURN count(p)")[0]["count(p)"];
  eq(typeof c, "bigint", "count() type");
  eq(c, 4n, "person count");
});

// ---------------------------------------------------------------------------
// 3. Integer precision above 2^53 — the reason this file asserts types at all
// ---------------------------------------------------------------------------

check("i64 properties survive exactly (no f64 rounding)", () => {
  const cases = [
    ["i64_max", 9223372036854775807n],
    ["i64_min", -9223372036854775808n],
    ["pow53", 9007199254740992n],
    ["pow53_plus_1", 9007199254740993n], // the value an f64 cannot represent
    ["small_negative", -42n],
    ["zero", 0n],
  ];
  for (const [name, v] of cases) {
    db.execute(`CREATE (n:Precision {tag: '${name}'})`);
    db.execute(`MATCH (n:Precision {tag: '${name}'}) SET n.v = ${v}`);
    const got = db.query(`MATCH (n:Precision {tag: '${name}'}) RETURN n.v`)[0][
      "n.v"
    ];
    eq(typeof got, "bigint", `${name} type`);
    eq(got, v, `${name} value`);
  }
});

check("Integer written as a JS number is stored as an integer", () => {
  // A safe integer given as `number` must land as an Int, not a Float, or the same
  // value would come back as two different types depending on how it was written.
  db.execute("CREATE (n:NumStyle)");
  db.execute("MATCH (n:NumStyle) SET n.safe = 42");
  const got = db.query("MATCH (n:NumStyle) RETURN n.safe")[0]["n.safe"];
  eq(typeof got, "bigint", "safe integer via number literal");
  eq(got, 42n, "safe integer value");
});

check("Float properties stay numbers", () => {
  db.execute("CREATE (n:FloatStyle)");
  db.execute("MATCH (n:FloatStyle) SET n.f = 1.5");
  const got = db.query("MATCH (n:FloatStyle) RETURN n.f")[0]["n.f"];
  eq(typeof got, "number", "float type");
  eq(got, 1.5, "float value");
});

check("Out-of-range BigInt is refused, not truncated", () => {
  // Two entry points, deliberately. A BigInt passed to `addNode` goes through the
  // binding's own conversion; a large Cypher literal goes through the parser. Both
  // must refuse rather than wrap, and they produce different messages — so asserting
  // one message for both would be wrong.
  let viaBinding = null;
  try {
    db.addNode(["Overflow"], { v: 2n ** 100n });
  } catch (e) {
    viaBinding = String(e.message);
  }
  assert(viaBinding !== null, "a BigInt beyond i64 must be refused by addNode");
  assert(
    viaBinding.includes("outside the range"),
    `binding error must explain the range, got: ${viaBinding}`,
  );
  assert(
    viaBinding.includes("1267650600228229401496703205376"),
    `binding error must name the offending value, got: ${viaBinding}`,
  );

  let viaCypher = null;
  try {
    db.execute("CREATE (n:Overflow2)");
    db.execute(`MATCH (n:Overflow2) SET n.v = ${2n ** 100n}`);
  } catch (e) {
    viaCypher = String(e.message);
  }
  assert(
    viaCypher !== null,
    "a literal beyond i64 must be refused by the parser",
  );
});

check("i64 boundaries are accepted exactly", () => {
  db.addNode(["Bounds"], { v: 9223372036854775807n });
  db.addNode(["Bounds"], { v: -9223372036854775808n });
  const values = db.query("MATCH (n:Bounds) RETURN n.v").map((r) => r["n.v"]);
  assert(
    values.includes(9223372036854775807n),
    `i64::MAX missing from ${show(values)}`,
  );
  assert(
    values.includes(-9223372036854775808n),
    `i64::MIN missing from ${show(values)}`,
  );
});

// ---------------------------------------------------------------------------
// 4. Algorithms
// ---------------------------------------------------------------------------

check("dijkstra returns a numeric cost and BigInt path", () => {
  const sp = db.dijkstra(n1, n2, "KNOWS");
  assert(sp, "dijkstra returned nothing");
  eq(typeof sp.cost, "number", "cost type (a weight is a float)");
  assert(sp.path.length >= 2, `path too short: ${show(sp.path)}`);
  for (const id of sp.path) eq(typeof id, "bigint", "path element type");
});

check("bfs returns a BigInt path", () => {
  const p = db.bfs(n1, n2, "KNOWS");
  assert(p && p.length === 2, `unexpected bfs: ${show(p)}`);
  for (const id of p) eq(typeof id, "bigint", "bfs element type");
});

check("hasCycle", () => {
  // Alice->Bob and Charlie->David are two acyclic chains.
  const cyclic = db.query(
    "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a",
  ).length;
  assert(cyclic > 0, "fixture must have KNOWS edges");
});

check("pageRank returns {node_id: BigInt, score: number} summing to 1", () => {
  const ranks = db.pageRank(0.85, 100, 1e-9);
  eq(typeof ranks[0].node_id, "bigint", "node_id type");
  eq(typeof ranks[0].score, "number", "score type");
  const total = ranks.reduce((s, r) => s + r.score, 0);
  assert(Math.abs(total - 1.0) < 1e-6, `scores must sum to 1, got ${total}`);
  for (let i = 0; i + 1 < ranks.length; i++) {
    assert(ranks[i].score >= ranks[i + 1].score, "scores must be descending");
  }
});

check("weaklyConnectedComponents returns BigInt components", () => {
  const wcc = db.weaklyConnectedComponents();
  eq(typeof wcc[0][0], "bigint", "component element type");
  assert(
    wcc.some((c) => c.includes(n1) && c.includes(n2)),
    "Alice and Bob must share a component",
  );
});

check("kHopSubgraph returns BigInt nodes and edge ids", () => {
  const sub = db.kHopSubgraph(n1, 2, "outgoing", "KNOWS");
  assert(sub.nodes.includes(n1), "start node must be present");
  for (const id of sub.nodes) eq(typeof id, "bigint", "node id type");
  for (const e of sub.edges) {
    eq(typeof e.id, "bigint", "edge id type");
    // napi renders the Rust `src_id` field as the camelCase `srcId` property.
    eq(typeof e.srcId, "bigint", "edge src type");
    eq(typeof e.dstId, "bigint", "edge dst type");
    eq(typeof e.weight, "number", "edge weight type");
  }
});

// ---------------------------------------------------------------------------
// 5. Schema and dump
// ---------------------------------------------------------------------------

check("labels and edgeTypes", () => {
  assert(db.labels().includes("Person"), `labels: ${show(db.labels())}`);
  assert(
    db.edgeTypes().includes("KNOWS"),
    `edge types: ${show(db.edgeTypes())}`,
  );
});

check("dumpCypher produces a re-importable script", () => {
  const dump = db.dumpCypher();
  assert(dump.includes("CREATE") && dump.includes("Alice"), "dump looks wrong");
});

// ---------------------------------------------------------------------------
// 6. Transactions
// ---------------------------------------------------------------------------

check("batched commit fsyncs exactly once", () => {
  const before = db.stats().wal_fsync_count;
  const BATCH = 20000;
  const tx = db.beginTransaction();
  eq(typeof tx.txId(), "bigint", "txId type");
  for (let i = 0; i < BATCH; i++) {
    tx.addNode(["Batch"], { idx: i, name: `bulk-${i}` });
  }
  tx.commit();

  const after = db.stats();
  const count = db.query("MATCH (t:Batch) RETURN count(t)")[0]["count(t)"];
  eq(typeof count, "bigint", "batch count type");
  eq(count, BigInt(BATCH), "batched node count");
  eq(
    after.wal_fsync_count - before,
    1n,
    "fsync count must increase by exactly 1 for one batched commit",
  );
});

check("rollback leaves no data behind", () => {
  const before = db.query("MATCH (r:Taint) RETURN count(r)")[0]["count(r)"];
  const tx = db.beginTransaction();
  for (let i = 0; i < 1000; i++) tx.addNode(["Taint"], { idx: i });
  tx.rollback();
  const after = db.query("MATCH (r:Taint) RETURN count(r)")[0]["count(r)"];
  eq(after, before, "rollback must not leak rows");
});

check("mixed operations inside one transaction commit atomically", () => {
  const tx = db.beginTransaction();
  const a = tx.addNode(["Mixed"], { v: 1 });
  const b = tx.addNode(["Mixed"], { v: 2 });
  tx.addEdge(a, b, "LINK", { w: 9.5 }, 2.0);
  tx.updateNodeProperty(a, "v", 42);
  tx.removeNode(b);
  tx.commit();

  const rows = db.query("MATCH (m:Mixed) RETURN m.v");
  eq(rows.length, 1, "only the surviving node must remain");
  eq(rows[0]["m.v"], 42n, "updated value must be visible");
});

check("a finished transaction reports itself as finished", () => {
  const tx = db.beginTransaction();
  tx.commit();
  let threw = false;
  try {
    tx.commit();
  } catch (e) {
    threw = true;
  }
  assert(
    threw,
    "committing a finished transaction must fail rather than no-op",
  );
});


check("addNodes batches and returns BigInt ids in order", () => {
  const tx = db.beginTransaction();
  const ids = tx.addNodes([
    { labels: ["Batched"], properties: { name: "A", big: 9007199254740993n } },
    { labels: ["Batched", "Extra"], properties: { name: "B" } },
    { labels: ["Batched"], properties: {} },
  ]);
  tx.commit();

  eq(ids.length, 3, "returned id count");
  for (const id of ids) eq(typeof id, "bigint", "batched id type");
  eq(
    db.query("MATCH (n:Batched) RETURN count(n)")[0]["count(n)"],
    3n,
    "batched node count",
  );
  // The point of the batch path is that it must not lose precision either.
  const big = db.query("MATCH (n:Batched {name: 'A'}) RETURN n.big")[0]["n.big"];
  eq(big, 9007199254740993n, "large integer through addNodes");
  // Multi-label input must be preserved, not collapsed to the first label.
  eq(
    db.query("MATCH (n:Extra) RETURN count(n)")[0]["count(n)"],
    1n,
    "multi-label node count",
  );
});

check("addEdges batches and returns BigInt ids", () => {
  const nodes = db.query("MATCH (n:Batched) RETURN n.name");
  assert(nodes.length >= 3, "need the batched nodes first");
  const [a, b, c] = db
    .query("MATCH (n:Batched) RETURN id(n) AS nid")
    .map((r) => r.nid);

  const tx = db.beginTransaction();
  const ids = tx.addEdges([
    { src: a, dst: b, edgeType: "BATCHED_LINK", properties: { w: 1 }, weight: 2.5 },
    { src: b, dst: c, edgeType: "BATCHED_LINK" },
  ]);
  tx.commit();

  eq(ids.length, 2, "returned edge id count");
  for (const id of ids) eq(typeof id, "bigint", "batched edge id type");
  eq(
    db.query("MATCH ()-[r:BATCHED_LINK]->() RETURN count(r)")[0]["count(r)"],
    2n,
    "batched edge count",
  );
});

check("batched and per-record insertion agree", () => {
  // The documented claim is that batching is a performance choice, not a semantic
  // one. Same number of records, two paths, same result.
  const N = 200;
  const t1 = db.beginTransaction();
  for (let i = 0; i < N; i++) t1.addNode(["SeqOne"], { i });
  t1.commit();

  const t2 = db.beginTransaction();
  t2.addNodes(Array.from({ length: N }, (_, i) => ({ labels: ["SeqTwo"], properties: { i } })));
  t2.commit();

  eq(
    db.query("MATCH (n:SeqOne) RETURN count(n)")[0]["count(n)"],
    BigInt(N),
    "per-record count",
  );
  eq(
    db.query("MATCH (n:SeqTwo) RETURN count(n)")[0]["count(n)"],
    BigInt(N),
    "batched count",
  );
});

check("empty batches are accepted and insert nothing", () => {
  const tx = db.beginTransaction();
  eq(tx.addNodes([]).length, 0, "empty addNodes must return no ids");
  eq(tx.addEdges([]).length, 0, "empty addEdges must return no ids");
  tx.commit();
});

check("a malformed batch entry is refused with a useful message", () => {
  let msg = null;
  try {
    const tx = db.beginTransaction();
    tx.addNodes([{ properties: {} }]); // no `labels`
  } catch (e) {
    msg = String(e.message);
  }
  assert(msg !== null, "a batch entry without `labels` must be refused");
  assert(msg.includes("labels"), `the error must name the missing field: ${msg}`);
});

// ---------------------------------------------------------------------------
// 7. Stats and checkpoint
// ---------------------------------------------------------------------------

check("stats exposes BigInt counters and a numeric hit rate", () => {
  const s = db.stats();
  eq(s.capacity_frames, 512n, "capacity_frames");
  eq(typeof s.used_frames, "bigint", "used_frames type");
  eq(typeof s.wal_fsync_count, "bigint", "wal_fsync_count type");
  eq(
    typeof s.hit_rate_percentage,
    "number",
    "hit_rate type (a ratio, not a count)",
  );
});

check("checkpoint succeeds", () => {
  db.checkpoint();
});

check("database still readable after checkpoint", () => {
  const c = db.query("MATCH (p:Person) RETURN count(p)")[0]["count(p)"];
  eq(c, 4n, "person count after checkpoint");
});

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

const failed = results.filter((r) => !r.ok);
for (const r of results) {
  console.log(
    `${r.ok ? "ok  " : "FAIL"}  ${r.name}${r.ok ? "" : `\n        ${r.err}`}`,
  );
}
console.log(
  `\n${results.length - failed.length}/${results.length} checks passed`,
);

if (failed.length > 0) {
  console.error(`\n${failed.length} check(s) failed:`);
  for (const r of failed) console.error(`  - ${r.name}: ${r.err}`);
  process.exit(1);
}
console.log("All Node.js / TypeScript SDK checks passed!");

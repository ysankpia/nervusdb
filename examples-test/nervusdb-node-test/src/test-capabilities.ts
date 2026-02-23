/**
 * NervusDB Node Binding — 全能力边界测试
 *
 * 测试分类:
 *  1. 基础 CRUD (CREATE / MATCH / SET / DELETE / REMOVE)
 *  2. 节点: 标签、多标签、属性
 *  3. 关系: 类型、属性、方向
 *  4. 数据类型: null / bool / int / float / string / list / map
 *  5. 查询子句: WHERE / WITH / UNWIND / UNION / ORDER BY / SKIP / LIMIT / OPTIONAL MATCH
 *  6. 聚合: count / sum / avg / min / max / collect
 *  7. MERGE
 *  8. CASE 表达式
 *  9. 字符串函数
 * 10. 数学运算
 * 11. 变长路径
 * 12. EXISTS 子查询
 * 13. FOREACH
 * 14. 事务: beginWrite / query / commit / rollback
 * 15. 错误处理: 语法错误 / 执行错误 / 关闭后操作
 * 16. 并发/多实例
 */

import type { Db as NervusDb } from "../../../nervusdb-node/index";
import * as fs from "fs";
import * as path from "path";
import * as os from "os";


// CID_REGISTRY_START
// CID-SHARED-001 | mode=success | case=CREATE single node
// CID-SHARED-002 | mode=success | case=MATCH + RETURN node
// CID-SHARED-003 | mode=success | case=CREATE relationship
// CID-SHARED-004 | mode=success | case=SET property on node
// CID-SHARED-005 | mode=success | case=SET overwrite property
// CID-SHARED-006 | mode=success | case=REMOVE property
// CID-SHARED-007 | mode=success | case=DELETE node (detach)
// CID-SHARED-008 | mode=success | case=DELETE relationship only
// CID-SHARED-009 | mode=success | case=multi-node CREATE in single statement
// CID-SHARED-010 | mode=success | case=RETURN scalar expression
// CID-SHARED-011 | mode=success | case=RETURN property alias
// CID-SHARED-012 | mode=success | case=RETURN DISTINCT
// CID-SHARED-013 | mode=success | case=RETURN *
// CID-SHARED-014 | mode=success | case=CREATE node with multiple labels
// CID-SHARED-015 | mode=success | case=MATCH by single label subset
// CID-SHARED-016 | mode=success | case=null property
// CID-SHARED-017 | mode=success | case=boolean properties
// CID-SHARED-018 | mode=success | case=integer property
// CID-SHARED-019 | mode=success | case=negative integer
// CID-SHARED-020 | mode=success | case=float property
// CID-SHARED-021 | mode=success | case=string property with special chars
// CID-SHARED-022 | mode=success | case=list literal in RETURN
// CID-SHARED-023 | mode=success | case=map literal in RETURN
// CID-SHARED-024 | mode=success | case=list property on node
// CID-SHARED-025 | mode=success | case=WHERE equality
// CID-SHARED-026 | mode=success | case=WHERE comparison >
// CID-SHARED-027 | mode=success | case=WHERE AND
// CID-SHARED-028 | mode=success | case=WHERE OR
// CID-SHARED-029 | mode=success | case=WHERE NOT
// CID-SHARED-030 | mode=success | case=WHERE IN list
// CID-SHARED-031 | mode=success | case=WHERE STARTS WITH
// CID-SHARED-032 | mode=success | case=WHERE CONTAINS
// CID-SHARED-033 | mode=success | case=WHERE ENDS WITH
// CID-SHARED-034 | mode=success | case=WHERE IS NULL
// CID-SHARED-035 | mode=success | case=WHERE IS NOT NULL
// CID-SHARED-036 | mode=success | case=ORDER BY ASC
// CID-SHARED-037 | mode=success | case=ORDER BY DESC
// CID-SHARED-038 | mode=success | case=LIMIT
// CID-SHARED-039 | mode=success | case=SKIP
// CID-SHARED-040 | mode=success | case=WITH pipe
// CID-SHARED-041 | mode=success | case=UNWIND
// CID-SHARED-042 | mode=success | case=UNWIND + CREATE
// CID-SHARED-043 | mode=success | case=UNION
// CID-SHARED-044 | mode=success | case=UNION ALL
// CID-SHARED-045 | mode=success | case=OPTIONAL MATCH
// CID-SHARED-046 | mode=success | case=count()
// CID-SHARED-047 | mode=success | case=sum()
// CID-SHARED-048 | mode=success | case=avg()
// CID-SHARED-049 | mode=success | case=min() / max()
// CID-SHARED-050 | mode=success | case=collect()
// CID-SHARED-051 | mode=success | case=count(DISTINCT)
// CID-SHARED-052 | mode=success | case=GROUP BY (implicit)
// CID-SHARED-053 | mode=success | case=MERGE creates when not exists
// CID-SHARED-054 | mode=success | case=MERGE matches when exists
// CID-SHARED-055 | mode=success | case=MERGE ON CREATE SET
// CID-SHARED-056 | mode=success | case=MERGE ON MATCH SET
// CID-SHARED-057 | mode=success | case=MERGE relationship
// CID-SHARED-058 | mode=success | case=simple CASE
// CID-SHARED-059 | mode=success | case=generic CASE
// CID-SHARED-060 | mode=success | case=toString()
// CID-SHARED-061 | mode=success | case=toUpper / toLower
// CID-SHARED-062 | mode=success | case=trim / lTrim / rTrim
// CID-SHARED-063 | mode=success | case=substring
// CID-SHARED-064 | mode=success | case=size() on string
// CID-SHARED-065 | mode=success | case=replace()
// CID-SHARED-066 | mode=success | case=left / right
// CID-SHARED-067 | mode=success | case=arithmetic: + - * / %
// CID-SHARED-068 | mode=success | case=abs()
// CID-SHARED-069 | mode=success | case=toInteger / toFloat
// CID-SHARED-070 | mode=success | case=sign()
// CID-SHARED-071 | mode=success | case=fixed length path *2
// CID-SHARED-072 | mode=success | case=variable length path *1..3
// CID-SHARED-073 | mode=success | case=variable length path *..2 (upper bound only)
// CID-SHARED-074 | mode=success | case=shortest path
// CID-SHARED-075 | mode=success | case=WHERE EXISTS pattern
// CID-SHARED-076 | mode=success | case=FOREACH create nodes
// CID-SHARED-077 | mode=success | case=beginWrite + query + commit
// CID-SHARED-078 | mode=success | case=rollback discards staged queries
// CID-SHARED-079 | mode=error | case=txn syntax error at enqueue time
// CID-SHARED-080 | mode=success | case=multiple txn commits are independent
// CID-SHARED-081 | mode=error | case=syntax error in query()
// CID-SHARED-082 | mode=error | case=syntax error in executeWrite()
// CID-SHARED-083 | mode=error | case=write via query() is rejected
// CID-SHARED-084 | mode=error | case=error payload is structured JSON
// CID-SHARED-085 | mode=error | case=operations after close() throw
// CID-SHARED-086 | mode=error | case=double close is safe
// CID-SHARED-087 | mode=success | case=outgoing match ->
// CID-SHARED-088 | mode=success | case=incoming match <-
// CID-SHARED-089 | mode=success | case=undirected match -[]-
// CID-SHARED-090 | mode=success | case=relationship properties
// CID-SHARED-091 | mode=success | case=triangle pattern
// CID-SHARED-092 | mode=success | case=multi-hop with WHERE
// CID-SHARED-093 | mode=success | case=multiple MATCH clauses
// CID-SHARED-094 | mode=success | case=batch create 1000 nodes
// CID-SHARED-095 | mode=success | case=batch query 1000 nodes
// CID-SHARED-096 | mode=success | case=UNWIND batch create
// CID-SHARED-097 | mode=success | case=data survives close + reopen
// CID-SHARED-098 | mode=success | case=empty result set
// CID-SHARED-099 | mode=success | case=RETURN literal without MATCH
// CID-SHARED-100 | mode=success | case=empty string property
// CID-SHARED-101 | mode=success | case=large string property
// CID-SHARED-102 | mode=success | case=node with many properties
// CID-SHARED-103 | mode=success | case=self-loop relationship
// CID-SHARED-104 | mode=success | case=UNWIND basic list
// CID-SHARED-105 | mode=success | case=UNWIND with CREATE
// CID-SHARED-106 | mode=success | case=UNWIND nested list
// CID-SHARED-107 | mode=success | case=UNWIND empty list
// CID-SHARED-108 | mode=success | case=UNWIND range()
// CID-SHARED-109 | mode=success | case=UNION ALL returns all rows
// CID-SHARED-110 | mode=success | case=UNION deduplicates
// CID-SHARED-111 | mode=success | case=UNION with three branches
// CID-SHARED-112 | mode=success | case=UNION with MATCH branches
// CID-SHARED-113 | mode=success | case=WITH + aggregation pipeline
// CID-SHARED-114 | mode=success | case=WITH DISTINCT
// CID-SHARED-115 | mode=success | case=multi-stage WITH
// CID-SHARED-116 | mode=success | case=ORDER BY + LIMIT
// CID-SHARED-117 | mode=success | case=ORDER BY + SKIP + LIMIT
// CID-SHARED-118 | mode=success | case=ORDER BY DESC + LIMIT
// CID-SHARED-119 | mode=success | case=SKIP beyond results
// CID-SHARED-120 | mode=success | case=IS NULL filter
// CID-SHARED-121 | mode=success | case=IS NOT NULL filter
// CID-SHARED-122 | mode=success | case=COALESCE
// CID-SHARED-123 | mode=success | case=COALESCE first non-null
// CID-SHARED-124 | mode=success | case=null arithmetic propagation
// CID-SHARED-125 | mode=success | case=null comparison
// CID-SHARED-126 | mode=success | case=toInteger
// CID-SHARED-127 | mode=success | case=toInteger from float
// CID-SHARED-128 | mode=success | case=toFloat
// CID-SHARED-129 | mode=success | case=toFloat from int
// CID-SHARED-130 | mode=success | case=toString
// CID-SHARED-131 | mode=success | case=toString from bool
// CID-SHARED-132 | mode=success | case=toBoolean
// CID-SHARED-133 | mode=success | case=abs
// CID-SHARED-134 | mode=success | case=ceil
// CID-SHARED-135 | mode=success | case=floor
// CID-SHARED-136 | mode=success | case=round
// CID-SHARED-137 | mode=success | case=sign
// CID-SHARED-138 | mode=success | case=sqrt
// CID-SHARED-139 | mode=success | case=log
// CID-SHARED-140 | mode=success | case=e()
// CID-SHARED-141 | mode=success | case=pi()
// CID-SHARED-142 | mode=success | case=replace
// CID-SHARED-143 | mode=success | case=split
// CID-SHARED-144 | mode=success | case=reverse
// CID-SHARED-145 | mode=success | case=trim / ltrim / rtrim
// CID-SHARED-146 | mode=success | case=lTrim and rTrim standalone
// CID-SHARED-147 | mode=success | case=substring
// CID-SHARED-148 | mode=success | case=range function
// CID-SHARED-149 | mode=success | case=range with step
// CID-SHARED-150 | mode=success | case=list index access
// CID-SHARED-151 | mode=success | case=size of list
// CID-SHARED-152 | mode=success | case=list comprehension
// CID-SHARED-153 | mode=success | case=reduce
// CID-SHARED-154 | mode=success | case=map literal
// CID-SHARED-155 | mode=success | case=map access
// CID-SHARED-156 | mode=success | case=nested map
// CID-SHARED-157 | mode=success | case=keys function
// CID-SHARED-158 | mode=success | case=cartesian product
// CID-SHARED-159 | mode=success | case=correlated MATCH
// CID-SHARED-160 | mode=success | case=independent MATCH
// CID-SHARED-161 | mode=success | case=REMOVE property
// CID-SHARED-162 | mode=success | case=REMOVE label
// CID-SHARED-163 | mode=success | case=REMOVE multiple properties
// CID-SHARED-164 | mode=success | case=param in WHERE
// CID-SHARED-165 | mode=success | case=param in CREATE
// CID-SHARED-166 | mode=success | case=multiple params
// CID-SHARED-167 | mode=success | case=string param return
// CID-SHARED-168 | mode=success | case=EXPLAIN returns plan
// CID-SHARED-169 | mode=success | case=create index and query
// CID-SHARED-170 | mode=success | case=index with updates
// CID-SHARED-171 | mode=success | case=index range query
// CID-SHARED-172 | mode=success | case=snapshot isolation
// CID-SHARED-173 | mode=success | case=reader isolation across handles
// CID-SHARED-174 | mode=error | case=syntax error detail
// CID-SHARED-175 | mode=error | case=unknown function error
// CID-SHARED-176 | mode=error | case=delete connected node error
// CID-SHARED-177 | mode=error | case=missing property returns null
// CID-SHARED-178 | mode=error | case=division by zero
// CID-SHARED-179 | mode=error | case=type error in arithmetic
// CID_REGISTRY_END

type NervusAddon = {
  Db: { open(path: string): NervusDb; openPaths(ndbPath: string, walPath: string): NervusDb };
  vacuum(path: string): {
    ndbPath: string;
    backupPath: string;
    oldNextPageId: number;
    newNextPageId: number;
    copiedDataPages: number;
    oldFilePages: number;
    newFilePages: number;
  };
  backup(path: string, backupDir: string): {
    id: string;
    createdAt: string;
    sizeBytes: number;
    fileCount: number;
    nervusdbVersion: string;
    checkpointTxid: number;
    checkpointEpoch: number;
  };
  bulkload(
    path: string,
    nodes: Array<{ externalId: number; label: string; properties?: Record<string, unknown> }>,
    edges: Array<{
      srcExternalId: number;
      relType: string;
      dstExternalId: number;
      properties?: Record<string, unknown>;
    }>
  ): void;
};

const addon = require("../native/nervusdb_node.node") as NervusAddon;

// ─── Test harness ───
let passed = 0;
let failed = 0;
let skipped = 0;
const failures: string[] = [];

function test(name: string, fn: () => void) {
  try {
    fn();
    passed++;
    console.log(`  ✅ ${name}`);
  } catch (e: any) {
    failed++;
    const msg = e?.message || String(e);
    failures.push(`${name}: ${msg}`);
    console.log(`  ❌ ${name}: ${msg}`);
  }
}

function skip(name: string, _reason?: string) {
  console.log(`  ℹ️  ${name} (${_reason || "note"})`);
}

function assert(cond: boolean, msg: string) {
  if (!cond) throw new Error(`Assertion failed: ${msg}`);
}

function assertEq(a: any, b: any, msg?: string) {
  const sa = JSON.stringify(a);
  const sb = JSON.stringify(b);
  if (sa !== sb) throw new Error(`${msg || "assertEq"}: ${sa} !== ${sb}`);
}

function assertThrows(fn: () => void, pattern?: string): string {
  try {
    fn();
    throw new Error("Expected error but none thrown");
  } catch (e: any) {
    const msg = e?.message || String(e);
    if (msg === "Expected error but none thrown") throw e;
    if (pattern && !msg.includes(pattern)) {
      throw new Error(`Error "${msg}" does not contain "${pattern}"`);
    }
    return msg;
  }
}

function freshDb(label?: string): { db: NervusDb; dbPath: string } {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), `ndb-test-${label || "x"}-`));
  const dbPath = path.join(dir, "test.ndb");
  const db = addon.Db.open(dbPath);
  return { db, dbPath };
}

// ═══════════════════════════════════════════════════════════════
console.log("\n🧪 NervusDB Node Binding — 全能力边界测试\n");

// ─── 1. 基础 CRUD ───
console.log("── 1. 基础 CRUD ──");

(() => {
  const { db } = freshDb("crud");

  test("CREATE single node", () => {
    const n = db.executeWrite("CREATE (n:Person {name: 'Alice', age: 30})");
    assert(n > 0, `expected created > 0, got ${n}`);
  });

  test("MATCH + RETURN node", () => {
    const rows = db.query("MATCH (n:Person {name: 'Alice'}) RETURN n");
    assert(rows.length === 1, `expected 1 row, got ${rows.length}`);
    const node = rows[0].n as any;
    assertEq(node.type, "node");
    assertEq(node.properties.name, "Alice");
    assertEq(node.properties.age, 30);
    assert(node.labels.includes("Person"), "missing label Person");
  });

  test("CREATE relationship", () => {
    db.executeWrite("CREATE (b:Person {name: 'Bob', age: 25})");
    db.executeWrite(
      "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS {since: 2020}]->(b)"
    );
    const rows = db.query(
      "MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a.name, r, b.name"
    );
    assert(rows.length >= 1, "expected at least 1 relationship row");
  });

  test("SET property on node", () => {
    db.executeWrite("MATCH (n:Person {name: 'Alice'}) SET n.email = 'alice@test.com'");
    const rows = db.query("MATCH (n:Person {name: 'Alice'}) RETURN n.email");
    assertEq(rows[0]["n.email"], "alice@test.com");
  });

  test("SET overwrite property", () => {
    db.executeWrite("MATCH (n:Person {name: 'Alice'}) SET n.age = 31");
    const rows = db.query("MATCH (n:Person {name: 'Alice'}) RETURN n.age");
    assertEq(rows[0]["n.age"], 31);
  });

  test("REMOVE property", () => {
    db.executeWrite("MATCH (n:Person {name: 'Alice'}) REMOVE n.email");
    const rows = db.query("MATCH (n:Person {name: 'Alice'}) RETURN n.email");
    assertEq(rows[0]["n.email"], null);
  });

  test("DELETE node (detach)", () => {
    db.executeWrite("CREATE (x:Temp {val: 'delete-me'})");
    const before = db.query("MATCH (x:Temp) RETURN count(x) AS c");
    assert((before[0].c as number) >= 1, "temp node should exist");
    db.executeWrite("MATCH (x:Temp {val: 'delete-me'}) DETACH DELETE x");
    const after = db.query("MATCH (x:Temp {val: 'delete-me'}) RETURN count(x) AS c");
    assertEq(after[0].c, 0);
  });

  test("DELETE relationship only", () => {
    db.executeWrite("CREATE (a:X)-[:R]->(b:Y)");
    db.executeWrite("MATCH (:X)-[r:R]->(:Y) DELETE r");
    const rows = db.query("MATCH (:X)-[r:R]->(:Y) RETURN count(r) AS c");
    assertEq(rows[0].c, 0);
  });

  test("multi-node CREATE in single statement", () => {
    db.executeWrite("CREATE (:Multi1 {v: 1}), (:Multi2 {v: 2})");
    const rows = db.query("MATCH (n:Multi1) RETURN count(n) AS c");
    assert((rows[0].c as number) >= 1, "multi-create should work");
  });

  db.close();
})();

// ─── 1b. RETURN 投影 ───
console.log("\n── 1b. RETURN 投影 ──");

(() => {
  const { db } = freshDb("return");
  db.executeWrite("CREATE (a:P {name: 'X', age: 10})-[:R {w: 5}]->(b:P {name: 'Y', age: 20})");

  test("RETURN scalar expression", () => {
    const rows = db.query("RETURN 1 + 2 AS sum");
    assertEq(rows[0].sum, 3);
  });

  test("RETURN property alias", () => {
    const rows = db.query("MATCH (n:P {name: 'X'}) RETURN n.name AS who");
    assertEq(rows[0].who, "X");
  });

  test("RETURN DISTINCT", () => {
    db.executeWrite("CREATE (:D {v: 1})");
    db.executeWrite("CREATE (:D {v: 1})");
    db.executeWrite("CREATE (:D {v: 2})");
    const rows = db.query("MATCH (n:D) RETURN DISTINCT n.v ORDER BY n.v");
    assertEq(rows.length, 2);
  });

  test("RETURN *", () => {
    const rows = db.query("MATCH (n:P {name: 'X'}) RETURN *");
    assert(rows.length >= 1, "RETURN * should work");
    assert("n" in rows[0], "should have n in result");
  });

  db.close();
})();

// ─── 2. 节点: 多标签 ───
console.log("\n── 2. 多标签节点 ──");

(() => {
  const { db } = freshDb("labels");

  test("CREATE node with multiple labels", () => {
    db.executeWrite("CREATE (n:Person:Employee:Manager {name: 'Carol'})");
    const rows = db.query("MATCH (n:Person:Employee {name: 'Carol'}) RETURN n");
    assert(rows.length === 1, "multi-label match failed");
    const node = rows[0].n as any;
    assert(node.labels.includes("Person"), "missing Person");
    assert(node.labels.includes("Employee"), "missing Employee");
    assert(node.labels.includes("Manager"), "missing Manager");
  });

  test("MATCH by single label subset", () => {
    const rows = db.query("MATCH (n:Manager) RETURN n.name");
    assert(rows.length >= 1, "should match by Manager label");
  });

  db.close();
})();

// ─── 3. 数据类型 ───
console.log("\n── 3. 数据类型 ──");

(() => {
  const { db } = freshDb("types");

  test("null property", () => {
    db.executeWrite("CREATE (n:T {val: null})");
    const rows = db.query("MATCH (n:T) RETURN n.val");
    assertEq(rows[0]["n.val"], null);
  });

  test("boolean properties", () => {
    db.executeWrite("CREATE (n:Bool {t: true, f: false})");
    const rows = db.query("MATCH (n:Bool) RETURN n.t, n.f");
    assertEq(rows[0]["n.t"], true);
    assertEq(rows[0]["n.f"], false);
  });

  test("integer property", () => {
    db.executeWrite("CREATE (n:Num {val: 42})");
    const rows = db.query("MATCH (n:Num) RETURN n.val");
    assertEq(rows[0]["n.val"], 42);
  });

  test("negative integer", () => {
    db.executeWrite("CREATE (n:Neg {val: -100})");
    const rows = db.query("MATCH (n:Neg) RETURN n.val");
    assertEq(rows[0]["n.val"], -100);
  });

  test("float property", () => {
    db.executeWrite("CREATE (n:Flt {val: 3.14})");
    const rows = db.query("MATCH (n:Flt) RETURN n.val");
    const v = rows[0]["n.val"] as number;
    assert(Math.abs(v - 3.14) < 0.001, `float mismatch: ${v}`);
  });

  test("string property with special chars", () => {
    db.executeWrite("CREATE (n:Str {val: 'hello \"world\" \\\\n'})");
    const rows = db.query("MATCH (n:Str) RETURN n.val");
    assert(typeof rows[0]["n.val"] === "string", "should be string");
  });

  test("list literal in RETURN", () => {
    const rows = db.query("RETURN [1, 2, 3] AS lst");
    const lst = rows[0].lst;
    assert(Array.isArray(lst), "should be array");
    assertEq(lst, [1, 2, 3]);
  });

  test("map literal in RETURN", () => {
    const rows = db.query("RETURN {a: 1, b: 'two'} AS m");
    const m = rows[0].m as any;
    assertEq(m.a, 1);
    assertEq(m.b, "two");
  });

  test("list property on node", () => {
    db.executeWrite("CREATE (n:Lst {tags: ['a', 'b', 'c']})");
    const rows = db.query("MATCH (n:Lst) RETURN n.tags");
    const tags = rows[0]["n.tags"];
    assert(Array.isArray(tags), "tags should be array");
    assertEq(tags, ["a", "b", "c"]);
  });

  db.close();
})();

// ─── 4. WHERE 过滤 ───
console.log("\n── 4. WHERE 过滤 ──");

(() => {
  const { db } = freshDb("where");
  db.executeWrite("CREATE (a:P {name: 'A', age: 20})");
  db.executeWrite("CREATE (b:P {name: 'B', age: 30})");
  db.executeWrite("CREATE (c:P {name: 'C', age: 40})");

  test("WHERE equality", () => {
    const rows = db.query("MATCH (n:P) WHERE n.age = 30 RETURN n.name");
    assertEq(rows.length, 1);
    assertEq(rows[0]["n.name"], "B");
  });

  test("WHERE comparison >", () => {
    const rows = db.query("MATCH (n:P) WHERE n.age > 25 RETURN n.name ORDER BY n.name");
    assertEq(rows.length, 2);
  });

  test("WHERE AND", () => {
    const rows = db.query("MATCH (n:P) WHERE n.age > 15 AND n.age < 35 RETURN n.name ORDER BY n.name");
    assertEq(rows.length, 2);
  });

  test("WHERE OR", () => {
    const rows = db.query("MATCH (n:P) WHERE n.name = 'A' OR n.name = 'C' RETURN n.name ORDER BY n.name");
    assertEq(rows.length, 2);
  });

  test("WHERE NOT", () => {
    const rows = db.query("MATCH (n:P) WHERE NOT n.name = 'B' RETURN n.name ORDER BY n.name");
    assertEq(rows.length, 2);
  });

  test("WHERE IN list", () => {
    const rows = db.query("MATCH (n:P) WHERE n.name IN ['A', 'C'] RETURN n.name ORDER BY n.name");
    assertEq(rows.length, 2);
  });

  test("WHERE STARTS WITH", () => {
    const rows = db.query("MATCH (n:P) WHERE n.name STARTS WITH 'A' RETURN n.name");
    assertEq(rows.length, 1);
  });

  test("WHERE CONTAINS", () => {
    db.executeWrite("CREATE (n:P {name: 'Alice', age: 50})");
    const rows = db.query("MATCH (n:P) WHERE n.name CONTAINS 'lic' RETURN n.name");
    assertEq(rows.length, 1);
  });

  test("WHERE ENDS WITH", () => {
    const rows = db.query("MATCH (n:P) WHERE n.name ENDS WITH 'e' RETURN n.name");
    assert(rows.length >= 1, "should find Alice");
  });

  test("WHERE IS NULL", () => {
    db.executeWrite("CREATE (n:P {name: 'NoAge'})");
    const rows = db.query("MATCH (n:P) WHERE n.age IS NULL RETURN n.name");
    assert(rows.length >= 1, "should find node without age");
  });

  test("WHERE IS NOT NULL", () => {
    const rows = db.query("MATCH (n:P) WHERE n.age IS NOT NULL RETURN n.name ORDER BY n.name");
    assert(rows.length >= 3, "should find nodes with age");
  });

  db.close();
})();

// ─── 5. 查询子句 ───
console.log("\n── 5. 查询子句 ──");

(() => {
  const { db } = freshDb("clauses");
  db.executeWrite("CREATE (:N {v: 3})");
  db.executeWrite("CREATE (:N {v: 1})");
  db.executeWrite("CREATE (:N {v: 2})");
  db.executeWrite("CREATE (:N {v: 5})");
  db.executeWrite("CREATE (:N {v: 4})");

  test("ORDER BY ASC", () => {
    const rows = db.query("MATCH (n:N) RETURN n.v ORDER BY n.v");
    const vals = rows.map((r: any) => r["n.v"]);
    assertEq(vals, [1, 2, 3, 4, 5]);
  });

  test("ORDER BY DESC", () => {
    const rows = db.query("MATCH (n:N) RETURN n.v ORDER BY n.v DESC");
    const vals = rows.map((r: any) => r["n.v"]);
    assertEq(vals, [5, 4, 3, 2, 1]);
  });

  test("LIMIT", () => {
    const rows = db.query("MATCH (n:N) RETURN n.v ORDER BY n.v LIMIT 3");
    assertEq(rows.length, 3);
  });

  test("SKIP", () => {
    const rows = db.query("MATCH (n:N) RETURN n.v ORDER BY n.v SKIP 2 LIMIT 2");
    assertEq(rows.length, 2);
    assertEq(rows[0]["n.v"], 3);
  });

  test("WITH pipe", () => {
    const rows = db.query("MATCH (n:N) WITH n.v AS val WHERE val > 3 RETURN val ORDER BY val");
    assertEq(rows.length, 2);
    assertEq(rows[0].val, 4);
  });

  test("UNWIND", () => {
    const rows = db.query("UNWIND [10, 20, 30] AS x RETURN x");
    assertEq(rows.length, 3);
    assertEq(rows[0].x, 10);
  });

  test("UNWIND + CREATE", () => {
    db.executeWrite("UNWIND [1, 2, 3] AS i CREATE (:UW {idx: i})");
    const rows = db.query("MATCH (n:UW) RETURN n.idx ORDER BY n.idx");
    assertEq(rows.length, 3);
  });

  test("UNION", () => {
    const rows = db.query("RETURN 1 AS x UNION RETURN 2 AS x");
    assertEq(rows.length, 2);
  });

  test("UNION ALL", () => {
    const rows = db.query("RETURN 1 AS x UNION ALL RETURN 1 AS x");
    assertEq(rows.length, 2);
  });

  test("OPTIONAL MATCH", () => {
    db.executeWrite("CREATE (:Lonely {name: 'solo'})");
    const rows = db.query(
      "MATCH (n:Lonely) OPTIONAL MATCH (n)-[r]->(m) RETURN n.name, r, m"
    );
    assert(rows.length >= 1, "should return at least 1 row");
    assertEq(rows[0].r, null);
    assertEq(rows[0].m, null);
  });

  db.close();
})();

// ─── 6. 聚合 ───
console.log("\n── 6. 聚合函数 ──");

(() => {
  const { db } = freshDb("agg");
  db.executeWrite("CREATE (:S {v: 10})");
  db.executeWrite("CREATE (:S {v: 20})");
  db.executeWrite("CREATE (:S {v: 30})");

  test("count()", () => {
    const rows = db.query("MATCH (n:S) RETURN count(n) AS c");
    assertEq(rows[0].c, 3);
  });

  test("sum()", () => {
    const rows = db.query("MATCH (n:S) RETURN sum(n.v) AS s");
    assertEq(rows[0].s, 60);
  });

  test("avg()", () => {
    const rows = db.query("MATCH (n:S) RETURN avg(n.v) AS a");
    assertEq(rows[0].a, 20);
  });

  test("min() / max()", () => {
    const rows = db.query("MATCH (n:S) RETURN min(n.v) AS lo, max(n.v) AS hi");
    assertEq(rows[0].lo, 10);
    assertEq(rows[0].hi, 30);
  });

  test("collect()", () => {
    const rows = db.query("MATCH (n:S) RETURN collect(n.v) AS vals");
    const vals = rows[0].vals as number[];
    assert(Array.isArray(vals), "collect should return array");
    assertEq(vals.length, 3);
  });

  test("count(DISTINCT)", () => {
    db.executeWrite("CREATE (:S {v: 10})");
    const rows = db.query("MATCH (n:S) RETURN count(DISTINCT n.v) AS c");
    assertEq(rows[0].c, 3);
  });

  test("GROUP BY (implicit)", () => {
    db.executeWrite("CREATE (:G {cat: 'a', v: 1})");
    db.executeWrite("CREATE (:G {cat: 'a', v: 2})");
    db.executeWrite("CREATE (:G {cat: 'b', v: 3})");
    const rows = db.query("MATCH (n:G) RETURN n.cat, sum(n.v) AS total ORDER BY n.cat");
    assertEq(rows.length, 2);
    assertEq(rows[0]["n.cat"], "a");
    assertEq(rows[0].total, 3);
  });

  db.close();
})();

// ─── 7. MERGE ───
console.log("\n── 7. MERGE ──");

(() => {
  const { db } = freshDb("merge");

  test("MERGE creates when not exists", () => {
    db.executeWrite("MERGE (n:M {key: 'x'})");
    const rows = db.query("MATCH (n:M {key: 'x'}) RETURN count(n) AS c");
    assertEq(rows[0].c, 1);
  });

  test("MERGE matches when exists", () => {
    db.executeWrite("MERGE (n:M {key: 'x'})");
    const rows = db.query("MATCH (n:M {key: 'x'}) RETURN count(n) AS c");
    assertEq(rows[0].c, 1, "should still be 1, not 2");
  });

  test("MERGE ON CREATE SET", () => {
    db.executeWrite("MERGE (n:M {key: 'y'}) ON CREATE SET n.created = true");
    const rows = db.query("MATCH (n:M {key: 'y'}) RETURN n.created");
    assertEq(rows[0]["n.created"], true);
  });

  test("MERGE ON MATCH SET", () => {
    db.executeWrite("MERGE (n:M {key: 'y'}) ON MATCH SET n.updated = true");
    const rows = db.query("MATCH (n:M {key: 'y'}) RETURN n.updated");
    assertEq(rows[0]["n.updated"], true);
  });

  test("MERGE relationship", () => {
    db.executeWrite("CREATE (:MA {id: 1})");
    db.executeWrite("CREATE (:MB {id: 2})");
    db.executeWrite("MATCH (a:MA), (b:MB) MERGE (a)-[:LINK]->(b)");
    db.executeWrite("MATCH (a:MA), (b:MB) MERGE (a)-[:LINK]->(b)");
    const rows = db.query("MATCH (:MA)-[r:LINK]->(:MB) RETURN count(r) AS c");
    assertEq(rows[0].c, 1, "MERGE rel should be idempotent");
  });

  db.close();
})();

// ─── 8. CASE 表达式 ───
console.log("\n── 8. CASE 表达式 ──");

(() => {
  const { db } = freshDb("case");
  db.executeWrite("CREATE (:C {v: 1})");
  db.executeWrite("CREATE (:C {v: 2})");
  db.executeWrite("CREATE (:C {v: 3})");

  test("simple CASE", () => {
    const rows = db.query(
      "MATCH (n:C) RETURN CASE n.v WHEN 1 THEN 'one' WHEN 2 THEN 'two' ELSE 'other' END AS label ORDER BY n.v"
    );
    assertEq(rows[0].label, "one");
    assertEq(rows[1].label, "two");
    assertEq(rows[2].label, "other");
  });

  test("generic CASE", () => {
    const rows = db.query(
      "MATCH (n:C) RETURN CASE WHEN n.v < 2 THEN 'low' WHEN n.v > 2 THEN 'high' ELSE 'mid' END AS cat ORDER BY n.v"
    );
    assertEq(rows[0].cat, "low");
    assertEq(rows[1].cat, "mid");
    assertEq(rows[2].cat, "high");
  });

  db.close();
})();

// ─── 9. 字符串函数 ───
console.log("\n── 9. 字符串函数 ──");

(() => {
  const { db } = freshDb("strfn");

  test("toString()", () => {
    const rows = db.query("RETURN toString(42) AS s");
    assertEq(rows[0].s, "42");
  });

  test("toUpper / toLower", () => {
    const rows = db.query("RETURN toUpper('hello') AS u, toLower('HELLO') AS l");
    assertEq(rows[0].u, "HELLO");
    assertEq(rows[0].l, "hello");
  });

  test("trim / lTrim / rTrim", () => {
    const rows = db.query("RETURN trim('  hi  ') AS t, lTrim('  hi') AS l, rTrim('hi  ') AS r");
    assertEq(rows[0].t, "hi");
    assertEq(rows[0].l, "hi");
    assertEq(rows[0].r, "hi");
  });

  test("substring", () => {
    const rows = db.query("RETURN substring('hello', 1, 3) AS s");
    assertEq(rows[0].s, "ell");
  });

  test("size() on string", () => {
    const rows = db.query("RETURN size('hello') AS s");
    assertEq(rows[0].s, 5);
  });

  test("replace()", () => {
    const rows = db.query("RETURN replace('hello world', 'world', 'nervus') AS s");
    assertEq(rows[0].s, "hello nervus");
  });

  test("left / right", () => {
    const rows = db.query("RETURN left('hello', 3) AS l, right('hello', 3) AS r");
    assertEq(rows[0].l, "hel");
    assertEq(rows[0].r, "llo");
  });

  db.close();
})();

// ─── 10. 数学运算 ───
console.log("\n── 10. 数学运算 ──");

(() => {
  const { db } = freshDb("math");

  test("arithmetic: + - * / %", () => {
    const rows = db.query("RETURN 10 + 3 AS a, 10 - 3 AS b, 10 * 3 AS c, 10 / 3 AS d, 10 % 3 AS e");
    assertEq(rows[0].a, 13);
    assertEq(rows[0].b, 7);
    assertEq(rows[0].c, 30);
    assert(typeof rows[0].d === "number", "division should return number");
    assertEq(rows[0].e, 1);
  });

  test("abs()", () => {
    const rows = db.query("RETURN abs(-5) AS v");
    assertEq(rows[0].v, 5);
  });

  test("toInteger / toFloat", () => {
    const rows = db.query("RETURN toInteger(3.7) AS i, toFloat(3) AS f");
    assertEq(rows[0].i, 3);
    assert(typeof rows[0].f === "number", "toFloat should return number");
  });

  test("sign()", () => {
    const rows = db.query("RETURN sign(-5) AS neg, sign(0) AS zero, sign(5) AS pos");
    assertEq(rows[0].neg, -1);
    assertEq(rows[0].zero, 0);
    assertEq(rows[0].pos, 1);
  });

  db.close();
})();

// ─── 11. 变长路径 ───
console.log("\n── 11. 变长路径 ──");

(() => {
  const { db } = freshDb("varlen");
  db.executeWrite("CREATE (a:V {name: 'A'})-[:NEXT]->(b:V {name: 'B'})-[:NEXT]->(c:V {name: 'C'})-[:NEXT]->(d:V {name: 'D'})");

  test("fixed length path *2", () => {
    const rows = db.query("MATCH (a:V {name: 'A'})-[:NEXT*2]->(c) RETURN c.name");
    assertEq(rows.length, 1);
    assertEq(rows[0]["c.name"], "C");
  });

  test("variable length path *1..3", () => {
    const rows = db.query("MATCH (a:V {name: 'A'})-[:NEXT*1..3]->(x) RETURN x.name ORDER BY x.name");
    assertEq(rows.length, 3);
  });

  test("variable length path *..2 (upper bound only)", () => {
    const rows = db.query("MATCH (a:V {name: 'A'})-[:NEXT*..2]->(x) RETURN x.name ORDER BY x.name");
    assertEq(rows.length, 2);
  });

  test("shortest path", () => {
    const rows = db.query("MATCH p = shortestPath((a:V {name: 'A'})-[:NEXT*]->(d:V {name: 'D'})) RETURN length(p) AS len");
    assertEq(rows[0].len, 3);
  });

  db.close();
})();

// ─── 12. EXISTS 子查询 ───
console.log("\n── 12. EXISTS 子查询 ──");

(() => {
  const { db } = freshDb("exists");
  db.executeWrite("CREATE (a:E {name: 'has-rel'})-[:R]->(b:E {name: 'target'})");
  db.executeWrite("CREATE (:E {name: 'no-rel'})");

  test("WHERE EXISTS pattern", () => {
    const rows = db.query("MATCH (n:E) WHERE EXISTS { (n)-[:R]->() } RETURN n.name");
    assertEq(rows.length, 1);
    assertEq(rows[0]["n.name"], "has-rel");
  });

  db.close();
})();

// ─── 13. FOREACH ───
console.log("\n── 13. FOREACH ──");

(() => {
  const { db } = freshDb("foreach");

  test("FOREACH create nodes", () => {
    db.executeWrite("FOREACH (i IN [1, 2, 3] | CREATE (:FE {idx: i}))");
    const rows = db.query("MATCH (n:FE) RETURN n.idx ORDER BY n.idx");
    assertEq(rows.length, 3);
  });

  db.close();
})();

// ─── 14. 事务 ───
console.log("\n── 14. 事务 (WriteTxn) ──");

(() => {
  test("beginWrite + query + commit", () => {
    const { db } = freshDb("txn-commit");
    const txn = db.beginWrite();
    txn.query("CREATE (:TX {v: 1})");
    txn.query("CREATE (:TX {v: 2})");
    const affected = txn.commit();
    assert(affected >= 2, `expected affected >= 2, got ${affected}`);
    const rows = db.query("MATCH (n:TX) RETURN n.v ORDER BY n.v");
    assertEq(rows.length, 2);
    db.close();
  });

  test("rollback discards staged queries", () => {
    const { db } = freshDb("txn-rollback");
    const txn = db.beginWrite();
    txn.query("CREATE (:TX {v: 99})");
    txn.rollback();
    const affected = txn.commit();
    assertEq(affected, 0);
    const rows = db.query("MATCH (n:TX {v: 99}) RETURN count(n) AS c");
    assertEq(rows[0].c, 0);
    db.close();
  });

  test("txn syntax error at enqueue time", () => {
    const { db } = freshDb("txn-syntax");
    const txn = db.beginWrite();
    assertThrows(() => txn.query("INVALID CYPHER !!!"));
    txn.rollback();
    db.close();
  });

  test("multiple txn commits are independent", () => {
    const { db } = freshDb("txn-ind");
    const txn1 = db.beginWrite();
    txn1.query("CREATE (:Ind {batch: 1})");
    txn1.commit();
    const txn2 = db.beginWrite();
    txn2.query("CREATE (:Ind {batch: 2})");
    txn2.commit();
    const rows = db.query("MATCH (n:Ind) RETURN n.batch ORDER BY n.batch");
    assertEq(rows.length, 2);
    db.close();
  });
})();

// ─── 15. 错误处理 ───
console.log("\n── 15. 错误处理 ──");

(() => {
  const { db } = freshDb("errors");

  test("syntax error in query()", () => {
    const msg = assertThrows(() => db.query("NOT VALID CYPHER"));
    assert(msg.includes("NERVUS_SYNTAX") || msg.includes("syntax") || msg.includes("parse"), `unexpected error: ${msg}`);
  });

  test("syntax error in executeWrite()", () => {
    assertThrows(() => db.executeWrite("BLAH BLAH"));
  });

  test("write via query() is rejected", () => {
    const msg = assertThrows(() => db.query("CREATE (:ShouldFail)"));
    assert(msg.includes("execute_write") || msg.includes("read"), `unexpected error: ${msg}`);
  });

  test("error payload is structured JSON", () => {
    try {
      db.query("INVALID!!!");
    } catch (e: any) {
      const msg = e.message || "";
      try {
        const payload = JSON.parse(msg);
        assert("code" in payload, "payload should have code");
        assert("category" in payload, "payload should have category");
        assert("message" in payload, "payload should have message");
      } catch {
        assert(msg.includes("NERVUS_") || msg.includes("syntax"), `error should be structured: ${msg}`);
      }
    }
  });

  test("operations after close() throw", () => {
    const { db: db2 } = freshDb("closed");
    db2.close();
    assertThrows(() => db2.query("RETURN 1"), "closed");
  });

  test("double close is safe", () => {
    const { db: db3 } = freshDb("dblclose");
    db3.close();
    db3.close();
  });

  db.close();
})();

// ─── 16. 关系方向 ───
console.log("\n── 16. 关系方向 ──");

(() => {
  const { db } = freshDb("direction");
  db.executeWrite("CREATE (a:D {name: 'A'})-[:TO]->(b:D {name: 'B'})");

  test("outgoing match ->", () => {
    const rows = db.query("MATCH (a:D {name: 'A'})-[:TO]->(b) RETURN b.name");
    assertEq(rows.length, 1);
    assertEq(rows[0]["b.name"], "B");
  });

  test("incoming match <-", () => {
    const rows = db.query("MATCH (b:D {name: 'B'})<-[:TO]-(a) RETURN a.name");
    assertEq(rows.length, 1);
    assertEq(rows[0]["a.name"], "A");
  });

  test("undirected match -[]-", () => {
    const rows = db.query("MATCH (a:D {name: 'A'})-[:TO]-(b) RETURN b.name");
    assert(rows.length >= 1, "undirected should match");
  });

  test("relationship properties", () => {
    db.executeWrite("CREATE (:RP {id: 1})-[:EDGE {weight: 0.5, label: 'test'}]->(:RP {id: 2})");
    const rows = db.query("MATCH ()-[r:EDGE]->() RETURN r");
    const rel = rows[0].r as any;
    assertEq(rel.type, "relationship");
    assertEq(rel.properties.weight, 0.5);
    assertEq(rel.properties.label, "test");
  });

  db.close();
})();

// ─── 17. 复杂图模式 ───
console.log("\n── 17. 复杂图模式 ──");

(() => {
  const { db } = freshDb("complex");

  test("triangle pattern", () => {
    db.executeWrite(
      "CREATE (a:T {name: 'a'})-[:E]->(b:T {name: 'b'})-[:E]->(c:T {name: 'c'})-[:E]->(a)"
    );
    const rows = db.query(
      "MATCH (a:T)-[:E]->(b:T)-[:E]->(c:T)-[:E]->(a) RETURN a.name, b.name, c.name"
    );
    assert(rows.length >= 1, "should find triangle");
  });

  test("multi-hop with WHERE", () => {
    db.executeWrite(
      "CREATE (:H {lv: 0})-[:STEP]->(:H {lv: 1})-[:STEP]->(:H {lv: 2})-[:STEP]->(:H {lv: 3})"
    );
    const rows = db.query(
      "MATCH (a:H)-[:STEP]->(b:H)-[:STEP]->(c:H) WHERE a.lv = 0 AND c.lv = 2 RETURN b.lv"
    );
    assertEq(rows.length, 1);
    assertEq(rows[0]["b.lv"], 1);
  });

  test("multiple MATCH clauses", () => {
    db.executeWrite("CREATE (:MM {id: 'x'})");
    db.executeWrite("CREATE (:MM {id: 'y'})");
    const rows = db.query(
      "MATCH (a:MM {id: 'x'}) MATCH (b:MM {id: 'y'}) RETURN a.id, b.id"
    );
    assertEq(rows.length, 1);
    assertEq(rows[0]["a.id"], "x");
    assertEq(rows[0]["b.id"], "y");
  });

  db.close();
})();

// ─── 18. 大批量写入 ───
console.log("\n── 18. 批量写入性能 ──");

(() => {
  const { db } = freshDb("bulk");

  test("batch create 1000 nodes", () => {
    const start = Date.now();
    for (let i = 0; i < 1000; i++) {
      db.executeWrite(`CREATE (:Bulk {idx: ${i}})`);
    }
    const elapsed = Date.now() - start;
    const rows = db.query("MATCH (n:Bulk) RETURN count(n) AS c");
    assertEq(rows[0].c, 1000);
    console.log(`    (1000 nodes in ${elapsed}ms, ${(1000 / elapsed * 1000).toFixed(0)} ops/s)`);
  });

  test("batch query 1000 nodes", () => {
    const start = Date.now();
    const rows = db.query("MATCH (n:Bulk) RETURN n.idx ORDER BY n.idx LIMIT 1000");
    const elapsed = Date.now() - start;
    assertEq(rows.length, 1000);
    console.log(`    (query 1000 in ${elapsed}ms)`);
  });

  test("UNWIND batch create", () => {
    const items = Array.from({ length: 100 }, (_, i) => i);
    const start = Date.now();
    db.executeWrite(`UNWIND [${items.join(",")}] AS i CREATE (:UBulk {idx: i})`);
    const elapsed = Date.now() - start;
    const rows = db.query("MATCH (n:UBulk) RETURN count(n) AS c");
    assertEq(rows[0].c, 100);
    console.log(`    (UNWIND 100 in ${elapsed}ms)`);
  });

  db.close();
})();

// ─── 19. 数据库持久化 ───
console.log("\n── 19. 持久化 (close + reopen) ──");

(() => {
  const { db, dbPath } = freshDb("persist");
  db.executeWrite("CREATE (:Persist {key: 'survives'})");
  db.close();

  const db2 = addon.Db.open(dbPath);
  test("data survives close + reopen", () => {
    const rows = db2.query("MATCH (n:Persist) RETURN n.key");
    assertEq(rows.length, 1);
    assertEq(rows[0]["n.key"], "survives");
  });
  db2.close();
})();

// ─── 20. 边界情况 ───
console.log("\n── 20. 边界情况 ──");

(() => {
  const { db } = freshDb("edge");

  test("empty result set", () => {
    const rows = db.query("MATCH (n:NonExistent) RETURN n");
    assertEq(rows.length, 0);
  });

  test("RETURN literal without MATCH", () => {
    const rows = db.query("RETURN 'hello' AS greeting, 42 AS num, true AS flag, null AS nothing");
    assertEq(rows[0].greeting, "hello");
    assertEq(rows[0].num, 42);
    assertEq(rows[0].flag, true);
    assertEq(rows[0].nothing, null);
  });

  test("empty string property", () => {
    db.executeWrite("CREATE (:ES {val: ''})");
    const rows = db.query("MATCH (n:ES) RETURN n.val");
    assertEq(rows[0]["n.val"], "");
  });

  test("large string property", () => {
    const big = "x".repeat(10000);
    db.executeWrite(`CREATE (:Big {val: '${big}'})`);
    const rows = db.query("MATCH (n:Big) RETURN size(n.val) AS len");
    assertEq(rows[0].len, 10000);
  });

  test("node with many properties", () => {
    const props = Array.from({ length: 50 }, (_, i) => `p${i}: ${i}`).join(", ");
    db.executeWrite(`CREATE (:ManyProps {${props}})`);
    const rows = db.query("MATCH (n:ManyProps) RETURN n");
    const node = rows[0].n as any;
    assertEq(node.properties.p0, 0);
    assertEq(node.properties.p49, 49);
  });

  test("self-loop relationship", () => {
    db.executeWrite("CREATE (n:Loop {name: 'self'})-[:SELF]->(n)");
    const rows = db.query("MATCH (n:Loop)-[:SELF]->(n) RETURN n.name");
    assertEq(rows.length, 1);
  });

  db.close();
})();

// ─── 21. API 对齐（openPaths / 维护能力） ───
console.log("\n── 21. API 对齐（openPaths / 维护能力） ──");

(() => {
  test("Db.openPaths + path getters", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "ndb-node-openpaths-"));
    const ndbPath = path.join(dir, "openpaths.ndb");
    const walPath = path.join(dir, "openpaths.wal");
    const db = addon.Db.openPaths(ndbPath, walPath);
    assertEq(db.ndbPath, ndbPath);
    assertEq(db.walPath, walPath);
    db.executeWrite("CREATE (:OpenPaths {ok: true})");
    const rows = db.query("MATCH (n:OpenPaths) RETURN count(n) AS c");
    assertEq(rows[0].c, 1);
    db.close();
  });

  test("createIndex + checkpoint + compact", () => {
    const { db } = freshDb("maintenance");
    db.executeWrite("CREATE (:Idx {email: 'a@test.com'})");
    db.createIndex("Idx", "email");
    db.checkpoint();
    db.compact();
    const rows = db.query("MATCH (n:Idx {email: 'a@test.com'}) RETURN count(n) AS c");
    assertEq(rows[0].c, 1);
    db.close();
  });

  test("searchVector returns nearest hit", () => {
    const { db } = freshDb("vector-api");
    const txn = db.beginWrite();
    const label = txn.getOrCreateLabel("Vec");
    const node = txn.createNode(10001, label);
    txn.setVector(node, [0.9, 0.1, 0.0]);
    txn.commit();
    const hits = db.searchVector([1.0, 0.0, 0.0], 1);
    assert(hits.length >= 1, "searchVector should return at least one hit");
    assert(typeof hits[0].nodeId === "number", "hit.nodeId should be number");
    assert(typeof hits[0].distance === "number", "hit.distance should be number");
    db.close();
  });

  test("module backup + vacuum + bulkload", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "ndb-node-maint-"));
    const dbPath = path.join(dir, "main.ndb");
    const bulkPath = path.join(dir, "bulk.ndb");
    const backupDir = path.join(dir, "backups");
    fs.mkdirSync(backupDir, { recursive: true });

    const db = addon.Db.open(dbPath);
    db.executeWrite("CREATE (:Maint {k: 'v'})");
    db.close();

    const backupInfo = addon.backup(dbPath, backupDir);
    assert(typeof backupInfo.id === "string" && backupInfo.id.length > 0, "backup id should exist");
    assert(backupInfo.fileCount >= 1, `backup fileCount should be >=1, got ${backupInfo.fileCount}`);

    addon.bulkload(
      bulkPath,
      [{ externalId: 20001, label: "BulkNode", properties: { name: "bulk-a" } }],
      []
    );

    const vacuumReport = addon.vacuum(dbPath);
    assert(vacuumReport.newFilePages > 0, "vacuum newFilePages should be > 0");

    const reopened = addon.Db.open(bulkPath);
    const rows = reopened.query("MATCH (n:BulkNode {name: 'bulk-a'}) RETURN count(n) AS c");
    assertEq(rows[0].c, 1);
    reopened.close();
  });
})();

// ─── 22. WriteTxn 低层 API 对齐 ───
console.log("\n── 22. WriteTxn Low-Level API ──");

(() => {
  test("low-level node/edge/property lifecycle", () => {
    const { db } = freshDb("txn-low-level");
    const txn1 = db.beginWrite();
    const label = txn1.getOrCreateLabel("LL");
    const rel = txn1.getOrCreateRelType("LL_REL");
    const a = txn1.createNode(30001, label);
    const b = txn1.createNode(30002, label);
    txn1.createEdge(a, rel, b);
    txn1.setNodeProperty(a, "name", "alpha");
    txn1.setEdgeProperty(a, rel, b, "weight", 3);
    txn1.commit();

    const rows1 = db.query("MATCH (x:LL)-[r:LL_REL]->(y:LL) RETURN x.name AS name, r.weight AS w");
    assertEq(rows1.length, 1);
    assertEq(rows1[0].name, "alpha");
    assertEq(rows1[0].w, 3);

    const txn2 = db.beginWrite();
    txn2.removeNodeProperty(a, "name");
    txn2.removeEdgeProperty(a, rel, b, "weight");
    txn2.tombstoneEdge(a, rel, b);
    txn2.tombstoneNode(b);
    txn2.commit();

    const rows2 = db.query("MATCH (x:LL)-[r:LL_REL]->(y:LL) RETURN count(r) AS c");
    assertEq(rows2[0].c, 0);
    db.close();
  });
})();

// ─── 36. UNWIND (expanded) ───
console.log("\n── 36. UNWIND (expanded) ──");

(() => {
  const { db } = freshDb("unwind-exp");

  test("UNWIND basic list", () => {
    const rows = db.query("UNWIND [1, 2, 3] AS x RETURN x ORDER BY x");
    assertEq(rows.length, 3);
    assertEq(rows[0].x, 1);
    assertEq(rows[2].x, 3);
  });

  test("UNWIND with CREATE", () => {
    db.executeWrite("UNWIND [10, 20, 30] AS v CREATE (:UW {val: v})");
    const rows = db.query("MATCH (n:UW) RETURN n.val ORDER BY n.val");
    assertEq(rows.length, 3);
    assertEq(rows[0]["n.val"], 10);
  });

  test("UNWIND nested list", () => {
    const rows = db.query("UNWIND [[1,2],[3,4]] AS sub UNWIND sub AS x RETURN x ORDER BY x");
    assertEq(rows.length, 4);
    assertEq(rows[0].x, 1);
    assertEq(rows[3].x, 4);
  });

  test("UNWIND empty list", () => {
    const rows = db.query("UNWIND [] AS x RETURN x");
    assertEq(rows.length, 0);
  });

  test("UNWIND range()", () => {
    const rows = db.query("UNWIND range(1, 5) AS x RETURN x ORDER BY x");
    assertEq(rows.length, 5);
    assertEq(rows[0].x, 1);
    assertEq(rows[4].x, 5);
  });

  db.close();
})();

// ─── 37. UNION / UNION ALL ───
console.log("\n── 37. UNION / UNION ALL ──");

(() => {
  const { db } = freshDb("union-exp");
  db.executeWrite("CREATE (:UA {name: 'Alice'}), (:UB {name: 'Bob'})");

  test("UNION ALL returns all rows", () => {
    const rows = db.query("MATCH (n:UA) RETURN n.name AS name UNION ALL MATCH (n:UB) RETURN n.name AS name");
    assertEq(rows.length, 2);
  });

  test("UNION deduplicates", () => {
    db.executeWrite("CREATE (:UC {name: 'Same'})");
    db.executeWrite("CREATE (:UD {name: 'Same'})");
    const rows = db.query("MATCH (n:UC) RETURN n.name AS name UNION MATCH (n:UD) RETURN n.name AS name");
    assertEq(rows.length, 1);
  });

  test("UNION with three branches", () => {
    const rows = db.query("RETURN 1 AS x UNION RETURN 2 AS x UNION RETURN 3 AS x");
    assertEq(rows.length, 3);
  });

  test("UNION with MATCH branches", () => {
    db.executeWrite("CREATE (:UE {v: 'e'})");
    db.executeWrite("CREATE (:UF {v: 'f'})");
    const rows = db.query("MATCH (n:UE) RETURN n.v AS v UNION MATCH (n:UF) RETURN n.v AS v");
    assertEq(rows.length, 2);
  });

  db.close();
})();

// ─── 38. WITH pipeline ───
console.log("\n── 38. WITH pipeline ──");

(() => {
  const { db } = freshDb("with-exp");
  db.executeWrite("CREATE (:WP {name: 'A', score: 10}), (:WP {name: 'B', score: 20}), (:WP {name: 'C', score: 10})");

  test("WITH + aggregation pipeline", () => {
    const rows = db.query("MATCH (n:WP) WITH n.score AS s, count(*) AS cnt RETURN s, cnt ORDER BY s");
    assertEq(rows.length, 2);
  });

  test("WITH DISTINCT", () => {
    const rows = db.query("MATCH (n:WP) WITH DISTINCT n.score AS s RETURN s ORDER BY s");
    assertEq(rows.length, 2);
    assertEq(rows[0].s, 10);
    assertEq(rows[1].s, 20);
  });

  test("multi-stage WITH", () => {
    const rows = db.query("MATCH (n:WP) WITH n.name AS name, n.score AS score WHERE score > 15 WITH name RETURN name");
    assertEq(rows.length, 1);
    assertEq(rows[0].name, "B");
  });

  db.close();
})();

// ─── 39. ORDER BY + SKIP + LIMIT combined ───
console.log("\n── 39. ORDER BY + SKIP + LIMIT ──");

(() => {
  const { db } = freshDb("pagination");
  for (let i = 1; i <= 10; i++) {
    db.executeWrite(`CREATE (:Page {idx: ${i}})`);
  }

  test("ORDER BY + LIMIT", () => {
    const rows = db.query("MATCH (n:Page) RETURN n.idx ORDER BY n.idx LIMIT 3");
    assertEq(rows.length, 3);
    assertEq(rows[0]["n.idx"], 1);
    assertEq(rows[2]["n.idx"], 3);
  });

  test("ORDER BY + SKIP + LIMIT", () => {
    const rows = db.query("MATCH (n:Page) RETURN n.idx ORDER BY n.idx SKIP 3 LIMIT 3");
    assertEq(rows.length, 3);
    assertEq(rows[0]["n.idx"], 4);
    assertEq(rows[2]["n.idx"], 6);
  });

  test("ORDER BY DESC + LIMIT", () => {
    const rows = db.query("MATCH (n:Page) RETURN n.idx ORDER BY n.idx DESC LIMIT 2");
    assertEq(rows.length, 2);
    assertEq(rows[0]["n.idx"], 10);
  });

  test("SKIP beyond results", () => {
    const rows = db.query("MATCH (n:Page) RETURN n.idx SKIP 100");
    assertEq(rows.length, 0);
  });

  db.close();
})();

// ─── 40. Null handling ───
console.log("\n── 40. Null handling ──");

(() => {
  const { db } = freshDb("null-exp");
  db.executeWrite("CREATE (:NL {name: 'has-val', val: 42})");
  db.executeWrite("CREATE (:NL {name: 'no-val'})");

  test("IS NULL filter", () => {
    const rows = db.query("MATCH (n:NL) WHERE n.val IS NULL RETURN n.name");
    assertEq(rows.length, 1);
    assertEq(rows[0]["n.name"], "no-val");
  });

  test("IS NOT NULL filter", () => {
    const rows = db.query("MATCH (n:NL) WHERE n.val IS NOT NULL RETURN n.name");
    assertEq(rows.length, 1);
    assertEq(rows[0]["n.name"], "has-val");
  });

  test("COALESCE", () => {
    const rows = db.query("MATCH (n:NL) RETURN coalesce(n.val, -1) AS v ORDER BY v");
    assertEq(rows[0].v, -1);
    assertEq(rows[1].v, 42);
  });

  test("COALESCE first non-null", () => {
    const rows = db.query("RETURN coalesce(null, null, 42) AS v");
    assertEq(rows[0].v, 42);
  });

  test("null arithmetic propagation", () => {
    const rows = db.query("RETURN null + 1 AS r");
    assertEq(rows[0].r, null);
  });

  test("null comparison", () => {
    const rows = db.query("RETURN null = null AS r");
    assertEq(rows[0].r, null);
  });

  db.close();
})();

// ─── 41. Type conversion functions ───
console.log("\n── 41. Type conversion ──");

(() => {
  const { db } = freshDb("typeconv");

  test("toInteger", () => {
    const rows = db.query("RETURN toInteger('42') AS v");
    assertEq(rows[0].v, 42);
  });

  test("toInteger from float", () => {
    const rows = db.query("RETURN toInteger(3.9) AS v");
    assertEq(rows[0].v, 3);
  });

  test("toFloat", () => {
    const rows = db.query("RETURN toFloat('3.14') AS v");
    assert(Math.abs((rows[0].v as number) - 3.14) < 0.01, `expected ~3.14, got ${rows[0].v}`);
  });

  test("toFloat from int", () => {
    const rows = db.query("RETURN toFloat(42) AS v");
    assert(Math.abs((rows[0].v as number) - 42.0) < 0.001, `expected 42.0, got ${rows[0].v}`);
  });

  test("toString", () => {
    const rows = db.query("RETURN toString(42) AS v");
    assertEq(rows[0].v, "42");
  });

  test("toString from bool", () => {
    const rows = db.query("RETURN toString(true) AS v");
    assertEq(rows[0].v, "true");
  });

  test("toBoolean", () => {
    const rows = db.query("RETURN toBoolean('true') AS v");
    assertEq(rows[0].v, true);
  });

  db.close();
})();

// ─── 42. Math functions (full) ───
console.log("\n── 42. Math functions ──");

(() => {
  const { db } = freshDb("math-full");

  test("abs", () => {
    const rows = db.query("RETURN abs(-5) AS v");
    assertEq(rows[0].v, 5);
  });

  test("ceil", () => {
    const rows = db.query("RETURN ceil(2.3) AS v");
    assertEq(rows[0].v, 3);
  });

  test("floor", () => {
    const rows = db.query("RETURN floor(2.7) AS v");
    assertEq(rows[0].v, 2.0);
  });

  test("round", () => {
    const rows = db.query("RETURN round(2.5) AS v");
    assertEq(rows[0].v, 3.0);
  });

  test("sign", () => {
    const rows = db.query("RETURN sign(-10) AS neg, sign(0) AS zero, sign(5) AS pos");
    assertEq(rows[0].neg, -1);
    assertEq(rows[0].zero, 0);
    assertEq(rows[0].pos, 1);
  });

  test("sqrt", () => {
    const rows = db.query("RETURN sqrt(16) AS v");
    assertEq(rows[0].v, 4.0);
  });

  test("log", () => {
    const rows = db.query("RETURN log(1) AS v");
    assertEq(rows[0].v, 0.0);
  });

  test("e()", () => {
    const rows = db.query("RETURN e() AS v");
    assert(Math.abs((rows[0].v as number) - Math.E) < 0.001, `e() should be ~2.718`);
  });

  test("pi()", () => {
    const rows = db.query("RETURN pi() AS v");
    assert(Math.abs((rows[0].v as number) - Math.PI) < 0.001, `pi() should be ~3.14159`);
  });

  test("rand() returns 0..1", () => {
    const rows = db.query("RETURN rand() AS r");
    assert((rows[0].r as number) >= 0 && (rows[0].r as number) < 1, `rand() should be in [0,1)`);
  });

  db.close();
})();

// ─── 43. String functions (expanded) ───
console.log("\n── 43. String functions (expanded) ──");

(() => {
  const { db } = freshDb("str-exp");

  test("replace", () => {
    const rows = db.query("RETURN replace('hello world', 'world', 'graph') AS v");
    assertEq(rows[0].v, "hello graph");
  });

  test("split", () => {
    const rows = db.query("RETURN split('a,b,c', ',') AS v");
    assertEq((rows[0].v as any).length, 3);
    assertEq((rows[0].v as any)[0], "a");
  });

  test("reverse", () => {
    const rows = db.query("RETURN reverse('abc') AS v");
    assertEq(rows[0].v, "cba");
  });

  test("trim / ltrim / rtrim", () => {
    const rows = db.query("RETURN trim('  hi  ') AS t, lTrim('  hi') AS l, rTrim('hi  ') AS r");
    assertEq(rows[0].t, "hi");
    assertEq(rows[0].l, "hi");
    assertEq(rows[0].r, "hi");
  });

  test("lTrim and rTrim standalone", () => {
    const rows = db.query("RETURN lTrim('  hi') AS l, rTrim('hi  ') AS r");
    assertEq(rows[0].l, "hi");
    assertEq(rows[0].r, "hi");
  });

  test("substring", () => {
    const rows = db.query("RETURN substring('hello', 1, 3) AS v");
    assertEq(rows[0].v, "ell");
  });

  db.close();
})();

// ─── 44. List operations ───
console.log("\n── 44. List operations ──");

(() => {
  const { db } = freshDb("list-ops");

  test("range function", () => {
    const rows = db.query("RETURN range(1, 5) AS r");
    assertEq((rows[0].r as any).length, 5);
    assertEq((rows[0].r as any)[0], 1);
    assertEq((rows[0].r as any)[4], 5);
  });

  test("range with step", () => {
    const rows = db.query("RETURN range(0, 10, 3) AS r");
    assertEq((rows[0].r as any).length, 4);
    assertEq((rows[0].r as any)[0], 0);
    assertEq((rows[0].r as any)[3], 9);
  });

  test("list index access", () => {
    const rows = db.query("RETURN [10, 20, 30][1] AS v");
    assertEq(rows[0].v, 20);
  });

  test("size of list", () => {
    const rows = db.query("RETURN size([1, 2, 3, 4]) AS v");
    assertEq(rows[0].v, 4);
  });

  test("list comprehension", () => {
    const rows = db.query("RETURN [x IN [1, 2, 3, 4] WHERE x > 2] AS v");
    assertEq((rows[0].v as any).length, 2);
  });

  test("reduce", () => {
    const rows = db.query("RETURN reduce(acc = 0, x IN [1, 2, 3] | acc + x) AS v");
    assertEq(rows[0].v, 6);
  });

  db.close();
})();

// ─── 45. Map operations ───
console.log("\n── 45. Map operations ──");

(() => {
  const { db } = freshDb("map-ops");

  test("map literal", () => {
    const rows = db.query("RETURN {name: 'Alice', age: 30} AS m");
    assertEq((rows[0].m as any).name, "Alice");
    assertEq((rows[0].m as any).age, 30);
  });

  test("map access", () => {
    const rows = db.query("WITH {x: 1, y: 2} AS m RETURN m.x AS v");
    assertEq(rows[0].v, 1);
  });

  test("nested map", () => {
    const rows = db.query("RETURN {outer: {inner: 42}} AS m");
    assertEq((rows[0].m as any).outer.inner, 42);
  });

  test("keys function", () => {
    db.executeWrite("CREATE (:KF {a: 1, b: 2, c: 3})");
    const rows = db.query("MATCH (n:KF) RETURN keys(n) AS k");
    assert((rows[0].k as any).length >= 3, "should have at least 3 keys");
  });

  db.close();
})();

// ─── 46. Multiple MATCH ───
console.log("\n── 46. Multiple MATCH ──");

(() => {
  const { db } = freshDb("multi-match");
  db.executeWrite("CREATE (:MA {id: 1}), (:MA {id: 2}), (:MB {id: 3})");

  test("cartesian product", () => {
    const rows = db.query("MATCH (a:MA) MATCH (b:MB) RETURN a.id, b.id");
    assertEq(rows.length, 2);
  });

  test("correlated MATCH", () => {
    db.executeWrite("CREATE (:MC {name: 'x'})-[:LINK]->(:MD {name: 'y'})");
    const rows = db.query("MATCH (a:MC) MATCH (a)-[:LINK]->(b) RETURN a.name, b.name");
    assertEq(rows.length, 1);
    assertEq(rows[0]["b.name"], "y");
  });

  test("independent MATCH", () => {
    db.executeWrite("CREATE (:ME {v: 'a'})");
    db.executeWrite("CREATE (:MF {v: 'b'})");
    const rows = db.query("MATCH (a:ME) MATCH (b:MF) RETURN a.v AS av, b.v AS bv");
    assertEq(rows.length, 1);
    assertEq(rows[0].av, "a");
    assertEq(rows[0].bv, "b");
  });

  db.close();
})();

// ─── 47. REMOVE clause ───
console.log("\n── 47. REMOVE clause ──");

(() => {
  const { db } = freshDb("remove-exp");

  test("REMOVE property", () => {
    db.executeWrite("CREATE (:RM {name: 'test', extra: 'gone'})");
    db.executeWrite("MATCH (n:RM) REMOVE n.extra");
    const rows = db.query("MATCH (n:RM) RETURN n.extra AS v");
    assertEq(rows[0].v, null);
  });

  test("REMOVE label", () => {
    db.executeWrite("CREATE (:RLabel:Extra {name: 'labeled'})");
    db.executeWrite("MATCH (n:RLabel:Extra) REMOVE n:Extra");
    const rows = db.query("MATCH (n:Extra) RETURN count(n) AS c");
    assertEq(rows[0].c, 0);
  });

  test("REMOVE multiple properties", () => {
    db.executeWrite("CREATE (:RM2 {a: 1, b: 2, c: 3})");
    db.executeWrite("MATCH (n:RM2) REMOVE n.a, n.b");
    const rows = db.query("MATCH (n:RM2) RETURN n.a AS a, n.b AS b, n.c AS c");
    assertEq(rows[0].a, null);
    assertEq(rows[0].b, null);
    assertEq(rows[0].c, 3);
  });

  db.close();
})();

// ─── 48. Parameter queries ───
console.log("\n── 48. Parameter queries ──");

(() => {
  const { db } = freshDb("params-exp");
  db.executeWrite("CREATE (:PM {name: 'Alice', age: 30})");

  test("param in WHERE", () => {
    const rows = db.query("MATCH (n:PM) WHERE n.name = $name RETURN n.age", { name: "Alice" });
    assertEq(rows.length, 1);
    assertEq(rows[0]["n.age"], 30);
  });

  test("param in CREATE", () => {
    db.executeWrite("CREATE (:PM {name: $name, age: $age})", { name: "Bob", age: 25 });
    const rows = db.query("MATCH (n:PM {name: 'Bob'}) RETURN n.age");
    assertEq(rows[0]["n.age"], 25);
  });

  test("multiple params", () => {
    const rows = db.query("MATCH (n:PM) WHERE n.age >= $min AND n.age <= $max RETURN n.name ORDER BY n.name", { min: 25, max: 30 });
    assertEq(rows.length, 2);
  });

  test("string param return", () => {
    const rows = db.query("RETURN $greeting AS v", { greeting: "hello" });
    assertEq(rows[0].v, "hello");
  });

  db.close();
})();

// ─── 49. EXPLAIN ───
console.log("\n── 49. EXPLAIN ──");

(() => {
  const { db } = freshDb("explain");
  db.executeWrite("CREATE (:EX {name: 'test'})");

  test("EXPLAIN returns plan", () => {
    const rows = db.query("EXPLAIN MATCH (n:EX) RETURN n");
    assert(rows.length >= 1, "EXPLAIN should return at least one row");
  });

  db.close();
})();

// ─── 50. Index operations ───
console.log("\n── 50. Index operations ──");

(() => {
  const { db } = freshDb("index-exp");

  test("create index and query", () => {
    db.executeWrite("CREATE (:IX {email: 'a@test.com'}), (:IX {email: 'b@test.com'})");
    db.createIndex("IX", "email");
    const rows = db.query("MATCH (n:IX {email: 'a@test.com'}) RETURN n.email");
    assertEq(rows.length, 1);
    assertEq(rows[0]["n.email"], "a@test.com");
  });

  test("index with updates", () => {
    db.executeWrite("CREATE (:IX2 {email: 'a@b.com'})");
    db.createIndex("IX2", "email");
    db.executeWrite("CREATE (:IX2 {email: 'c@d.com'})");
    const rows = db.query("MATCH (n:IX2 {email: 'c@d.com'}) RETURN n.email");
    assertEq(rows.length, 1);
  });

  test("index range query", () => {
    for (let i = 0; i < 50; i++) {
      db.executeWrite(`CREATE (:IX3 {v: ${i}})`);
    }
    db.createIndex("IX3", "v");
    const rows = db.query("MATCH (n:IX3) WHERE n.v >= 40 RETURN n.v ORDER BY n.v");
    assertEq(rows.length, 10);
    assertEq(rows[0]["n.v"], 40);
  });

  db.close();
})();

// ─── 51. Concurrent snapshots ───
console.log("\n── 51. Concurrent snapshots ──");

(() => {
  const { db, dbPath } = freshDb("concurrent");

  test("snapshot isolation", () => {
    db.executeWrite("CREATE (:SI {v: 'before'})");
    const snap1 = db.query("MATCH (n:SI) RETURN count(n) AS c");
    db.executeWrite("CREATE (:SI {v: 'after'})");
    const snap2 = db.query("MATCH (n:SI) RETURN count(n) AS c");
    assertEq(snap1[0].c, 1);
    assertEq(snap2[0].c, 2);
  });

  test("reader isolation across handles", () => {
    db.executeWrite("CREATE (:SH {v: 1})");
    const first = db.query("MATCH (n:SH) RETURN count(n) AS c");
    db.executeWrite("CREATE (:SH {v: 2})");
    const db2 = addon.Db.open(dbPath);
    const second = db2.query("MATCH (n:SH) RETURN count(n) AS c");
    assertEq(first[0].c, 1);
    assertEq(second[0].c, 2);
    db2.close();
  });

  db.close();
})();

// ─── 52. Error handling (expanded) ───
console.log("\n── 52. Error handling (expanded) ──");

(() => {
  const { db } = freshDb("errors-exp");

  test("syntax error detail", () => {
    assertThrows(() => db.query("MATC (n) RETURN n"));
  });

  test("unknown function error", () => {
    assertThrows(() => db.query("RETURN nonExistentFunc(1)"));
  });

  test("delete connected node error", () => {
    db.executeWrite("CREATE (:DE {id: 1})-[:R]->(:DE {id: 2})");
    const msg = assertThrows(() => db.executeWrite("MATCH (n:DE {id: 1}) DELETE n"));
    assert(msg.includes("DETACH DELETE") || msg.includes("execute"), `unexpected error: ${msg}`);
  });

  test("missing property returns null", () => {
    db.executeWrite("CREATE (:NP {name: 'test'})");
    const rows = db.query("MATCH (n:NP) RETURN n.nonexistent AS v");
    assertEq(rows[0].v, null);
  });

  test("division by zero", () => {
    const rows = db.query("RETURN 1/0 AS v");
    assertEq(rows[0].v, null);
  });

  test("type error in arithmetic", () => {
    const rows = db.query("RETURN 'hello' + 1 AS v");
    assertEq(rows[0].v, null);
  });

  db.close();
})();

// ═══════════════════════════════════════════════════════════════
// Summary
console.log("\n" + "═".repeat(60));
console.log(`🧪 测试完成: ${passed} passed, ${failed} failed, ${skipped} skipped`);
if (failures.length > 0) {
  console.log("\n❌ 失败列表:");
  failures.forEach((f) => console.log(`  - ${f}`));
}
console.log("═".repeat(60));
process.exit(failed > 0 ? 1 : 0);

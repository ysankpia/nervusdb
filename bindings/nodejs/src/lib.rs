#[macro_use]
extern crate napi_derive;

// 显式导入，**不用 `bindgen_prelude::*`**：那个 glob 带入 napi 自己的
// `Result<T, S = Status>` 别名并遮蔽 `std::result::Result`，于是本文件里随处可见的
// `Result<X, napi::Error>` 会被解析成 `Error<napi::Error>`，报出
// 「`napi::Error: AsRef<str>` is not satisfied」这种看不出因果的错误。
use napi::bindgen_prelude::{BigInt, FromNapiValue};
use napi::{Env, JsObject, JsUnknown, ValueType};
use nervusdb_core::{NervusDb as CoreNervusDb, Transaction as CoreTransaction, Value};
use std::collections::{HashMap, HashSet};

/// `i64` → JS BigInt（输出侧）。
fn bigint(v: i64) -> BigInt {
    BigInt::from(v)
}

/// BigInt 参数 → `u64`，超出范围时**报出真实数值**。
///
/// 负数和超过 `u64::MAX` 的值都会被拒绝，而不是取模或截断——ID 是个身份，
/// 静默换一个身份比报错危险得多。
fn to_u64(v: &BigInt) -> Result<u64, napi::Error> {
    let (sign, val, lossless) = v.get_u128();
    if sign || !lossless || val > u64::MAX as u128 {
        let shown = if sign {
            format!("-{val}")
        } else {
            val.to_string()
        };
        return Err(napi::Error::from_reason(format!(
            "id {shown} is not a valid node or edge id (must be 0..=u64::MAX)"
        )));
    }
    Ok(val as u64)
}

/// 图中整数属性的**精确**往返。
///
/// ## 为什么整数在 JS 侧必须是 BigInt
///
/// 属性值是 `i64`，而 JavaScript 的 `number` 是 f64：超过 2^53 的整数经过它就
/// **静默舍入**。实测（本改动之前）：
///
/// | 写入            | 读回                    |
/// | --------------- | ----------------------- |
/// | `2^53 + 1`      | `9007199254740992`      |
/// | `i64::MAX`      | `9223372036854776000`   |
/// | `i64::MIN`      | `-9223372036854776000`  |
///
/// Python SDK 没有这个问题（`int` 是任意精度），所以同一个库、同一个值，两个 SDK
/// 给出不同答案——而 Node 那侧不报错。
///
/// 这不是「JS 的天然限制」：内核**专门**为精确性放弃过便利（`sum()` 用
/// `checked_add`，因为它曾用 f64 累加而静默丢精度，见 CHANGELOG）。binding 把
/// 内核刚保住的精确性再丢掉，是 self-defeating。
///
/// ## 代价（明确记录）
///
/// 全部整数变 `bigint` 后，用户代码里这些写法会抛 `TypeError`：
/// `count + 1`（不能混算）、`JSON.stringify(结果)`（BigInt 不可序列化）、
/// `Math.max(...)`。读取、比较、模板字符串不受影响。这是一次**破坏性变更**，
/// 在 CHANGELOG 与 SDK 文档里都标明了。
fn i64_to_js(env: &Env, v: i64) -> Result<JsUnknown, napi::Error> {
    env.create_bigint_from_i64(v)?.into_unknown()
}

/// 把任意 JS 标量转成图属性值。
///
/// BigInt 是主路径（整数一律以 BigInt 出入）。`number` 仍然接受，且**区分整数与
/// 浮点**：`2.0` 是浮点，`2n` 是整数——若把 `2.0` 也当整数，用户就没法写浮点属性。
fn js_to_value(_env: &Env, val: JsUnknown) -> Result<Value, napi::Error> {
    match val.get_type()? {
        ValueType::Boolean => {
            let b = val.coerce_to_bool()?.get_value()?;
            Ok(Value::from(b))
        }
        ValueType::Number => {
            let n: f64 = val.coerce_to_number()?.get_double()?;
            if n.fract() == 0.0 && n.is_finite() && n.abs() <= 9_007_199_254_740_992.0 {
                // 安全的整数值：转成 Int，与 `2n` 落成同一类型，避免「同一个 2，
                // 一个来自 `2` 另一个来自 `2n`」在库里变成两种值。
                Ok(Value::from(n as i64))
            } else {
                Ok(Value::from(n))
            }
        }
        ValueType::String => {
            let s = val.coerce_to_string()?.into_utf8()?.as_str()?.to_string();
            Ok(Value::from(s))
        }
        ValueType::BigInt => {
            // 用 bindgen 侧的 `BigInt`（有 `FromNapiValue`），而不是旧式 `JsBigInt`
            // （它既没有 `coerce_to_bigint` 也没有 `TryFrom<JsUnknown>`）。
            // `from_unknown` 顺带算出 word_count 并处理符号位。
            let big = BigInt::from_unknown(val)
                .map_err(|_| napi::Error::from_reason("expected a BigInt".to_string()))?;
            // 先取 i128，以便**超出 i64 时把真实数值报给用户**：只说「超出范围」
            // 而不说是哪个数，调用方无从判断是自己写错还是精度丢失。
            let (wide, _) = big.get_i128();
            let (v, lossless) = big.get_i64();
            if !lossless {
                return Err(napi::Error::from_reason(format!(
                    "integer {wide} is outside the range a graph property can hold (i64)"
                )));
            }
            Ok(Value::from(v))
        }
        ValueType::Null | ValueType::Undefined => Ok(Value::Null),
        ValueType::Object => Err(napi::Error::from_reason(
            "objects and arrays cannot be stored as a graph property; \
             supported types are string, bigint, number, boolean and null"
                .to_string(),
        )),
        other => Err(napi::Error::from_reason(format!(
            "cannot store a {other} as a graph property; supported types are \
             string, bigint, number, boolean and null"
        ))),
    }
}

/// 属性字典（`{k: v}`）→ 图属性表。
fn js_to_properties(
    env: &Env,
    val_opt: Option<JsUnknown>,
) -> Result<HashMap<String, Value>, napi::Error> {
    let mut map = HashMap::new();
    let Some(val) = val_opt else {
        return Ok(map);
    };
    if val.get_type()? != ValueType::Object {
        return Ok(map);
    }
    // `Object::get` 需要 `napi_get_named_property` 的键；先取键列表再逐项读。
    // 用 `JsObject::keys`（而非 serde 反序列化）是为了让值**保持 BigInt**——
    // 走 `serde_json::Value` 会把 BigInt 变成 f64，正是本改动要消除的问题。
    let obj = JsObject::try_from(val)
        .map_err(|_| napi::Error::from_reason("expected an object".to_string()))?;
    for key in JsObject::keys(&obj)? {
        let child: Option<JsUnknown> = JsObject::get(&obj, &key)?;
        let Some(child) = child else { continue };
        map.insert(key, js_to_value(env, child)?);
    }
    Ok(map)
}

/// 图属性值 → JS 标量。整数一律 BigInt。
fn value_to_js(env: &Env, v: &Value) -> Result<JsUnknown, napi::Error> {
    match v {
        Value::Null => Ok(env.get_undefined()?.into_unknown()),
        Value::Int(i) => i64_to_js(env, *i),
        Value::Float(f) => Ok(env.create_double(*f)?.into_unknown()),
        Value::Bool(b) => Ok(env.get_boolean(*b)?.into_unknown()),
        Value::String(s) => Ok(env.create_string(s)?.into_unknown()),
        Value::List(items) => {
            let mut arr = env.create_array_with_length(items.len())?;
            for (i, item) in items.iter().enumerate() {
                arr.set_element(i as u32, value_to_js(env, item)?)?;
            }
            Ok(arr.into_unknown())
        }
    }
}

/// 一行查询结果 → JS 对象（列名 → 值）。
fn row_to_js(env: &Env, columns: &[String], values: &[Value]) -> Result<JsUnknown, napi::Error> {
    let mut obj = env.create_object()?;
    for (idx, name) in columns.iter().enumerate() {
        match values.get(idx) {
            Some(v) => obj.set_named_property(name, value_to_js(env, v)?)?,
            None => obj.set_named_property(name, env.get_undefined()?.into_unknown())?,
        }
    }
    Ok(obj.into_unknown())
}

#[napi(object)]
pub struct DijkstraResult {
    pub cost: f64,
    pub path: Vec<BigInt>,
}

#[napi(object)]
pub struct SubgraphEdge {
    pub id: BigInt,
    pub src_id: BigInt,
    pub dst_id: BigInt,
    pub edge_type: String,
    pub weight: f64,
}

#[napi(object)]
pub struct KHopSubgraph {
    pub nodes: Vec<BigInt>,
    pub edges: Vec<SubgraphEdge>,
}

fn parse_direction(direction: Option<&str>) -> Result<nervusdb_core::Direction, napi::Error> {
    match direction.map(|d| d.to_ascii_lowercase()) {
        None => Ok(nervusdb_core::Direction::Both),
        Some(d) => match d.as_str() {
            "out" | "outgoing" => Ok(nervusdb_core::Direction::Outgoing),
            "in" | "incoming" => Ok(nervusdb_core::Direction::Incoming),
            "both" | "any" => Ok(nervusdb_core::Direction::Both),
            other => Err(napi::Error::from_reason(format!(
                "invalid direction '{}': expected outgoing/incoming/both",
                other
            ))),
        },
    }
}

/// 解析一条批量节点输入：`{ labels: string[], properties?: {...} }`。
///
/// 逐字段读取而非整体反序列化，是为了让属性值**保持 BigInt**：走
/// `serde_json::Value` 会把 BigInt 变成 f64，正是这次改动要消除的问题。
fn parse_node_item(
    env: &Env,
    item: JsUnknown,
) -> Result<(HashSet<String>, HashMap<String, Value>), napi::Error> {
    let obj = JsObject::try_from(item)
        .map_err(|_| napi::Error::from_reason("each node must be an object".to_string()))?;

    let labels_unknown = JsObject::get::<_, JsUnknown>(&obj, "labels")?
        .ok_or_else(|| napi::Error::from_reason("node is missing `labels`".to_string()))?;
    let labels_arr = JsObject::try_from(labels_unknown).map_err(|_| {
        napi::Error::from_reason("`labels` must be an array of strings".to_string())
    })?;
    let len = labels_arr.get_array_length()? as usize;
    let mut labels = HashSet::with_capacity(len);
    for i in 0..len {
        let v: JsUnknown = labels_arr.get_element(i as u32)?;
        labels.insert(v.coerce_to_string()?.into_utf8()?.as_str()?.to_string());
    }

    let props: Option<JsUnknown> = JsObject::get(&obj, "properties")?;
    let properties = js_to_properties(env, props)?;
    Ok((labels, properties))
}

/// 解析一条批量边输入：`{ src, dst, edgeType, properties?, weight? }`。
fn parse_edge_item(
    env: &Env,
    item: JsUnknown,
) -> Result<nervusdb_core::disk_graph::EdgeInsert, napi::Error> {
    let obj = JsObject::try_from(item)
        .map_err(|_| napi::Error::from_reason("each edge must be an object".to_string()))?;

    let read_u64 = |name: &str| -> Result<u64, napi::Error> {
        let v = JsObject::get::<_, JsUnknown>(&obj, name)?
            .ok_or_else(|| napi::Error::from_reason(format!("edge is missing `{name}`")))?;
        let big = BigInt::from_unknown(v)
            .map_err(|_| napi::Error::from_reason(format!("`{name}` must be a BigInt")))?;
        to_u64(&big)
    };
    let src_id = read_u64("src")?;
    let dst_id = read_u64("dst")?;

    let edge_type = JsObject::get::<_, JsUnknown>(&obj, "edgeType")?
        .ok_or_else(|| napi::Error::from_reason("edge is missing `edgeType`".to_string()))?
        .coerce_to_string()?
        .into_utf8()?
        .as_str()?
        .to_string();

    let props: Option<JsUnknown> = JsObject::get(&obj, "properties")?;
    let properties = js_to_properties(env, props)?;

    let weight: Option<f64> = match JsObject::get::<_, JsUnknown>(&obj, "weight")? {
        Some(v) => Some(v.coerce_to_number()?.get_double()?),
        None => None,
    };

    Ok(nervusdb_core::disk_graph::EdgeInsert {
        edge_id: 0, // 引擎在批量预留时填充；见 `Transaction::add_edges` 的说明
        src_id,
        dst_id,
        edge_type,
        properties,
        weight: weight.unwrap_or(1.0),
    })
}

#[napi(js_name = "NervusDb")]
pub struct JsNervusDb {
    inner: CoreNervusDb,
}
#[napi]
impl JsNervusDb {
    #[napi(factory)]
    pub fn open(path: String, pool_size: Option<u32>) -> Result<Self, napi::Error> {
        let frames = pool_size.unwrap_or(1024) as usize;
        let db = CoreNervusDb::open_with_pool_size(path, frames)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(Self { inner: db })
    }

    #[napi]
    pub fn execute(&self, env: Env, cypher: String) -> Result<JsUnknown, napi::Error> {
        let res = self
            .inner
            .execute(&cypher)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let mut obj = env.create_object()?;
        // 计数字段也是整数，走同一套 BigInt 规则——否则 `nodes_created` 会是
        // number 而属性里的整数是 bigint，同一个 SDK 里两种整数类型。
        obj.set_named_property("nodes_created", i64_to_js(&env, res.nodes_created as i64)?)?;
        obj.set_named_property("edges_created", i64_to_js(&env, res.edges_created as i64)?)?;
        obj.set_named_property("nodes_deleted", i64_to_js(&env, res.nodes_deleted as i64)?)?;
        obj.set_named_property("edges_deleted", i64_to_js(&env, res.edges_deleted as i64)?)?;
        obj.set_named_property(
            "properties_set",
            i64_to_js(&env, res.properties_set as i64)?,
        )?;
        obj.set_named_property("message", env.create_string(&res.message)?)?;
        Ok(obj.into_unknown())
    }

    /// 查询并返回行数组；每行是一个 `{列名: 值}` 对象。
    ///
    /// 整数列（含 `count()` 一类聚合）返回 **BigInt**，浮点返回 `number`；理由见
    /// `i64_to_js` 的说明（该函数是私有的，故此处用文字而非文档链接）。
    #[napi]
    pub fn query(&self, env: Env, cypher: String) -> Result<Vec<JsUnknown>, napi::Error> {
        let res = self
            .inner
            .query_cypher(&cypher)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let mut rows = Vec::with_capacity(res.rows.len());
        for row in res.rows {
            rows.push(row_to_js(&env, &res.columns, &row.values)?);
        }
        Ok(rows)
    }

    #[napi]
    pub fn add_node(
        &self,
        env: Env,
        labels: Vec<String>,
        properties: Option<JsUnknown>,
    ) -> Result<BigInt, napi::Error> {
        let labels_set: HashSet<String> = labels.into_iter().collect();
        let props = js_to_properties(&env, properties)?;
        let id = self
            .inner
            .add_node(labels_set, props)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(bigint(id as i64))
    }

    #[napi]
    pub fn add_edge(
        &self,
        env: Env,
        src: BigInt,
        dst: BigInt,
        edge_type: String,
        properties: Option<JsUnknown>,
        weight: Option<f64>,
    ) -> Result<BigInt, napi::Error> {
        let props = js_to_properties(&env, properties)?;
        let w = weight.unwrap_or(1.0);
        let id = self
            .inner
            .add_edge(to_u64(&src)?, to_u64(&dst)?, &edge_type, props, w)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(bigint(id as i64))
    }

    #[napi]
    pub fn dijkstra(
        &self,
        start: BigInt,
        end: BigInt,
        edge_type: Option<String>,
    ) -> Result<Option<DijkstraResult>, napi::Error> {
        let res = self
            .inner
            .dijkstra(to_u64(&start)?, to_u64(&end)?, edge_type.as_deref());
        Ok(res.map(|(cost, path)| DijkstraResult {
            cost,
            path: path.into_iter().map(|id| bigint(id as i64)).collect(),
        }))
    }

    #[napi]
    pub fn stats(&self, env: Env) -> Result<JsUnknown, napi::Error> {
        let s = self.inner.buffer_stats();
        let mut obj = env.create_object()?;
        // 全部计数都是整数 → BigInt，与属性值同一规则。
        obj.set_named_property(
            "capacity_frames",
            i64_to_js(&env, s.capacity_frames as i64)?,
        )?;
        obj.set_named_property("used_frames", i64_to_js(&env, s.used_frames as i64)?)?;
        obj.set_named_property("dirty_frames", i64_to_js(&env, s.dirty_frames as i64)?)?;
        obj.set_named_property("cache_hits", i64_to_js(&env, s.cache_hits as i64)?)?;
        obj.set_named_property("cache_misses", i64_to_js(&env, s.cache_misses as i64)?)?;
        obj.set_named_property("disk_reads", i64_to_js(&env, s.disk_reads as i64)?)?;
        obj.set_named_property("disk_writes", i64_to_js(&env, s.disk_writes as i64)?)?;
        obj.set_named_property(
            "file_size_bytes",
            i64_to_js(&env, s.file_size_bytes as i64)?,
        )?;
        obj.set_named_property("wal_page_count", i64_to_js(&env, s.wal_page_count as i64)?)?;
        obj.set_named_property("spill_count", i64_to_js(&env, s.spill_count as i64)?)?;
        obj.set_named_property("wal_size_bytes", i64_to_js(&env, s.wal_size_bytes as i64)?)?;
        obj.set_named_property(
            "wal_fsync_count",
            i64_to_js(&env, s.wal_fsync_count as i64)?,
        )?;
        obj.set_named_property(
            "wal_frames_written",
            i64_to_js(&env, s.wal_frames_written as i64)?,
        )?;
        // 命中率是**浮点**，保持 number：它是比例，不是计数。
        obj.set_named_property(
            "hit_rate_percentage",
            env.create_double(s.hit_rate_percentage)?,
        )?;
        Ok(obj.into_unknown())
    }

    /// 无权 BFS 最短路径
    #[napi]
    pub fn bfs(
        &self,
        start: BigInt,
        end: BigInt,
        edge_type: Option<String>,
    ) -> Result<Option<Vec<BigInt>>, napi::Error> {
        Ok(self
            .inner
            .bfs(to_u64(&start)?, to_u64(&end)?, edge_type.as_deref())
            .map(|path| path.into_iter().map(|id| bigint(id as i64)).collect()))
    }

    #[napi]
    pub fn has_cycle(&self) -> Result<bool, napi::Error> {
        Ok(self.inner.has_cycle())
    }

    /// PageRank 阻尼迭代：[{ node_id, score }, ...]，按分数降序
    #[napi(js_name = "pageRank")]
    pub fn pagerank(
        &self,
        env: Env,
        damping_factor: Option<f64>,
        max_iterations: Option<u32>,
        tolerance: Option<f64>,
    ) -> Result<Vec<JsUnknown>, napi::Error> {
        let scores = self.inner.pagerank_with(
            damping_factor.unwrap_or(0.85),
            max_iterations.unwrap_or(100) as usize,
            tolerance.unwrap_or(1e-6),
        );
        let mut out = Vec::with_capacity(scores.len());
        for s in scores {
            let mut obj = env.create_object()?;
            obj.set_named_property("node_id", i64_to_js(&env, s.node_id as i64)?)?;
            obj.set_named_property("score", env.create_double(s.score)?)?;
            out.push(obj.into_unknown());
        }
        Ok(out)
    }

    /// 弱连通分量：[[node_id, ...], ...]，按分量规模降序
    #[napi]
    pub fn weakly_connected_components(&self) -> Result<Vec<Vec<BigInt>>, napi::Error> {
        Ok(self
            .inner
            .weakly_connected_components()
            .into_iter()
            .map(|c| c.into_iter().map(|id| bigint(id as i64)).collect())
            .collect())
    }

    /// K-Hop 局部子图提取
    #[napi]
    pub fn k_hop_subgraph(
        &self,
        start: BigInt,
        k: u32,
        direction: Option<String>,
        edge_type: Option<String>,
    ) -> Result<KHopSubgraph, napi::Error> {
        let dir = parse_direction(direction.as_deref())?;
        let sub = self
            .inner
            .k_hop_subgraph_with(to_u64(&start)?, k as usize, dir, edge_type.as_deref())
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        Ok(KHopSubgraph {
            nodes: sub.nodes.into_iter().map(|id| bigint(id as i64)).collect(),
            edges: sub
                .edges
                .into_iter()
                .map(|e| SubgraphEdge {
                    id: bigint(e.id as i64),
                    src_id: bigint(e.src_id as i64),
                    dst_id: bigint(e.dst_id as i64),
                    edge_type: e.edge_type,
                    weight: e.weight,
                })
                .collect(),
        })
    }

    /// 图模式：全部标签
    #[napi]
    pub fn labels(&self) -> Result<Vec<String>, napi::Error> {
        Ok(self.inner.labels())
    }

    #[napi]
    pub fn edge_types(&self) -> Result<Vec<String>, napi::Error> {
        Ok(self.inner.edge_types())
    }

    /// 导出当前图数据为可回灌的 Cypher 脚本
    #[napi]
    pub fn dump_cypher(&self) -> Result<String, napi::Error> {
        let mut buffer: Vec<u8> = Vec::new();
        self.inner
            .dump_cypher(&mut buffer)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        String::from_utf8(buffer)
            .map_err(|e| napi::Error::from_reason(format!("dump is not valid UTF-8: {}", e)))
    }

    #[napi]
    pub fn checkpoint(&self) -> Result<(), napi::Error> {
        self.inner
            .checkpoint()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(())
    }

    /// 开启显式批量事务：脏页在 commit() 时批量写入 WAL 并**只做一次 fsync**
    #[napi(js_name = "beginTransaction")]
    pub fn begin_transaction(&self) -> Result<Transaction, napi::Error> {
        let inner = self
            .inner
            .begin_transaction()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(Transaction { inner: Some(inner) })
    }
}

/// 显式批量事务句柄
#[napi]
pub struct Transaction {
    inner: Option<CoreTransaction>,
}

#[napi]
impl Transaction {
    #[napi]
    pub fn tx_id(&self) -> Result<BigInt, napi::Error> {
        self.inner
            .as_ref()
            .map(|tx| bigint(tx.tx_id() as i64))
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))
    }

    #[napi]
    pub fn add_node(
        &mut self,
        env: Env,
        labels: Vec<String>,
        properties: Option<JsUnknown>,
    ) -> Result<BigInt, napi::Error> {
        let labels_set: HashSet<String> = labels.into_iter().collect();
        let props = js_to_properties(&env, properties)?;
        let tx = self
            .inner
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))?;
        let id = tx
            .add_node(labels_set, props)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(bigint(id as i64))
    }

    /// 事务内**批量**添加节点，返回按输入顺序排列的 id。
    ///
    /// `nodes` 是 `{ labels, properties }` 的数组：
    ///
    /// ```js
    /// const tx = db.beginTransaction();
    /// const ids = tx.addNodes([
    ///   { labels: ["Person"], properties: { name: "A" } },
    ///   { labels: ["Person"], properties: { name: "B" } },
    /// ]);
    /// tx.commit();
    /// ```
    ///
    /// ## 为什么它不只是循环调用 `addNode`
    ///
    /// 逐条 `addNode` 每条取一次全局写锁并各自预留 id；批量版本**一次预留全部
    /// id**。文档里这是主要的性能手段：每个事务 20,000 节点时 451,576 ops/s，
    /// 而每节点一个事务只有 251 ops/s（见 `docs/benchmarks.md`）。
    ///
    /// **Python SDK 一直有 `add_nodes`/`add_edges`，Node 没有**，而
    /// `docs/benchmarks.md` 写着「`add_nodes` / `add_edges` exist in both SDKs」。
    /// 那句话当时是假的，现在补上。
    ///
    /// 语义与逐条 `addNode` 完全一致（同一事务、同样索引维护、同样约束校验），
    /// 返回的 id 与输入**顺序一一对应**。
    #[napi(js_name = "addNodes")]
    pub fn add_nodes(
        &mut self,
        env: Env,
        nodes: Vec<JsUnknown>,
    ) -> Result<Vec<BigInt>, napi::Error> {
        let mut parsed = Vec::with_capacity(nodes.len());
        for item in nodes {
            parsed.push(parse_node_item(&env, item)?);
        }
        let tx = self
            .inner
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))?;
        let ids = tx
            .add_nodes(parsed)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(ids.into_iter().map(|id| bigint(id as i64)).collect())
    }

    /// 事务内**批量**添加边，返回按输入顺序排列的 id。
    ///
    /// `edges` 是 `{ src, dst, edgeType, properties?, weight? }` 的数组。
    /// id 由引擎分配（与 `addEdge` 同一规则：调用方给的 id 不生效）；这里的
    /// `src`/`dst` 接受 BigInt。
    #[napi(js_name = "addEdges")]
    pub fn add_edges(
        &mut self,
        env: Env,
        edges: Vec<JsUnknown>,
    ) -> Result<Vec<BigInt>, napi::Error> {
        let mut inserts = Vec::with_capacity(edges.len());
        for item in edges {
            inserts.push(parse_edge_item(&env, item)?);
        }
        let tx = self
            .inner
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))?;
        let ids = tx
            .add_edges(inserts)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(ids.into_iter().map(|id| bigint(id as i64)).collect())
    }

    #[napi]
    pub fn add_edge(
        &mut self,
        env: Env,
        src: BigInt,
        dst: BigInt,
        edge_type: String,
        properties: Option<JsUnknown>,
        weight: Option<f64>,
    ) -> Result<BigInt, napi::Error> {
        let props = js_to_properties(&env, properties)?;
        let w = weight.unwrap_or(1.0);
        let tx = self
            .inner
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))?;
        let id = tx
            .add_edge(to_u64(&src)?, to_u64(&dst)?, &edge_type, props, w)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(bigint(id as i64))
    }

    #[napi]
    pub fn update_node_property(
        &mut self,
        env: Env,
        node_id: BigInt,
        key: String,
        value: JsUnknown,
    ) -> Result<(), napi::Error> {
        let val = js_to_value(&env, value)?;
        self.inner
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))?
            .update_node_property(to_u64(&node_id)?, key, val)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(())
    }

    #[napi]
    pub fn update_edge_property(
        &mut self,
        env: Env,
        edge_id: BigInt,
        key: String,
        value: JsUnknown,
    ) -> Result<(), napi::Error> {
        let val = js_to_value(&env, value)?;
        self.inner
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))?
            .update_edge_property(to_u64(&edge_id)?, key, val)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(())
    }

    #[napi]
    pub fn remove_node(&mut self, node_id: BigInt) -> Result<(), napi::Error> {
        self.inner
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))?
            .remove_node(to_u64(&node_id)?)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(())
    }

    #[napi]
    pub fn remove_edge(&mut self, edge_id: BigInt) -> Result<(), napi::Error> {
        self.inner
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))?
            .remove_edge(to_u64(&edge_id)?)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(())
    }

    /// 提交事务：所有变更原子应用，仅触发一次 WAL fsync
    #[napi]
    pub fn commit(&mut self) -> Result<(), napi::Error> {
        let tx = self.inner.take().ok_or_else(|| {
            napi::Error::from_reason("transaction already committed or rolled back")
        })?;
        tx.commit()
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    /// 回滚事务：丢弃全部未提交变更，主库零污染
    #[napi]
    pub fn rollback(&mut self) -> Result<(), napi::Error> {
        let tx = self.inner.take().ok_or_else(|| {
            napi::Error::from_reason("transaction already committed or rolled back")
        })?;
        tx.rollback()
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }
}

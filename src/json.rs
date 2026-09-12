//! 最小 JSON 编码器：只做输出，不做解析。
//!
//! ## 为什么手写
//!
//! 本模块服务的两个出口都是**输出**：C API 把查询结果交还给调用方，
//! Cypher 的 `RETURN` 把属性字典渲染为字符串。代码里**没有任何一处解析 JSON**
//! （已核实：零处 `from_str` / `from_value` / `from_slice`），因此引入一个完整的
//! JSON 解析器只为把结构体写出去，是把大量用不到的代码纳入依赖树。
//!
//! 这与 `codec.rs` 的理由一致：能自己写清楚的东西不外包，尤其是输出格式这种
//! 一旦改变就会影响下游调用方的边界。
//!
//! ## 覆盖范围
//!
//! 只实现本项目会输出的类型：`null`、布尔、整数、浮点、字符串、数组、字符串键的
//! 对象。不含解析器、不含泛型 `Serialize` 抽象、不做美化输出。
//!
//! ## 转义规则（RFC 8259）
//!
//! `"` `\` `\b` `\f` `\n` `\r` `\t` 用短转义；其余 U+0000..U+001F 用 `\u00XX`。
//! **不转义** DEL（U+007F）与非 ASCII——JSON 允许它们原样以 UTF-8 出现，
//! 转义只会让中文属性变得不可读。

use crate::graph::Value;
use std::collections::HashMap;

/// 把字符串按 JSON 规则转义并写入（含两侧引号）。
pub fn write_escaped(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // 其余控制字符必须转义，否则会产生非法 JSON
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// 把 `Value` 编码为 JSON。
///
/// `Null` 与 `List` 只存在于求值期（见 `graph.rs` 的说明），但查询结果里会出现，
/// 因此这里必须给出合法的 JSON：`null` 与数组。
pub fn write_value(out: &mut String, v: &Value) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::Float(f) => {
            // JSON 没有 NaN / Infinity。写成 null 而不是输出非法字面量，
            // 否则下游 JSON 解析器会直接拒绝整个文档。
            if f.is_finite() {
                out.push_str(&f.to_string())
            } else {
                out.push_str("null")
            }
        }
        Value::String(s) => write_escaped(out, s),
        Value::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(out, item);
            }
            out.push(']');
        }
    }
}

/// 把 `Binding` 渲染出的字符串解析回 JSON 无关：这里只负责转义为 JSON 字符串。
///
/// 单独提供是因为 Cypher 的 `RETURN n` 把节点渲染成**已经构造好的 JSON 文本**
/// （见 `render_binding`），此处按原文嵌入，不再二次转义。
pub fn write_raw_json_string(out: &mut String, raw: &str) {
    out.push_str(raw);
}

/// 编码一个 Cypher 结果集：`{"columns":[...],"rows":[[...],...]}`
///
/// 行以数组而非对象表示：重复列名（`RETURN a.x AS n, b.y AS n`）在对象里会
/// 互相覆盖，而数组保序且允许重名，这正是结果集的本意。
pub fn result_set_to_string(columns: &[String], rows: &[Vec<String>]) -> String {
    let mut out = String::with_capacity(64 + rows.len() * columns.len() * 16);
    out.push_str("{\"columns\":[");
    for (i, c) in columns.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_escaped(&mut out, c);
    }
    out.push_str("],\"rows\":[");
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('[');
        for (j, v) in row.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            // 单元格在 `Row::values` 里已经是渲染好的文本（见 executor），
            // 因此按 JSON 字符串转义而不是当字面量拼接。
            write_escaped(&mut out, v);
        }
        out.push(']');
    }
    out.push_str("]}");
    out
}

/// 把字符串键的映射编码为 JSON 对象。
pub fn write_map(out: &mut String, m: &HashMap<String, Value>) {
    out.push('{');
    // 键排序：HashMap 迭代序随机，不排序会让同一份数据每次输出不同字节，
    // 使结果无法比对、也让测试无从断言。
    let mut keys: Vec<&String> = m.keys().collect();
    keys.sort_unstable();
    for (i, k) in keys.into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_escaped(out, k);
        out.push(':');
        write_value(out, &m[k]);
    }
    out.push('}');
}

/// 便捷入口：属性字典 → JSON 字符串。
pub fn map_to_string(m: &HashMap<String, Value>) -> String {
    let mut out = String::with_capacity(m.len() * 32 + 2);
    write_map(&mut out, m);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(v: &Value) -> String {
        let mut s = String::new();
        write_value(&mut s, v);
        s
    }

    #[test]
    fn scalars() {
        assert_eq!(enc(&Value::Bool(true)), "true");
        assert_eq!(enc(&Value::Bool(false)), "false");
        assert_eq!(enc(&Value::Int(-7)), "-7");
        assert_eq!(enc(&Value::Float(1.5)), "1.5");
    }

    #[test]
    fn strings_are_escaped() {
        assert_eq!(enc(&Value::String("abc".into())), "\"abc\"");
        assert_eq!(enc(&Value::String("a\"b".into())), "\"a\\\"b\"");
        assert_eq!(enc(&Value::String("a\\b".into())), "\"a\\\\b\"");
        assert_eq!(enc(&Value::String("a\nb".into())), "\"a\\nb\"");
        assert_eq!(enc(&Value::String("a\tb".into())), "\"a\\tb\"");
    }

    /// 控制字符必须转义成 \u00XX，否则输出不是合法 JSON。
    #[test]
    fn control_characters_are_escaped() {
        assert_eq!(enc(&Value::String("\u{01}".into())), "\"\\u0001\"");
        assert_eq!(enc(&Value::String("\u{1f}".into())), "\"\\u001f\"");
    }

    /// 中文与其它非 ASCII 必须原样保留（JSON 允许 UTF-8 直出）。
    #[test]
    fn non_ascii_passes_through_unescaped() {
        assert_eq!(enc(&Value::String("青云门".into())), "\"青云门\"");
    }

    /// NaN / Infinity 不是合法 JSON，必须退化为 null 而不是输出非法字面量。
    #[test]
    fn non_finite_floats_become_null() {
        assert_eq!(enc(&Value::Float(f64::NAN)), "null");
        assert_eq!(enc(&Value::Float(f64::INFINITY)), "null");
        assert_eq!(enc(&Value::Float(f64::NEG_INFINITY)), "null");
    }

    /// 键排序保证同一映射每次输出相同字节。
    #[test]
    fn map_encoding_is_deterministic() {
        let mut m = HashMap::new();
        for i in 0..50 {
            m.insert(format!("k{}", i), Value::Int(i));
        }
        let a = map_to_string(&m);
        let b = map_to_string(&m);
        assert_eq!(a, b, "same map must encode identically");
        assert!(a.starts_with("{\"k0\":"), "keys must be sorted, got: {}", a);
    }

    #[test]
    fn empty_map() {
        assert_eq!(map_to_string(&HashMap::new()), "{}");
    }

    #[test]
    fn result_set_shape() {
        let cols = vec!["a".to_string(), "b.c".to_string()];
        let rows = vec![
            vec!["1".to_string(), "\"x\"".to_string()],
            vec!["2".to_string(), "null".to_string()],
        ];
        assert_eq!(
            result_set_to_string(&cols, &rows),
            "{\"columns\":[\"a\",\"b.c\"],\"rows\":[[\"1\",\"\\\"x\\\"\"],[\"2\",\"null\"]]}"
        );
    }

    #[test]
    fn result_set_empty() {
        assert_eq!(
            result_set_to_string(&[], &[]),
            "{\"columns\":[],\"rows\":[]}"
        );
    }
}

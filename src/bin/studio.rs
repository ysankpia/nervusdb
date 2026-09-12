//! GraphLite Studio —— 在浏览器里查看图数据的本地工作台。
//!
//! ## 为什么是独立二进制
//!
//! 它需要开网络端口、要嵌入一个 HTML 页面。这些都不属于一个嵌入式数据库内核的
//! 职责：核心库必须保持零依赖、单文件、不碰网络。Studio 只是核心库的一个消费者，
//! `cargo install` 后作为 `graphlite-studio` 独立存在。
//!
//! ## 为什么手写 HTTP
//!
//! 只需要 3 个 GET 路由。为此引入一个 HTTP 框架，会让「零依赖」这条已经用测试
//! 守住的不变量在一个二进制里破功——而依赖一旦进来就会扩散。手写 200 行的代价
//! 换来的是：这个工具的行为完全可由本文件解释，不依赖任何外部版本。
//!
//! ## 安全边界
//!
//! **只绑 `127.0.0.1`。** 本服务没有任何认证，绑到对外接口等于把数据库公开。
//! 端口由内核分配（`:0`），避免与用户已有服务冲突。
//!
//! ## 只读打开
//!
//! 用 [`GraphLite::open_read_only`]：Studio 从不写数据，因此它可以与正在写入的
//! Agent 进程**同时运行**。这正是共享读锁要解决的场景。

use graphlite::GraphLite;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 内嵌的可视化页面。`include_str!` 而非读文件：二进制自包含，
/// 不会因为找不到静态资源而在运行时 404，也不受工作目录影响。
const PAGE: &str = include_str!("studio_page.html");

/// 单次导出的节点上限。超出会被**明确拒绝**而不是静默截断——
/// 把几十万节点塞进浏览器只会让它卡死，用户需要知道这件事。
const MAX_EXPORT: usize = 5_000;

/// 默认导出量。几百个节点是可视化仍能看清关系的规模。
const DEFAULT_LIMIT: usize = 300;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path: PathBuf = match args.next() {
        Some(p) if p == "-h" || p == "--help" => {
            print_usage();
            return Ok(());
        }
        Some(p) => PathBuf::from(p),
        None => {
            print_usage();
            std::process::exit(2);
        }
    };

    // 允许命令行覆盖默认导出量：大库上可能想一次看更多
    let limit: usize = args
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_LIMIT);

    if !path.exists() {
        eprintln!("Database not found: {}", path.display());
        std::process::exit(1);
    }

    // 先打开一次以给出即时反馈（库损坏、版本不符、WAL 待回放等问题应当在启动时
    // 就告诉用户，而不是等他打开浏览器才发现），随后**立即释放**。
    //
    // 不长期持有句柄的理由见 `open_for_request`。
    {
        let probe = match GraphLite::open_read_only(&path) {
            Ok(db) => db,
            Err(e) => {
                eprintln!("Cannot open '{}':", path.display());
                eprintln!("  {}", e);
                eprintln!();
                eprintln!("Studio opens the database read-only. If the message mentions");
                eprintln!("an unreplayed WAL, open the database once with a read-write");
                eprintln!("handle (the CLI, or any SDK) and retry.");
                std::process::exit(1);
            }
        };
        println!("GraphLite Studio");
        println!(
            "  database : {} ({} nodes, {} edges)",
            path.display(),
            probe.node_count(),
            probe.edge_count()
        );
    }

    // 绑回环地址；端口交给内核分配，避免与用户已有服务冲突
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let url = format!("http://127.0.0.1:{}", port);

    println!("  listening: {}", url);
    println!("  mode     : read-only; the lock is taken per request, not held");
    println!("  Ctrl-C to stop");

    open_browser(&url);

    let shared_path = Arc::new(path);
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            // 单个连接失败不应终止服务：浏览器会频繁建立/丢弃连接
            Err(_) => continue,
        };
        let path = Arc::clone(&shared_path);
        // 每个连接一个线程：页面与数据请求会被浏览器并发发起
        std::thread::spawn(move || {
            let _ = handle_connection(&path, stream, limit);
        });
    }

    Ok(())
}

/// 为一个请求打开数据库。
///
/// ## 为什么按请求开关，而不是启动时开一次
///
/// 共享读锁与写锁是**互斥**的（这正是排他性要保证的）。若 Studio 在启动时打开并
/// 长期持有读锁，那么用户开着界面期间 Agent 就完全无法写入——而「Agent 在后台写、
/// 我在前台看」正是本工具存在的理由。这个矛盾是在端到端验证时才暴露出来的：
/// Studio 运行期间，另一个进程的写入被 `DatabaseLocked` 拒绝。
///
/// 改为按请求开关后，Studio 只在处理单个 HTTP 请求的几毫秒内持锁，写者可以在两次
/// 请求之间自由写入。代价是每个请求多一次 `open`（约 1ms，含 WAL 待回放检查）。
///
/// ## 这仍不是真正的并发读写
///
/// 读者与写者依旧不能**同时**持锁：若写者恰好在 Studio 处理请求时写入，该请求会
/// 拿到 `DatabaseLocked`，此时返回 503 并提示重试。真正的并发需要快照隔离
/// （读者读一份稳定快照、写者只追加 WAL），那是另一个量级的改动，已记入
/// `ROADMAP.md`，不在这里假装实现。
fn open_for_request(path: &Path) -> Result<GraphLite, String> {
    GraphLite::open_read_only(path).map_err(|e| e.to_string())
}

fn print_usage() {
    println!("GraphLite Studio — browse a graph database in your browser");
    println!();
    println!("USAGE:");
    println!("    graphlite-studio <database.db> [default-node-limit]");
    println!();
    println!("The server binds to 127.0.0.1 on an OS-assigned port and opens your");
    println!("browser. The database is opened read-only, so a writer may keep running.");
}

/// 处理一个连接。只支持 GET：本服务没有任何写端点。
fn handle_connection(
    path: &Path,
    mut stream: TcpStream,
    default_limit: usize,
) -> std::io::Result<()> {
    // 读请求行。不支持请求体：只有 GET，因此无需解析 Content-Length，
    // 也就无需防御 chunked 编码之类的边界情况。
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }

    // 必须把请求头读到空行：否则连接复用时会残留请求头，
    // 被误当成下一个请求的请求行。
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let (request_path, query) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    };

    // 页面本身不需要数据库：先把它放行，避免为一份静态资源去抢文件锁。
    // 这不是微优化——浏览器打开页面时会立刻并发请求数据，若静态资源也要锁，
    // 只会增加与写者碰撞的机会。
    let (status, content_type, body) = if method != "GET" {
        (
            405,
            "text/plain; charset=utf-8",
            "Only GET is supported.".to_string(),
        )
    } else if request_path == "/" || request_path == "/index.html" {
        (200, "text/html; charset=utf-8", PAGE.to_string())
    } else {
        // 只有数据端点才开库：持锁时间控制在单次请求内，写者可在请求间隙写入
        match open_for_request(path) {
            Ok(db) => route(&db, request_path, query, default_limit),
            // 写者持锁时返回 503 而不是 500：这是可重试的瞬时状态，不是服务故障
            Err(e) => (
                503,
                "application/json; charset=utf-8",
                error_json(&format!(
                    "Database is busy (a writer holds the lock). Retry in a moment. \n{}",
                    e
                )),
            ),
        }
    };

    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status,
        status_text(status),
        content_type,
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        503 => "Service Unavailable",
        500 => "Internal Server Error",
        _ => "Error",
    }
}

/// 路由分发。返回 `(状态码, Content-Type, 响应体)`。
fn route(
    db: &GraphLite,
    request_path: &str,
    query: &str,
    default_limit: usize,
) -> (u16, &'static str, String) {
    match request_path {
        "/api/graph" => {
            let limit = param(query, "limit")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(default_limit);

            // 明确拒绝而非静默截断：用户需要知道「你想看的部分没全给你」
            if limit > MAX_EXPORT {
                return (
                    400,
                    "application/json; charset=utf-8",
                    error_json(&format!(
                        "Requested {} nodes but the maximum is {}. \
                         Narrow the view (fewer nodes) rather than rendering a graph \
                         no browser can lay out.",
                        limit, MAX_EXPORT
                    )),
                );
            }

            match db.export_subgraph(limit) {
                Ok(export) => (200, "application/json; charset=utf-8", export.to_json()),
                Err(e) => (
                    500,
                    "application/json; charset=utf-8",
                    error_json(&e.to_string()),
                ),
            }
        }

        "/api/stats" => {
            let labels = db.labels();
            let body = format!(
                "{{\"nodes\":{},\"edges\":{},\"labels\":{},\"constraints\":{},\"read_only\":{}}}",
                db.node_count(),
                db.edge_count(),
                json_string_array(&labels),
                db.unique_constraints().len(),
                db.is_read_only()
            );
            (200, "application/json; charset=utf-8", body)
        }

        "/api/query" => {
            // 只读查询端点：供页面里的 Cypher 输入框使用。
            // 写语句会被只读句柄拒绝（见 `GraphLite::reject_write`），
            // 因此这里不需要另做一次判定——两处判定会有一处先过期。
            let Some(cypher) = param(query, "q") else {
                return (
                    400,
                    "application/json; charset=utf-8",
                    error_json("Missing query parameter `q`."),
                );
            };
            let decoded = url_decode(cypher);
            match db.run_cypher(&decoded) {
                Ok(result) => {
                    let columns = result.columns.clone();
                    let rows: Vec<Vec<String>> = result
                        .rows
                        .iter()
                        .map(|r| {
                            r.values
                                .iter()
                                .map(|v| match v {
                                    graphlite::Value::Int(i) => i.to_string(),
                                    graphlite::Value::Float(f) => f.to_string(),
                                    graphlite::Value::Bool(b) => b.to_string(),
                                    graphlite::Value::String(s) => s.clone(),
                                })
                                .collect()
                        })
                        .collect();
                    let body = format!(
                        "{{\"columns\":{},\"rows\":{}}}",
                        json_string_array(&columns),
                        json_rows(&rows)
                    );
                    (200, "application/json; charset=utf-8", body)
                }
                Err(e) => (
                    400,
                    "application/json; charset=utf-8",
                    error_json(&e.to_string()),
                ),
            }
        }

        _ => (
            404,
            "text/plain; charset=utf-8",
            format!("Not found: {}", request_path),
        ),
    }
}

/// 从查询串中取参数值（不做 URL 解码，调用方按需处理）。
fn param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        if k == key {
            Some(v)
        } else {
            None
        }
    })
}

/// 最小的百分号解码。只处理 `%XX` 与 `+`——Cypher 查询里会出现空格与引号。
fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                match (hi, lo) {
                    (Some(h), Some(l)) => {
                        out.push((h * 16 + l) as u8);
                        i += 3;
                    }
                    // 非法转义按字面量处理，不报错：一个坏字符不该让整条查询失败
                    _ => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn json_string_array(items: &[String]) -> String {
    let mut out = String::from("[");
    for (i, s) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        graphlite::json::write_escaped(&mut out, s);
    }
    out.push(']');
    out
}

fn json_rows(rows: &[Vec<String>]) -> String {
    let mut out = String::from("[");
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('[');
        for (j, cell) in row.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            graphlite::json::write_escaped(&mut out, cell);
        }
        out.push(']');
    }
    out.push(']');
    out
}

fn error_json(msg: &str) -> String {
    let mut out = String::from("{\"error\":");
    graphlite::json::write_escaped(&mut out, msg);
    out.push('}');
    out
}

/// 尽力打开浏览器。失败**不致命**：打印链接让用户自己点，
/// 总比因为没装 `xdg-open` 就退出要好。
fn open_browser(url: &str) {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };

    match std::process::Command::new(program).arg(url).spawn() {
        Ok(_) => {}
        Err(_) => {
            eprintln!();
            eprintln!("Could not launch a browser automatically.");
            eprintln!("Open this URL manually: {}", url);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn param_extracts_by_key() {
        assert_eq!(param("limit=50&x=1", "limit"), Some("50"));
        assert_eq!(param("limit=50&x=1", "x"), Some("1"));
        assert_eq!(param("limit=50", "missing"), None);
    }

    #[test]
    fn url_decode_handles_percent_and_plus() {
        assert_eq!(url_decode("MATCH+%28a%29"), "MATCH (a)");
        // 中文按 UTF-8 百分号编码
        assert_eq!(url_decode("%E9%9D%92%E4%BA%91"), "青云");
    }

    /// 非法转义按字面量保留，不得让整条查询失败。
    #[test]
    fn url_decode_tolerates_bad_escapes() {
        assert_eq!(url_decode("100%"), "100%");
        assert_eq!(url_decode("%ZZ"), "%ZZ");
        assert_eq!(url_decode("a%2"), "a%2");
    }

    #[test]
    fn json_helpers_escape_correctly() {
        assert_eq!(json_string_array(&["a\"b".to_string()]), "[\"a\\\"b\"]");
        assert_eq!(
            json_rows(&[vec!["x".to_string(), "y".to_string()]]),
            "[[\"x\",\"y\"]]"
        );
        assert_eq!(error_json("bad"), "{\"error\":\"bad\"}");
    }
}

//! GraphLite Studio 的端到端测试。
//!
//! ## 为什么必须真的起服务、发 HTTP
//!
//! Studio 的价值全在「浏览器能否拿到正确的图数据」这一条上。只测内部函数无法
//! 覆盖：路由分发、HTTP 报文格式、JSON 转义、以及**锁的生命周期**——最后一项
//! 恰恰是初版出错的地方（启动时开库并长期持锁，导致用户开着界面时 Agent 完全
//! 无法写入）。
//!
//! 因此这里的测试启动真实子进程、用真实 TCP 连接发请求、并断言写者可以在
//! Studio 运行期间成功写入。

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// 一个已启动的 Studio 子进程；`Drop` 时终止。
struct Studio {
    child: Child,
    port: u16,
}

impl Drop for Studio {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Studio {
    /// 启动 Studio 并等待它打印监听端口。
    ///
    /// 等待而不是 sleep 固定时长：端口是内核分配的，只能从输出里读；固定 sleep
    /// 要么让测试变慢，要么在慢机器上偶发失败。
    fn start(db: &Path, limit: usize) -> Studio {
        let mut child = Command::new(env!("CARGO_BIN_EXE_graphlite-studio"))
            .arg(db)
            .arg(limit.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to start graphlite-studio");

        let stdout = child.stdout.take().expect("stdout pipe");
        let mut reader = BufReader::new(stdout);

        let deadline = Instant::now() + Duration::from_secs(20);
        let mut port = None;
        while Instant::now() < deadline {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            if let Some(idx) = line.find("127.0.0.1:") {
                let tail = &line[idx + "127.0.0.1:".len()..];
                let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(p) = digits.parse::<u16>() {
                    port = Some(p);
                    break;
                }
            }
        }

        let port = port.expect("studio did not report a listening port");
        Studio { child, port }
    }

    /// 发一个最小 HTTP/1.1 GET，返回 `(状态码, 响应体)`。
    fn get(&self, path: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("set timeout");

        let request = format!(
            "GET {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            path
        );
        stream.write_all(request.as_bytes()).expect("write request");
        stream.flush().expect("flush");

        let mut raw = String::new();
        stream.read_to_string(&mut raw).expect("read response");

        let status = raw
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0);

        // 响应体在首个空行之后；`Connection: close` 保证能读到结尾
        let body = match raw.split_once("\r\n\r\n") {
            Some((_, b)) => b.to_string(),
            None => String::new(),
        };
        (status, body)
    }
}

/// 造一个含多标签、多关系类型的测试库。
fn make_db(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("studio.db");
    let db = graphlite::GraphLite::open(&path).unwrap();

    let mut ids: Vec<u64> = Vec::new();
    db.with_transaction(|tx| {
        use std::collections::{HashMap, HashSet};
        for i in 0..12u64 {
            let mut m = HashMap::new();
            m.insert(
                "name".to_string(),
                graphlite::Value::from(format!("角色{}", i)),
            );
            let labels: HashSet<String> = if i % 3 == 0 {
                ["Character", "Hero"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            } else {
                HashSet::from(["Character".to_string()])
            };
            ids.push(tx.add_node(labels, m)?);
        }
        for i in 0..3u64 {
            let mut m = HashMap::new();
            m.insert(
                "name".to_string(),
                graphlite::Value::from(format!("门派{}", i)),
            );
            ids.push(tx.add_node(HashSet::from(["Sect".to_string()]), m)?);
        }
        for i in 0..20 {
            tx.add_edge(
                ids[i % 12],
                ids[(i * 5 + 3) % 12],
                "KNOWS",
                HashMap::new(),
                1.0,
            )?;
        }
        for i in 0..6 {
            tx.add_edge(
                ids[i % 12],
                ids[12 + i % 3],
                "MEMBER_OF",
                HashMap::new(),
                1.0,
            )?;
        }
        Ok(())
    })
    .unwrap();

    db.checkpoint().unwrap();
    drop(db);
    path
}

#[test]
fn test_studio_serves_page_stats_and_graph() {
    let dir = TempDir::new().unwrap();
    let db_path = make_db(&dir);
    let studio = Studio::start(&db_path, 300);

    // 页面：必须是完整 HTML，且不含外部资源引用（离线可用）
    let (status, page) = studio.get("/");
    assert_eq!(status, 200, "the page must be served");
    assert!(
        page.contains("GraphLite Studio"),
        "page must be the studio UI"
    );
    assert!(
        !page.contains("http://") && !page.contains("https://"),
        "the page must not reference external resources; it has to work offline"
    );

    // 统计
    let (status, stats) = studio.get("/api/stats");
    assert_eq!(status, 200);
    assert!(
        stats.contains("\"nodes\":15"),
        "stats must count nodes: {}",
        stats
    );
    assert!(
        stats.contains("\"edges\":26"),
        "stats must count edges: {}",
        stats
    );
    assert!(
        stats.contains("\"read_only\":true"),
        "studio must report itself read-only: {}",
        stats
    );

    // 图数据：节点、边、标签、中文属性都要正确
    let (status, graph) = studio.get("/api/graph?limit=100");
    assert_eq!(status, 200);
    assert!(
        graph.contains("\"nodes\":["),
        "graph payload must have nodes"
    );
    assert!(
        graph.contains("\"edges\":["),
        "graph payload must have edges"
    );
    assert!(
        graph.contains("角色0"),
        "non-ASCII properties must survive JSON encoding: {}",
        &graph[..graph.len().min(300)]
    );
    assert!(
        graph.contains("\"type\":\"KNOWS\""),
        "edge types must be exported"
    );
    assert!(
        graph.contains("\"truncated\":false"),
        "a complete export must not claim truncation"
    );

    // 404 与 405
    assert_eq!(studio.get("/nope").0, 404);
}

/// 超出上限必须**明确拒绝**，而不是静默给出一个被截断的图。
///
/// 一个静默截断的图会让人以为自己看到了全貌——这正是需要避免的误导。
#[test]
fn test_studio_rejects_excessive_limit() {
    let dir = TempDir::new().unwrap();
    let db_path = make_db(&dir);
    let studio = Studio::start(&db_path, 300);

    let (status, body) = studio.get("/api/graph?limit=999999");
    assert_eq!(
        status, 400,
        "an excessive limit must be rejected, not silently clamped"
    );
    assert!(
        body.contains("maximum"),
        "the error must state the maximum, got: {}",
        body
    );
}

/// 只读的查询端点必须能跑 Cypher；写语句必须被拒绝。
#[test]
fn test_studio_query_endpoint_is_read_only() {
    let dir = TempDir::new().unwrap();
    let db_path = make_db(&dir);
    let studio = Studio::start(&db_path, 300);

    let (status, body) = studio.get("/api/query?q=MATCH%20(c:Sect)%20RETURN%20count(*)%20AS%20n");
    assert_eq!(status, 200);
    assert!(
        body.contains("\"columns\":[\"n\"]") && body.contains("[\"3\"]"),
        "read query must return the count, got: {}",
        body
    );

    // 写语句：只读句柄会拒绝，端点返回 4xx
    let (status, body) = studio.get("/api/query?q=CREATE%20(x:ShouldNotExist)");
    assert!(
        (400..500).contains(&status),
        "a write through the read-only endpoint must fail, got {} {}",
        status,
        body
    );

    // 库中不得出现该节点——这是本测试最关键的断言
    drop(studio);
    let db = graphlite::GraphLite::open(&db_path).unwrap();
    let res = db
        .run_cypher("MATCH (x:ShouldNotExist) RETURN count(*) AS n")
        .unwrap();
    assert_eq!(
        res.rows[0].values[0].as_i64(),
        Some(0),
        "the rejected write must not have created anything"
    );
}

/// **本文件最重要的测试。**
///
/// Studio 若在启动时开库并长期持有读锁，用户开着界面期间写者就完全无法写入——
/// 而「Agent 在后台写、我在前台看」正是这个工具存在的理由。初版正是这样写的，
/// 端到端验证时才发现。
///
/// 现在锁只在单个请求期间持有，因此写者可以在请求间隙成功写入，且 Studio 下一次
/// 请求就能看到新数据。
#[test]
fn test_studio_allows_writer_between_requests() {
    let dir = TempDir::new().unwrap();
    let db_path = make_db(&dir);
    let studio = Studio::start(&db_path, 300);

    // 先确认 Studio 可用
    let (status, before) = studio.get("/api/stats");
    assert_eq!(status, 200);
    assert!(before.contains("\"nodes\":15"));

    // Studio 运行期间，另一个句柄必须能写入
    {
        let db = graphlite::GraphLite::open(&db_path).unwrap_or_else(|e| {
            panic!(
                "a writer must be able to open the database while studio runs, got: {}",
                e
            )
        });
        db.add_node(
            std::collections::HashSet::from(["Character".to_string()]),
            std::collections::HashMap::new(),
        )
        .expect("a writer must be able to write while studio runs");
        db.checkpoint().expect("writer checkpoint");
    }

    // Studio 的下一次请求必须看到新数据
    let (status, after) = studio.get("/api/stats");
    assert_eq!(status, 200);
    assert!(
        after.contains("\"nodes\":16"),
        "studio must observe the writer's new node; before={}, after={}",
        before,
        after
    );
}

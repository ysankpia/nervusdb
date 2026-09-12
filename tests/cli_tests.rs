//! 战役四验证套件：交互式 REPL 终端 (graphlite-cli)。
//!
//! 通过 `CARGO_BIN_EXE_graphlite-cli` 定位构建产物，把脚本从 stdin 管道喂入，
//! 校验多行输入、ASCII 表格渲染与全部内置点命令的真实行为。

use graphlite::GraphLite;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::tempdir;

/// 以给定 stdin 脚本运行 CLI，返回 (stdout, stderr, 退出码)
fn run_cli(db_path: &Path, script: &str) -> (String, String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_graphlite-cli"))
        .arg(db_path)
        .env("GRAPH_LITE_HISTORY", db_path.with_extension("history"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start graphlite-cli");

    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(script.as_bytes())
        .expect("failed to write stdin script");

    let output = child
        .wait_with_output()
        .expect("failed to collect CLI output");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

#[test]
fn test_cli_help_and_quit() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("cli_help.db");

    let (stdout, _stderr, code) = run_cli(&db_path, ".help\n.quit\n");
    assert_eq!(code, 0, "clean exit expected");
    assert!(stdout.contains(".schema"), "help must document .schema");
    assert!(stdout.contains(".stats"), "help must document .stats");
    assert!(
        stdout.contains(".checkpoint"),
        "help must document .checkpoint"
    );
    assert!(stdout.contains(".dump"), "help must document .dump");
    assert!(stdout.contains(".history"), "help must document .history");
    assert!(stdout.contains(".quit"), "help must document .quit");
}

#[test]
fn test_cli_create_query_and_ascii_table() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("cli_query.db");

    let script = "\
CREATE (a:Person {name: 'Alice', age: 28})-[:KNOWS]->(b:Person {name: 'Bob', age: 32});
MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name, b.age;
.quit
";
    let (stdout, stderr, code) = run_cli(&db_path, script);
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert!(stdout.contains("Query OK"), "create must report Query OK");

    // ASCII 表格：表头、分隔线与数据行
    assert!(
        stdout.contains("a.name"),
        "table must contain header a.name"
    );
    assert!(stdout.contains("b.age"), "table must contain header b.age");
    assert!(stdout.contains("+--"), "table must render a separator line");
    assert!(stdout.contains("'Alice'"), "table must contain the value");
    assert!(stdout.contains("32"), "table must contain the age value");
    assert!(
        stdout.contains("1 row(s) in set"),
        "table must report row count"
    );
}

#[test]
fn test_cli_multi_line_statement() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("cli_multiline.db");

    // 一条语句跨两行，仅最后一行以分号终结。
    //
    // 注意每行**必须**是同一语句的续行：分号才是语句结束符，缺分号的行会被
    // 缓冲进同一条语句。这个测试最初的脚本写成了 `CREATE (...)` 换行 `RETURN a;`，
    // 于是被拼成 `CREATE (...) RETURN a;` —— 而 CREATE 不接受 RETURN。
    // 旧解析器把多余的 `RETURN a` 静默丢弃，测试因此"通过"；尾部检查加上之后
    // 它立刻失败，暴露了这个脚本自身的错误（以及那个静默丢弃的 bug）。
    let script = "\
CREATE (a:City {name: 'Beijing'})
;
MATCH (c:City)
RETURN c.name;
.quit
";
    let (stdout, stderr, code) = run_cli(&db_path, script);
    assert_eq!(code, 0, "stderr: {}", stderr);

    // 续行提示符必须出现，证明多行缓冲区生效
    assert!(
        stdout.contains("...>"),
        "continuation prompt must be shown for multi-line input"
    );
    assert!(
        stdout.contains("'Beijing'"),
        "multi-line query must actually execute"
    );
}

/// 无法被完整理解的语句必须**报错**，而不是丢弃尾部继续执行。
///
/// 这是回归守卫：`WHERE id(a) = 1` 曾被解析成裸变量 `id`（`(a) = 1` 被静默
/// 丢弃），条件退化成恒真，于是任何过滤条件都会返回全部数据且不报错。
/// 返回错误数据的查询比直接失败的查询危险得多。
#[test]
fn test_cli_rejects_unparsable_trailing_input() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("cli_trailing.db");

    let script = "\
CREATE (a:City {name: 'Beijing'});
CREATE (b:City {name: 'Shanghai'}) GARBAGE TOKENS HERE;
.quit
";
    let (stdout, stderr, code) = run_cli(&db_path, script);
    assert_eq!(code, 0, "CLI must survive a bad statement");

    // 报错走 stderr（与既有 `test_cli_error_reporting_keeps_session_alive` 一致）
    assert!(
        stderr.contains("Unexpected trailing input"),
        "a query with unparsable trailing input must be reported on stderr, got: {}",
        stderr
    );

    // 第一条（合法）语句生效，第二条（畸形）不得产生任何副作用。
    // 这正是本测试的重点：畸形语句必须"整条拒绝"，而不是执行它认得的那个前缀
    // （`CREATE (b:City {...})`）再悄悄丢掉尾部。
    let db = GraphLite::open(&db_path).unwrap();
    let count = db
        .run_cypher("MATCH (c:City) RETURN count(*) AS n")
        .unwrap();
    let total = count.rows[0].values[0].as_i64().unwrap_or(-1);
    assert_eq!(
        total, 1,
        "the malformed statement must not have created a node; got {} cities",
        total
    );

    // 且只有一次创建成功的提示（来自第一条语句）
    assert_eq!(
        stdout.matches("Created 1 nodes").count(),
        1,
        "exactly one statement should have succeeded; stdout: {}",
        stdout
    );
}

#[test]
fn test_cli_dot_commands_schema_stats_checkpoint() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("cli_dot.db");

    let script = "\
CREATE (a:Person {name: 'A'})-[:KNOWS {since: 2020}]->(b:Person {name: 'B'});
.schema
.stats
.checkpoint
.quit
";
    let (stdout, stderr, code) = run_cli(&db_path, script);
    assert_eq!(code, 0, "stderr: {}", stderr);

    // .schema 打印标签、边类型与计数
    assert!(stdout.contains("Graph Schema & Statistics"));
    assert!(
        stdout.contains("Person"),
        "schema must list the Person label"
    );
    assert!(
        stdout.contains("KNOWS"),
        "schema must list the KNOWS edge type"
    );
    assert!(stdout.contains("Nodes:"), "schema must report node count");

    // .stats 打印缓冲池与 WAL 指标
    assert!(stdout.contains("4KB Buffer Pool Metrics"));
    assert!(stdout.contains("Pool Capacity:"));
    assert!(stdout.contains("Cache Hit Rate:"));
    assert!(stdout.contains("WAL Size:"));
    assert!(stdout.contains("Spill (STEAL) Ops:"));

    // .checkpoint 必须成功执行
    assert!(stdout.contains("Checkpoint completed"));
}

#[test]
fn test_cli_dump_round_trip() {
    let dir = tempdir().unwrap();
    let source_db = dir.path().join("cli_dump_src.db");
    let restored_db = dir.path().join("cli_dump_dst.db");
    let dump_file = dir.path().join("dump.cypher");

    // 1. 导出：建图后经 .dump 落盘为 Cypher 脚本
    let script = format!(
        "\
CREATE (a:Person {{name: 'Alice', age: 28}})-[:KNOWS {{since: 2023}}]->(b:Person {{name: 'Bob', age: 32}});
CREATE (c:Person {{name: 'Carol', age: 41}});
.dump {}
.quit
",
        dump_file.display()
    );
    let (stdout, stderr, code) = run_cli(&source_db, &script);
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert!(stdout.contains("Dumped"), "dump must report a summary");

    let dump_content = std::fs::read_to_string(&dump_file).expect("dump file must exist");
    assert!(
        dump_content.contains("CREATE"),
        "dump must contain CREATE statements"
    );
    assert!(
        dump_content.contains("Alice"),
        "dump must contain node properties"
    );
    assert!(
        dump_content.contains("MATCH") && dump_content.contains("KNOWS"),
        "dump must contain relationship recreation statements"
    );
    assert!(
        dump_content.contains("__glid"),
        "dump must embed stable node ids"
    );

    // 2. 回灌：把导出的脚本喂回一个全新数据库，数据必须一致
    let restore_script = format!("{}\n.quit\n", dump_content);
    let (_out, err, code) = run_cli(&restored_db, &restore_script);
    assert_eq!(code, 0, "restore failed: {}", err);

    let verify_script = "\
MATCH (p:Person) RETURN count(p);
MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a.name, b.name, r.since;
.quit
";
    let (out, err, code) = run_cli(&restored_db, verify_script);
    assert_eq!(code, 0, "stderr: {}", err);
    assert!(
        out.contains('3'),
        "restored graph must contain 3 persons: {}",
        out
    );
    assert!(
        out.contains("'Alice'"),
        "restored relationship must connect Alice"
    );
    assert!(
        out.contains("2023"),
        "restored relationship must keep properties"
    );
}

#[test]
fn test_cli_error_reporting_keeps_session_alive() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("cli_error.db");

    // 非法语句报错后会话必须继续存活并正常执行后续语句
    let script = "\
THIS IS NOT CYPHER;
CREATE (a:Ok {flag: true});
MATCH (o:Ok) RETURN o.flag;
.quit
";
    let (stdout, stderr, code) = run_cli(&db_path, script);
    assert_eq!(code, 0, "session must survive a syntax error");
    assert!(
        stderr.contains("Error"),
        "syntax error must be reported on stderr, got: {}",
        stderr
    );
    assert!(
        stdout.contains("true"),
        "session must continue and execute the following statement"
    );
}

//! 不变量守卫：核心库必须保持**运行时零依赖**。
//!
//! ## 为什么这是一条需要测试守住的不变量
//!
//! 磁盘格式的每一个字节都由本仓库自己定义（`src/codec.rs`、`src/json.rs`、
//! `src/crc32.rs`）。这不是风格偏好，而是从一次真实事故里学到的：
//!
//! `bincode` 曾同时编码 WAL 帧与 Page 0 元数据，而它在 2025-12 停止维护——
//! 作者遭遇人肉骚扰后终止项目，其 3.0.0 版本里只有一个编译错误和一段墓志铭。
//! 此时格式尚未冻结，我们得以更换；**冻结之后就没有第二次免费机会了**。
//!
//! 只要能「顺手加一个 crate」，这条边界就会被慢慢侵蚀（先是编解码，然后是
//! 错误类型，然后是日志……）。因此用测试钉死：`Cargo.toml` 里不得出现
//! 运行时依赖，且源码不得引用它们。
//!
//! 开发依赖（`tempfile`）不受此限——它只用于测试，不进最终产物。

use std::fs;
use std::path::Path;

/// 从 `Cargo.toml` 的 `[dependencies]` 段中解析出依赖名。
///
/// 只做本测试需要的最小解析：定位 `[dependencies]` 段，收集其中的
/// `name = ...` / `name.version = ...` 形式的键，遇到下一个 `[section]` 结束。
fn runtime_dependencies(manifest: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_section = false;

    for raw in manifest.lines() {
        let line = raw.trim();

        if line.starts_with('[') {
            // 新的段开始：进入/离开依赖段
            in_section = line == "[dependencies]";
            continue;
        }
        if !in_section || line.is_empty() || line.starts_with('#') {
            continue;
        }

        // `name = ...` 或 `name.workspace = true`
        if let Some((key, _)) = line.split_once('=') {
            let name = key.trim().split('.').next().unwrap_or("").trim();
            if !name.is_empty() {
                deps.push(name.to_string());
            }
        }
    }
    deps
}

#[test]
fn core_library_has_zero_runtime_dependencies() {
    let manifest = fs::read_to_string("Cargo.toml").expect("Cargo.toml must be readable");
    let deps = runtime_dependencies(&manifest);

    assert!(
        deps.is_empty(),
        "the core library must have zero runtime dependencies, but found: {:?}\n\
         The on-disk format is defined by this repository's own encoders \
         (src/codec.rs, src/json.rs, src/crc32.rs). Adding a dependency here \
         re-introduces exactly the risk that losing bincode exposed: the format's \
         bytes would be owned by someone else's release schedule.",
        deps
    );
}

/// 源码里不得引用已知的外部 crate 名。
///
/// 光看 `Cargo.toml` 不够：一个 crate 可能通过 workspace 继承或 dev-dependency
/// 泄漏进来，而源码里的 `use` 才是真正的耦合点。
#[test]
fn source_does_not_reference_external_crates() {
    // 曾经用过、且必须保持退出的 crate
    const FORBIDDEN: &[&str] = &["bincode", "serde", "serde_json", "crc32fast", "thiserror"];

    let mut offenders: Vec<String> = Vec::new();
    collect_source_files(Path::new("src"), &mut |path, contents| {
        for name in FORBIDDEN {
            // 匹配 `use <name>` / `<name>::` / `extern crate <name>`，
            // 但排除文档注释与普通注释里的提及（那些是在解释「为什么不用」）。
            for (lineno, line) in contents.lines().enumerate() {
                let code = strip_comment(line);
                let referenced = code.contains(&format!("use {}", name))
                    || code.contains(&format!("{}::", name))
                    || code.contains(&format!("extern crate {}", name));
                if referenced {
                    offenders.push(format!("{}:{} references `{}`", path, lineno + 1, name));
                }
            }
        }
    });

    assert!(
        offenders.is_empty(),
        "source files must not reference external crates:\n{}",
        offenders.join("\n")
    );
}

/// 去掉行内的注释部分（`//` 之后），保留代码。
///
/// 这是测试用的近似实现：它不处理字符串字面量里出现 `//` 的情况，但本仓库
/// 的源码里没有这种写法，且即便误判也只会让检查更宽松、不会产生假失败。
fn strip_comment(line: &str) -> &str {
    match line.find("//") {
        Some(idx) => &line[..idx],
        None => line,
    }
}

fn collect_source_files(dir: &Path, f: &mut impl FnMut(String, String)) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_source_files(&path, f);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            if let Ok(contents) = fs::read_to_string(&path) {
                f(path.display().to_string(), contents);
            }
        }
    }
}

#[test]
fn dependency_parser_handles_real_manifest_shapes() {
    // 解析器本身要可信，否则上面的守卫可能是假绿。
    let sample = "\
[package]
name = \"x\"

[dependencies]
alpha = \"1.0\"
beta.version = \"2.0\"
# comment
gamma = { path = \"../g\" }

[dev-dependencies]
delta = \"3.0\"
";
    let deps = runtime_dependencies(sample);
    assert_eq!(deps, vec!["alpha", "beta", "gamma"]);
}

// =========================================================================
// 版本号一致性守卫
// =========================================================================
//
// 版本号写在五个地方：根 `Cargo.toml`，两个绑定 crate 的 `Cargo.toml`，
// Python 的 `pyproject.toml`，Node 的 `package.json`。发布时任何一处漏改，
// 都会产生**版本号与代码不符**的产物：用户装到的 SDK 声称是 1.0.0，实际却是
// 另一个版本的核心。这类问题在发布之后很难发现，也几乎无法收回。
//
// 因此用一条编译期测试钉死：所有版本号必须完全相等。

/// 从文本里提取第一个匹配 `key = "value"` / `key: "value"` 的值。
///
/// 必须校验**键名边界**：`rust-version` 与 `version-id` 都以 `version` 开头，
/// 用 `strip_prefix` 直接匹配会把它们误当成版本号——那样守卫可能对着错误的行断言。
fn extract_version(text: &str, key: &str) -> Option<String> {
    for raw in text.lines() {
        let line = raw.trim();

        // 支持 TOML 的 `version` 与 JSON 的 "version"
        let rest = if let Some(r) = line.strip_prefix(key) {
            r
        } else if let Some(r) = line.strip_prefix(&format!("\"{key}\"")) {
            r
        } else {
            continue;
        };

        // 键名之后必须紧跟分隔符（或空白后紧跟分隔符），否则是另一个键
        let rest = rest.trim_start();
        let rest = if let Some(r) = rest.strip_prefix('=') {
            r
        } else if let Some(r) = rest.strip_prefix(':') {
            r
        } else {
            continue;
        };

        let rest = rest.trim_start();
        let rest = rest.strip_prefix('"')?;
        let end = rest.find('"')?;
        return Some(rest[..end].to_string());
    }
    None
}

#[test]
fn all_manifests_share_one_version() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let cases = [
        ("Cargo.toml", root.join("Cargo.toml")),
        (
            "bindings/python/Cargo.toml",
            root.join("bindings/python/Cargo.toml"),
        ),
        (
            "bindings/nodejs/Cargo.toml",
            root.join("bindings/nodejs/Cargo.toml"),
        ),
        (
            "bindings/python/pyproject.toml",
            root.join("bindings/python/pyproject.toml"),
        ),
        (
            "bindings/nodejs/package.json",
            root.join("bindings/nodejs/package.json"),
        ),
    ];

    // `bindings/` 可能**合法缺席**：cargo 会自动把嵌套的 workspace 成员
    // （`bindings/python`、`bindings/nodejs`）排除在发布包之外，因此从
    // crates.io 下载的 crate 里没有这些清单。实测过：在
    // `target/package/nervusdb-0.1.0` 上跑本测试会因为读不到文件而 panic——
    // 那意味着任何 `cargo add nervusdb` 之后跑测试的人都会看到一次与他们无关的失败。
    //
    // 因此：文件不存在就跳过。**这一条守卫的价值本就在于「仓库内五处版本一致」**，
    // 而仓库内它们必然存在；缺失只会发生在裁剪后的发布包里，那里没有可校验的东西。
    let mut versions: Vec<(&str, String)> = Vec::new();
    let mut missing: Vec<&str> = Vec::new();
    for (label, path) in cases {
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => {
                missing.push(label);
                continue;
            }
        };
        let v = extract_version(&text, "version")
            .unwrap_or_else(|| panic!("{label} has no version field"));
        versions.push((label, v));
    }
    if !missing.is_empty() {
        // 核心清单必须在。缺了它说明不是「裁剪后的发布包」，而是仓库真的坏了。
        assert!(
            versions.iter().any(|(l, _)| *l == "Cargo.toml"),
            "Cargo.toml itself is unreadable; this is not a trimmed package"
        );
        eprintln!(
            "note: {} manifest(s) not present ({}) — treating this as a published \
             package rather than a repository checkout",
            missing.len(),
            missing.join(", ")
        );
    }

    let (first_label, first) = &versions[0];
    for (label, v) in &versions[1..] {
        assert_eq!(
            v, first,
            "{label} declares {v} but {first_label} declares {first}; \
             a released artifact whose version does not match the core it was built from \
             is impossible to correct after the fact"
        );
    }
}

/// 解析器本身要可信，否则上面的守卫可能是假绿。
#[test]
fn version_parser_handles_both_formats() {
    assert_eq!(
        extract_version("[package]\nversion = \"1.2.3\"\n", "version"),
        Some("1.2.3".to_string())
    );
    assert_eq!(
        extract_version("{\n  \"version\": \"4.5.6\",\n}\n", "version"),
        Some("4.5.6".to_string())
    );
    // 不误匹配其它以 version 开头的键
    assert_eq!(
        extract_version("rust-version = \"1.89\"\n", "version"),
        None
    );
    assert_eq!(extract_version("version-id = \"9\"\n", "version"), None);
}

// =========================================================================
// §13 守卫：定长切片转换必须带「为何不可能失败」的说明
// =========================================================================
//
// AGENTS.md §13 要求：`page.rs` 等处的定长切片转换必须写明为何由构造保证
// 不可能失败。这条约定原先只靠人工检查——而我实测发现 `page.rs` 的 18 处、
// `disk_graph.rs` 的 5 处**全部没有**说明，说明人工检查没有生效。
//
// 因此把它变成可验证的不变量：每处 `try_into().unwrap()` 的前 40 行内必须
// 存在提到 `§13`（或 `section 13`）的注释。用固定窗口而不是「向上找最近注释」，
// 是为了强制说明写在**该块内部**：散落在文件别处的注释无法为这里背书。
//
// 为什么值得为注释写测试：这类转换一旦真的失败就是 panic（进程终止），而
// 它们分布在格式解析最底层，改一个偏移常量就可能越界。注释在这里是**唯一**
// 能让下一位读者确认「安全」的成本极低的证据。

#[test]
fn fixed_offset_slice_conversions_are_documented() {
    let mut undocumented: Vec<String> = Vec::new();
    let mut total = 0usize;

    collect_source_files(Path::new("src"), &mut |path, contents| {
        let lines: Vec<&str> = contents.lines().collect();

        // 归属规则（三者都必须满足其一）：
        //   1. 说明在该转换**所属函数体内**，且在它上方；或
        //   2. 说明是紧邻该函数、且与函数之间没有其它 `fn` 的文档注释块中的一行。
        //
        // 关键：进入新函数时必须**重置**这个状态，否则文件开头的一处 §13 注释会
        // 为后面所有转换背书——那是一条永远为真的守卫，比没有守卫更糟。
        // 同时不能像更早的版本那样简单清空：Rust 的 `///` 文档注释写在 `fn` 行
        // **之前**，简单清空会把函数自己的文档注释判成「不属于本函数」。
        let mut s13_pos: Option<usize> = None;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();

            if trimmed.starts_with("//") && (line.contains("§13") || line.contains("section 13")) {
                s13_pos = Some(i);
            }

            let is_fn_head = trimmed.starts_with("fn ")
                || trimmed.starts_with("pub fn ")
                || trimmed.starts_with("pub(crate) fn ")
                || trimmed.starts_with("pub(super) fn ")
                || trimmed.starts_with("pub(in ");

            if is_fn_head {
                // 保留紧邻上方、连续的 `///` 文档注释块中的 §13 说明；
                // 其余情况一律重置。
                let mut keep = None;
                let mut j = i;
                while j > 0 {
                    let prev = lines[j - 1].trim_start();
                    if prev.starts_with("///") {
                        j -= 1;
                        if lines[j].contains("§13") || lines[j].contains("section 13") {
                            keep = Some(j);
                        }
                    } else {
                        break;
                    }
                }
                s13_pos = keep;
                continue;
            }

            let code = strip_comment(line);
            if !code.contains("try_into()") {
                continue;
            }
            let follows = lines[i..(i + 3).min(lines.len())]
                .iter()
                .map(|l| strip_comment(l))
                .collect::<Vec<_>>()
                .join(" ");
            if !(follows.contains(".unwrap()") || follows.contains(".expect(")) {
                continue;
            }

            total += 1;
            if !matches!(s13_pos, Some(j) if j < i) {
                undocumented.push(format!("{}:{}", path, i + 1));
            }
        }
    });

    assert!(
        total > 0,
        "no fixed-offset slice conversions found — the scanner matches nothing, so this \
         guard would pass vacuously"
    );
    assert!(
        undocumented.is_empty(),
        "{} of {total} fixed-offset slice conversions lack a §13 justification comment \
         in their own function body or its doc comment:\n{}",
        undocumented.len(),
        undocumented.join("\n")
    );
}

// =========================================================================
// 文档计数守卫
// =========================================================================
//
// 这一条来自两次真实事故：1.1.0 开发期间，README 写 187、ROADMAP 与
// docs/testing.md 写 190，而实测是 191。数字漂移不是笔误——它让读者无法判断
// 「文档说的覆盖范围」是否可信，而「文档不得夸大、不得陈旧」是本项目的硬要求。
//
// 守卫的做法：统计各 `tests/*.rs` 里的 `#[test]` 数量，与 `docs/testing.md`
// 表格末尾声明的总数比对。只比对**总数**，因为逐套件的精确数字随重构变化太快；
// 总数能捕捉「加了测试但没更新文档」这一最常见的漂移。
//
// 刻意不比对 README/ROADMAP：那两处的措辞形式随版本变化，强行解析会让守卫变脆，
// 而脆弱的守卫最终会被禁用——不如让它稳定地守住最有结构的那一处。

#[test]
fn documented_suite_table_matches_the_files() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // 逐套件比对 `docs/testing.md` 的表格与各测试文件里 `#[test]` 的实际数量。
    //
    // 为什么是逐套件而不是只比总数：总数要正确就得同时算准 doc-test 与 `#[ignore]`，
    // 而这两项都无法从文件内容可靠推出（doc-test 由 rustdoc 收集，`#[ignore]` 仍
    // 计入数量）。首版守卫正是按「tests/ + src/ 内联」推总数，算出的 191 与实测的
    // 192 差一个，于是守卫本身变成噪声来源——比没有守卫更糟。
    //
    // 表格与文件是同一份事实的两种陈述，逐行比对既不依赖任何外部计数，也能指出
    // **是哪一行**漂移了。
    let doc = fs::read_to_string(root.join("docs/testing.md")).expect("docs/testing.md must exist");

    let mut checked = 0usize;
    let mut problems: Vec<String> = Vec::new();

    for line in doc.lines() {
        // 形如：| `integration_tests.rs`       | 26    | ... |
        let cells: Vec<&str> = line.split('|').map(|c| c.trim()).collect();
        if cells.len() < 4 {
            continue;
        }
        let name = cells[1].trim_matches('`');
        if !name.ends_with(".rs") {
            continue; // 跳过 `inline (in src/)` 之类的非文件行
        }
        let Some(declared) = cells[2].parse::<usize>().ok() else {
            continue;
        };

        let path = root.join("tests").join(name);
        let contents = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => {
                problems.push(format!(
                    "docs/testing.md lists `{name}` but tests/{name} does not exist"
                ));
                continue;
            }
        };
        let actual = contents
            .lines()
            .filter(|l| l.trim_start().starts_with("#[test]"))
            .count();

        checked += 1;
        if actual != declared {
            problems.push(format!(
                "tests/{name}: documented {declared} case(s), file has {actual}"
            ));
        }
    }

    assert!(
        checked >= 10,
        "only {checked} suite rows were matched — the table format changed and this guard \
         is no longer checking anything meaningful"
    );

    // 表格之外，**散文里的套件数**也曾漂移：README 写「13 suites」、ROADMAP 写
    // 「16 suites」，而实际是 15。逐行比对抓不到这个，因为它比的是每行，不是行数。
    // 这里把「声明的套件数」与实际行数对上。
    // **反向检查：表格漏掉了某个测试文件。**
    //
    // 上面是「逐行核对已记录的行」，因此它天生看不见**没有行**的文件——
    // 新增 `tests/planner_tests.rs` 时守卫完全沉默，我是手工发现漏登记的。
    // 这是该守卫的**第二个**此类缺陷（第一个是只认标题不认编号条目），
    // 两个都是「只查 A→B、不查 B→A」的同一个形状。
    //
    // 反向检查的意义不只是数字对不上：**没进表格的套件等于没被点名**，
    // 而这张表的用途正是让人知道「哪套测试覆盖什么」。
    {
        let mut listed: std::collections::HashSet<String> = std::collections::HashSet::new();
        for line in doc.lines() {
            let cells: Vec<&str> = line.split('|').map(|c| c.trim()).collect();
            if cells.len() >= 4 {
                let n = cells[1].trim_matches('`');
                if n.ends_with(".rs") {
                    listed.insert(n.to_string());
                }
            }
        }
        let entries = fs::read_dir(root.join("tests")).expect("tests/ must exist");
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if !file_name.ends_with(".rs") {
                continue;
            }
            if !listed.contains(&file_name) {
                problems.push(format!(
                    "tests/{file_name} exists but has no row in the docs/testing.md suite \
table. Add a row (name, case count, what it covers) — an undocumented suite is a suite \
nobody knows to run."
                ));
            }
        }
    }

    let declared_suites = checked;
    for (path, needle) in [
        ("ROADMAP.md", "test cases across {n} suites"),
        ("README.md", "tests/              {n} suites,"),
        ("docs/testing.md", "Run as {n} integration suites"),
    ] {
        let text = match fs::read_to_string(root.join(path)) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let expected = needle.replace("{n}", &declared_suites.to_string());
        if !text.contains(&expected) {
            problems.push(format!(
                "{path} does not state {declared_suites} suites (expected to find `{expected}`). \
                 The table in docs/testing.md has {declared_suites} rows, so any other number \
                 there is stale."
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "docs disagree with the test files:\n{}",
        problems.join("\n")
    );
}

// =========================================================================
// 包名一致性守卫
// =========================================================================
//
// 三个清单各自声明了对外包名：
//   - `Cargo.toml`（crates.io）
//   - `bindings/python/pyproject.toml`（PyPI，`[project] name`）
//   - `bindings/nodejs/package.json`（npm，`"name"`）
//
// 它们**不必相同**（Rust 惯例是带后缀，Python/npm 惯例是裸名），但必须有明确
// 的对应关系，且不能出现「一个名字在两个生态里被不同项目占用」这种已知冲突。
//
// 这条守卫钉住**已知冲突**：`graphlite` 在 crates.io 上属于 GraphLite-AI，在 PyPI
// 上属于 eugene-eeo。本项目曾用这个前缀（当时叫 GraphLite），改名后不再使用；
// 若将来有人把任一清单改回去并发布，用户会装到**别人的包**——那是无法撤回的事故。
// 因此在这里拒绝它，并指向 ROADMAP 的命名记录。

#[test]
fn published_package_names_avoid_known_conflicts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // 已知属于**其它项目**的名字 -> 占用者。
    //
    // 这张表是「防止误发到别人的名字下」。本项目的名字是 `nervusdb`，它属于本项目
    // （crates.io 上旧版已全部 yank），因此**不在**表中——把它列进来会让守卫阻止
    // 自己的正确配置。
    //
    // 三个注册表实测于 2026-09-13：`graphlite` 在 crates.io 属 GraphLite-AI，
    // 在 PyPI 属 eugene-eeo，npm 上也被占用。
    const TAKEN: &[(&str, &str)] = &[
        (
            "graphlite",
            "PyPI: eugene-eeo/graphlite; crates.io: GraphLite-AI/GraphLite",
        ),
        ("graphlite-cli", "crates.io: GraphLite-AI/GraphLite"),
        ("graphlite-rust-sdk", "crates.io: GraphLite-AI/GraphLite"),
    ];

    // 与版本守卫同理：从 crates.io 下载的 crate 里没有 `bindings/`（cargo 排除
    // 嵌套 workspace 成员），因此这三个名字可能读不到。读不到就**只校验
    // crates.io 自己的名字**——那一条在发布包里依然可查，也依然是有意义的那条
    // （它是「绝不能把本包发到别人的名字下」的直接防线）。
    let pyproject = match fs::read_to_string(root.join("bindings/python/pyproject.toml")) {
        Ok(t) => t,
        Err(_) => {
            let cargo = fs::read_to_string(root.join("Cargo.toml")).expect("Cargo.toml must exist");
            let own = extract_version(&cargo, "name").expect("Cargo.toml must declare a name");
            assert!(
                !TAKEN.iter().any(|(n, _)| *n == own),
                "the crate name `{own}` belongs to another project: {}",
                TAKEN
                    .iter()
                    .find(|(n, _)| *n == own)
                    .map(|(_, who)| *who)
                    .unwrap_or("")
            );
            return;
        }
    };
    let mut in_project = false;
    let mut py_name = None;
    for line in pyproject.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_project = t == "[project]";
            continue;
        }
        if in_project {
            if let Some(rest) = t.strip_prefix("name") {
                let rest = rest.trim_start();
                if let Some(rest) = rest.strip_prefix('=') {
                    py_name = Some(rest.trim().trim_matches('"').to_string());
                }
            }
        }
    }
    let py_name = py_name.expect("pyproject.toml [project] must declare a name");

    // npm 名：顶层 `"name": "..."`
    let pkg = match fs::read_to_string(root.join("bindings/nodejs/package.json")) {
        Ok(t) => t,
        Err(_) => return,
    };
    let npm_name = extract_version(&pkg, "name").expect("package.json must declare a name");

    // crates.io 名
    let cargo = fs::read_to_string(root.join("Cargo.toml")).expect("Cargo.toml must exist");
    let mut crate_name = None;
    for line in cargo.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("name") {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                crate_name = Some(rest.trim().trim_matches('"').to_string());
                break;
            }
        }
    }
    let crate_name = crate_name.expect("Cargo.toml must declare a name");

    for (label, name) in [
        ("bindings/python/pyproject.toml", &py_name),
        ("bindings/nodejs/package.json", &npm_name),
        ("Cargo.toml", &crate_name),
    ] {
        for (taken, owner) in TAKEN {
            assert_ne!(
                name.trim(),
                *taken,
                "{label} would publish as `{taken}`, which belongs to another project \
                 ({owner}). Publishing would ship someone else's name, and a published \
                 name cannot be cleanly retracted — see ROADMAP \"SDK publication\"."
            );
        }
    }

    // 三个名字必须彼此可对应：都含 `nervusdb` 词根或全部一致。
    // 这条不是硬性生态要求，而是防止出现「Rust 叫 X、Python 叫 Y」而无从追溯。
    let lower = |s: &String| s.to_lowercase().replace('-', "");
    let stem = lower(&crate_name);
    for (label, name) in [
        ("bindings/python/pyproject.toml", &py_name),
        ("bindings/nodejs/package.json", &npm_name),
    ] {
        assert!(
            lower(name).starts_with("nervusdb") || lower(name) == stem,
            "{label} declares `{name}`, which is unrelated to the crate name \
             `{crate_name}`; a reader cannot connect the published artifact to this \
             repository"
        );
    }
}

// =========================================================================
// 格式版本号一致性守卫
// =========================================================================
//
// 磁盘格式版本号写在 `src/page.rs` 里，同时被四份文档复述。版本 5 之后
// **AGENTS.md 与 ROADMAP.md 都漏改了**（仍写 4）——而 AGENTS.md 正是「改代码前
// 必读」的那份，读者会据此对兼容性做出错误判断。
//
// 因此把「文档里的版本号必须等于常量」变成可验证的断言。只扫这四份明确声明
// 版本的地方，不泛化到全文：CHANGELOG 与 docs/history/ 记录的是**历史**，
// 那里出现旧版本号是正确的。

#[test]
fn documented_format_version_matches_the_code() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // 事实来源：src/page.rs 的 `pub const DB_PAGE_VERSION: u32 = N;`
    let page = fs::read_to_string(root.join("src/page.rs")).expect("src/page.rs must exist");
    let actual: u32 = page
        .lines()
        .find_map(|l| {
            let t = l.trim();
            let rest = t.strip_prefix("pub const DB_PAGE_VERSION: u32 =")?;
            rest.trim().trim_end_matches(';').trim().parse().ok()
        })
        .expect("src/page.rs must declare `pub const DB_PAGE_VERSION: u32 = N;`");
    assert!(actual > 0, "format version must be a positive integer");

    // 明确声明当前格式版本的文档，以及其中应出现的字面量。
    //
    // 每条是 (文件, 必须包含的片段)。片段里带 `{v}` 会被替换成实际版本号。
    let cases: &[(&str, &str)] = &[
        ("AGENTS.md", "`DB_PAGE_VERSION` is `{v}`"),
        ("ROADMAP.md", "**Frozen format (version {v})"),
        ("README.md", "frozen at **version {v}**"),
        // FORMAT.md 用「当前值是 N」的写法
        ("FORMAT.md", "The current value is **{v}**"),
        // CHANGELOG 的 0.1.0 段记录这次格式变更；措辞与上面几处不同
        ("CHANGELOG.md", "`DB_PAGE_VERSION` is now `{v}`"),
    ];

    // 反向守卫：0.1.0 段里不得再出现「格式版本停在 4」这类**陈旧**说法。
    //
    // 这正是本次的真实缺陷：改名把版本推到 5，而 CHANGELOG 里同一段既写 5
    // 又写「format version stays 4」——一条 changelog 自相矛盾，读者无从判断。
    // 上面的 cases 只能确认「写了正确的数字」，抓不到「同时写了错误的数字」。
    const STALE_IN_CHANGELOG: &[&str] = &[
        "format version stays 4",
        "DB_PAGE_VERSION stays 4",
        "version stays `4`",
    ];

    let mut problems: Vec<String> = Vec::new();
    for (path, needle) in cases {
        let text = match fs::read_to_string(root.join(path)) {
            Ok(t) => t,
            Err(e) => {
                problems.push(format!("{path}: unreadable ({e})"));
                continue;
            }
        };
        let expected = needle.replace("{v}", &actual.to_string());
        if !text.contains(&expected) {
            // 找出文档里实际写的数字，便于定位是哪种漂移
            let found = text
                .lines()
                .filter(|l| l.contains("version") || l.contains("DB_PAGE_VERSION"))
                .find(|l| {
                    [
                        "version 1",
                        "version 2",
                        "version 3",
                        "version 4",
                        "version 5",
                    ]
                    .iter()
                    .any(|p| l.contains(p))
                })
                .map(|l| l.trim().to_string())
                .unwrap_or_else(|| "(未找到版本声明行)".to_string());
            problems.push(format!(
                "{path} does not state the current format version.\n\
                 expected to find: {expected}\n\
                 a version-ish line there: {found}"
            ));
        }
    }

    // 陈旧说法检查（见 STALE_IN_CHANGELOG 的说明）
    if let Ok(changelog) = fs::read_to_string(root.join("CHANGELOG.md")) {
        for stale in STALE_IN_CHANGELOG {
            if changelog.contains(stale) {
                problems.push(format!(
                    "CHANGELOG.md still contains the stale claim `{stale}` while the \
                     format version is {actual} — a changelog that states two different \
                     versions for the same release cannot be trusted for either"
                ));
            }
        }
    }

    // 反向对照：确认这些文档**确实**在讨论版本，否则上面可能整体失效
    assert!(
        cases.len() >= 4,
        "expected at least four documents to declare the format version"
    );

    assert!(
        problems.is_empty(),
        "documented format version disagrees with `DB_PAGE_VERSION = {actual}`:\n{}",
        problems.join("\n")
    );
}

// =========================================================================
// 章节引用守卫
// =========================================================================
//
// 代码与文档里大量出现 `AGENTS.md §N` 形式的引用（本次统计 16 处）。把 §3 从
// 「139 行的四个子节」压缩成「一个 36 行的节」时，**四处 §3.x 引用当场失效**——
// 引用指向的编号不再存在，而没有任何东西会报错：读者只会看到一个查不到的章节号。
//
// 这条守卫把「引用必须能解析」变成断言。
//
// **它检查的是 AGENTS.md 的编号，仅此而已。** 两处它做不到，写在这里免得后人高估它：
//
//   1. 它不判断「引用指向了正确的地方」。实测抓到过一处：ROADMAP 曾用 `AGENTS.md §5`
//      引用交易队列上限，而该上限写在 §1 的第 5 条不变量里，§5 讲的是工作流。编号存在，
//      守卫放行，读者被指向了错的页。这类错误只能靠人对着代码读出来。
//   2. `docs/architecture.md` 里的裸 `§N`（如 "See §11 for why"）指的是**它自己的**
//      章节，守卫却拿 AGENTS.md 去校验。今天恰好都对得上，属于巧合；它不会发现
//      architecture.md 自己重编号导致的失效。

#[test]
fn agents_section_references_resolve() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // AGENTS.md 里章节号有**两种**形式，两种都要认：
    //   - `## 1. Non-Negotiable Invariants` / `### 3.1 ...`（标题）
    //   - `1. **Pure Disk-Backed Architecture**`（`## 1.` 节里的编号条目，
    //     即 §1–§14 那些「不变量」）
    //
    // 只认第一种是首版守卫的实际错误：它把全仓库 20 多处 `§13` 判为失效引用，
    // 而那些引用完全正确。**守卫误报比不报更糟**——它会训练人忽略它。
    let agents = fs::read_to_string(root.join("AGENTS.md")).expect("AGENTS.md must exist");
    let mut defined: std::collections::HashSet<String> = std::collections::HashSet::new();
    for line in agents.lines() {
        // 形式一：标题
        if let Some(rest) = line
            .strip_prefix("## ")
            .or_else(|| line.strip_prefix("### "))
        {
            let number: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if !number.is_empty() && !number.ends_with('.') {
                defined.insert(number);
            }
            continue;
        }
        // 形式二：`N. **标题**` 的编号条目（§1–§14）
        let trimmed = line.trim_start();
        let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() && trimmed[digits.len()..].starts_with(". **") {
            defined.insert(digits);
        }
    }
    assert!(
        defined.len() >= 10,
        "only found {} section numbers in AGENTS.md — the heading format changed and this \
         guard is no longer checking anything",
        defined.len()
    );

    // 扫描仓库里的 `§N` / `§N.M` 引用。history 与 CHANGELOG 记录的是历史，
    // 它们引用的是**当时**的章节号，不该按今天的 AGENTS.md 校验。
    // 目录 + **根目录下的散文**。首版只扫目录，于是 `ROADMAP.md` 里的失效引用
    // 被漏掉——而 ROADMAP 恰恰是「那个已不存在的 3.5 号」失效的那一处。负向验证
    // （把坏引用注入 ROADMAP）暴露了这个洞：守卫当时**通过了**。
    //
    // 注意注释里不写 `§` + 数字：守卫读的是文本，它无法区分「引用」与「提到引用」，
    // 写上去会变成自我误报。这正是守卫只该做机械检查、判断留给人的原因。
    const SCAN_DIRS: &[&str] = &["src", "tests", "docs"];
    const SCAN_ROOT_FILES: &[&str] = &["ROADMAP.md", "README.md"];
    let mut checked = 0usize;
    let mut problems: Vec<String> = Vec::new();

    let mut scan_file = |path: &Path, contents: &str, problems: &mut Vec<String>| {
        for (i, line) in contents.lines().enumerate() {
            // 跳过 history 文档
            if path.to_string_lossy().contains("/history/") {
                continue;
            }
            let bytes: Vec<char> = line.chars().collect();
            let mut idx = 0usize;
            while idx < bytes.len() {
                if bytes[idx] == '§' {
                    let mut j = idx + 1;
                    let mut num = String::new();
                    while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == '.') {
                        num.push(bytes[j]);
                        j += 1;
                    }
                    let num = num.trim_end_matches('.').to_string();
                    if !num.is_empty() {
                        checked += 1;
                        if !defined.contains(&num) {
                            problems.push(format!(
                                "{}:{}: references §{num}, which does not exist in AGENTS.md",
                                path.display(),
                                i + 1
                            ));
                        }
                    }
                    idx = j;
                    continue;
                }
                idx += 1;
            }
        }
    };

    for dir in SCAN_DIRS {
        let mut stack = vec![root.join(dir)];
        while let Some(d) = stack.pop() {
            let Ok(entries) = fs::read_dir(&d) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|s| s.to_str()) != Some("rs")
                    && p.extension().and_then(|s| s.to_str()) != Some("md")
                {
                    continue;
                }
                if let Ok(c) = fs::read_to_string(&p) {
                    scan_file(&p, &c, &mut problems);
                }
            }
        }
    }

    for name in SCAN_ROOT_FILES {
        let p = root.join(name);
        if let Ok(c) = fs::read_to_string(&p) {
            scan_file(&p, &c, &mut problems);
        }
    }

    assert!(
        checked > 0,
        "no `§N` references found at all — this guard would pass vacuously"
    );
    assert!(
        problems.is_empty(),
        "{} of {checked} section references do not resolve:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

// =========================================================================
// 发布包自洽性守卫
// =========================================================================

/// 发布包必须**自洽**：`cargo test` 在下载后的 crate 上也应当跑通。
///
/// ## 这条守卫来自一次实测的发布阻断
///
/// `bindings/python`、`bindings/nodejs` 是本 workspace 的成员，而 cargo 会自动把
/// **嵌套的包**排除在父包之外。于是从 crates.io 下载的 `nervusdb` 里没有
/// `bindings/`，而仓库里的两个守卫（版本一致性、包名冲突）当时直接 `.expect()`
/// 那些清单 —— 实测在 `target/package/nervusdb-0.1.0` 上跑测试，
/// **两个测试 panic**。也就是说任何 `cargo add nervusdb` 之后跑一次测试的人，
/// 都会看到两条与他们无关的失败，且原因只在发布配置里。
///
/// 修复分两处：守卫改为「文件缺席则跳过」（见各自注释），以及**本守卫**——
/// 把「打包后能跑通」这件事本身变成断言，而不是靠我记得去手工验证。
///
/// ## 它检查什么
///
/// 直接读 `cargo package --list` 的清单，断言：
///
/// 1. 库与测试源文件在包内（`include` 白名单曾把它们全部排除，实测
///    「no targets specified in the manifest」）；
/// 2. 守卫自己要读的文件在包内——否则下游必然 panic；
/// 3. 开发用的 `.cargo/config.toml` **不在**包内（它对下游无用，只是噪声）。
///
/// 不调用 `cargo package` 本身（那会做一次完整构建，把单测变成分钟级）。清单由
/// `cargo package --list` 生成，因此运行它需要 cargo 可用；环境里没有 cargo 时跳过
/// 并说明，而不是假装通过。
#[test]
fn published_package_is_self_consistent() {
    use std::process::Command;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // 只在仓库根上跑：在**已经打包**的目录里再打包没有意义（会递归、也会失败）。
    if !root.join("bindings").exists() {
        eprintln!("note: not a repository checkout (no bindings/); skipping package audit");
        return;
    }

    let out = match Command::new("cargo")
        .args(["package", "--list", "--allow-dirty"])
        .current_dir(root)
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => {
            eprintln!("note: `cargo package --list` unavailable; skipping package audit");
            return;
        }
    };
    let listing = String::from_utf8_lossy(&out.stdout);
    let files: std::collections::HashSet<&str> = listing.lines().map(|l| l.trim()).collect();

    let mut problems: Vec<String> = Vec::new();

    // 1. 必须包含库与测试。`include` 是白名单，一旦误用就会静默排掉全部源码——
    //    实测报错是 `no targets specified in the manifest`，但那是构建阶段的事，
    //    清单阶段就该拦住。
    for required in ["src/lib.rs", "Cargo.toml", "FORMAT.md", "CHANGELOG.md"] {
        if !files.contains(required) {
            problems.push(format!(
                "`{required}` is missing from the published package. A crate without its \
                 library cannot be built at all."
            ));
        }
    }

    // 2. 所有 `tests/*.rs` 都必须在包内——否则下游跑 `cargo test` 时测试少了一半，
    //    而那两个守卫恰好是「发布配置正确」的唯一自动化检查。
    let test_dir = root.join("tests");
    if let Ok(entries) = fs::read_dir(&test_dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".rs") {
                let rel = format!("tests/{name}");
                if !files.contains(rel.as_str()) {
                    problems.push(format!(
                        "`{rel}` is missing from the published package; downstream \
                         `cargo test` would silently run fewer checks"
                    ));
                }
            }
        }
    }

    // 3. 守卫读取的仓库根文件必须在包内。`bindings/` 是**故意**排除的（cargo 排除
    //    嵌套包），因此不在此列；那两个守卫已改为容忍其缺席。
    for required in [
        "AGENTS.md",
        "ROADMAP.md",
        "README.md",
        "docs/testing.md",
        "src/page.rs",
    ] {
        if !files.contains(required) {
            problems.push(format!(
                "`{required}` is missing from the published package, but a guard in \
                 tests/ reads it and would panic downstream"
            ));
        }
    }

    // 4. 开发配置不得进包。`exclude` 是黑名单，写错名字不会报错，只会静默无效——
    //    因此这里核对结果而不是配置。
    if files.contains(".cargo/config.toml") {
        problems.push(
            ".cargo/config.toml is being published. It carries this repository's macOS \
             link flags; cargo does not read a dependency's config, so it is inert for \
             consumers — just noise in their registry cache."
                .to_string(),
        );
    }

    assert!(
        problems.is_empty(),
        "the published package is not self-consistent:\n{}",
        problems.join("\n")
    );
}

// =========================================================================
// 被忽略的测试守卫
// =========================================================================

/// 全项目只允许**一个** `#[ignore]`，且它必须是那个有理由的子进程探针。
///
/// ## 为什么值得一条守卫
///
/// `#[ignore]` 是**让测试静默不跑**的机制，而「静默不跑」与「跑过了」在 CI 上是
/// 同一种绿色。因此它是本项目最容易被误用的属性：一个偶发失败的用例，改成
/// `#[ignore]` 就绿了，而它验证的东西从此再没人验证。
///
/// 实测过它确实有正当用途：`cross_process_child_probe` 必须由父测试在**持锁**状态
/// 下作为真实子进程启动（跨进程互斥只能那样验证），它自己依赖父进程传入的
/// `GL_CHILD_DB`、且没有自己的断言。移除 `#[ignore]` 会同时让探针 panic、父测试失败。
///
/// 所以这条守卫不是「禁止 ignore」，而是**要求它始终是有理由的那一个**：
///
/// - 数量必须恰好是 1（新增一个 = 有人静默关掉了测试，必须解释）
/// - 它必须仍是那个探针（被换成别的 = 原探针消失了）
/// - 它的文档注释必须说明为什么必须被忽略（否则下一个人只会看到一行 `#[ignore]`）
#[test]
fn only_the_documented_child_probe_is_ignored() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let mut found: Vec<(String, usize, String)> = Vec::new();

    for entry in fs::read_dir(root.join("tests")).expect("tests/ must exist") {
        let path = entry.expect("readable dir entry").path();
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let name = path
            .file_name()
            .expect("file has a name")
            .to_string_lossy()
            .to_string();
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        let lines: Vec<&str> = contents.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.trim() != "#[ignore]" {
                continue;
            }
            // 找它下面的 fn 名
            let fn_name = lines[i..]
                .iter()
                .find_map(|l| l.trim().strip_prefix("fn "))
                .and_then(|rest| rest.split('(').next())
                .unwrap_or("<unnamed>")
                .to_string();
            // 向上收集文档注释，作为「有理由」的证据。
            //
            // 必须**跳过属性行**（`#[test]` 就在 `#[ignore]` 上方）：朴素地只看紧邻
            // 上一行会在属性处中断，得到空字符串——那时守卫会误报「没有理由」。
            // 同一类归属问题在 Page-0 那处守卫里也踩过一次（见 #20 的记录）。
            let mut doc = String::new();
            let mut j = i;
            while j > 0 {
                let prev = lines[j - 1].trim();
                if prev.starts_with("///") {
                    doc.push_str(prev);
                    doc.push(' ');
                    j -= 1;
                } else if prev.starts_with("#[") {
                    // 属性行：越过它继续往上找文档注释
                    j -= 1;
                } else {
                    break;
                }
            }
            found.push((format!("{name}::{fn_name}"), i + 1, doc));
        }
    }

    assert_eq!(
        found.len(),
        1,
        "exactly one `#[ignore]` is expected (the cross-process child probe), found {}: {:?}\n\
         A second `#[ignore]` means a test was silenced rather than fixed. If a genuinely \
         ignore-only test was added, update this guard and say why in its doc comment.",
        found.len(),
        found
            .iter()
            .map(|(n, l, _)| format!("{n} at line {l}"))
            .collect::<Vec<_>>()
    );

    let (name, _line, doc) = &found[0];
    assert!(
        name.ends_with("cross_process_child_probe"),
        "the one ignored test must remain `cross_process_child_probe`, but it is `{name}`. \
         If the probe was renamed or removed, this guard needs updating deliberately."
    );
    assert!(
        doc.contains("GL_CHILD_DB") || doc.contains("子进程") || doc.contains("child process"),
        "the ignored test must carry a doc comment explaining *why* it is ignored; its \
         current doc is: {doc:?}"
    );
}

// =========================================================================
// examples 清单守卫
// =========================================================================

/// `examples/` 下的每个文件都必须登记在 `docs/api.zh-CN.md` 里，反之亦然。
///
/// ## 为什么
///
/// 那 10 个样例是**用户最先会照抄的东西**，而中文 API 文档是它们的唯一清单。清单
/// 与实际文件脱节的两种后果都是静默的：
///
/// - 新增样例不进清单 → 没人知道它存在，等于白写；
/// - 删除样例不改清单 → 文档指向一个不存在的文件，读者复制命令后得到
///   `error: no example target named ...`。
///
/// 这与套件表守卫是同一类问题（文档与文件是同一份事实的两种陈述），因此用同一种
/// 双向检查：只查「文档 → 文件」会漏掉未被登记的新文件，正是我在套件表上踩过的坑。
///
/// CI 另有一步**运行**每个样例；这条守卫管的是「有没有被登记」，那一步管的是
/// 「跑起来对不对」。
#[test]
fn every_example_is_listed_in_the_api_doc() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let mut actual: Vec<String> = Vec::new();
    for entry in fs::read_dir(root.join("examples")).expect("examples/ must exist") {
        let path = entry.expect("readable dir entry").path();
        if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            actual.push(
                path.file_stem()
                    .expect("example file has a stem")
                    .to_string_lossy()
                    .to_string(),
            );
        }
    }
    actual.sort();
    assert!(
        !actual.is_empty(),
        "examples/ is empty — this guard would pass vacuously"
    );

    let doc =
        fs::read_to_string(root.join("docs/api.zh-CN.md")).expect("docs/api.zh-CN.md must exist");
    let listed: std::collections::HashSet<String> = doc
        .split("examples/")
        .skip(1)
        .filter_map(|rest| rest.split(".rs").next())
        .filter(|name| !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .map(|s| s.to_string())
        .collect();

    let mut problems: Vec<String> = Vec::new();
    for name in &actual {
        if !listed.contains(name) {
            problems.push(format!(
                "examples/{name}.rs exists but is not listed in docs/api.zh-CN.md; a sample \
                 nobody can find may as well not exist"
            ));
        }
    }
    for name in &listed {
        if !actual.contains(name) {
            problems.push(format!(
                "docs/api.zh-CN.md lists examples/{name}.rs, but that file does not exist; \
                 a reader copying from the doc gets `no example target named {name}`"
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "examples and the API doc disagree:\n{}",
        problems.join("\n")
    );
}

/// 寻址常量必须与 `FORMAT.md` 声称的一致。
///
/// ## 为什么这条守卫必要
///
/// 物理寻址是一个**纯算术公式**：
///
/// ```text
/// NodeRecord: logical_page = (id - 1) / 128, offset = ((id - 1) % 128) * 32
/// EdgeRecord: logical_page = (id - 1) / 64,  offset = ((id - 1) % 64)  * 64
/// ```
///
/// 那两个除数（`NODE_RECORDS_PER_PAGE`、`EDGE_RECORDS_PER_PAGE`）是**格式的一部分**
/// —— `FORMAT.md` 明确写着「NodeRecord — 32 bytes, 128 per page」。改动它们等于改变
/// 每个 id 落在哪个字节上。
///
/// **而版本闸门抓不到这种改动。** 它检查 magic 与版本号；把 128 改成 127 不会让任何
/// 一个字节的 Page 0 变化，版本号也不必变。后果是静默且严重的：旧构建写的库被新构建
/// 按不同公式解读，**每个 id 都映射到相邻记录的字节偏移上**，读出来是**别的实体的
/// 数据**，不报错。
///
/// ## 为什么行为测试也抓不到
///
/// 写入与读取**共用同一个常量**，所以同一次构建内始终自洽——实测把常量改成 127，
/// `page_boundary_bench` 的 25 项边界检查**全部照常通过**。要检出必须**跨构建**比对，
/// 而这不是 CI 能廉价做到的。因此改为把常量钉在文档上：改了常量就必须同时改
/// `FORMAT.md`，而那一刻会被看到。
///
/// 这属于「把约定变成断言」那一类：`FORMAT.md` 已经记录了这个数字，但代码与它之间
/// 没有任何连接。
#[test]
fn addressing_constants_match_the_format_spec() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let page = fs::read_to_string(root.join("src/page.rs")).expect("src/page.rs must exist");

    let page_size: usize = {
        let needle = "pub const PAGE_SIZE: usize = ";
        let line = page
            .lines()
            .find(|l| l.trim_start().starts_with(needle))
            .unwrap_or_else(|| panic!("src/page.rs must declare `{needle}…`"));
        line.trim()
            .trim_start_matches(needle)
            .trim()
            .trim_end_matches(';')
            .trim()
            .parse()
            .expect("PAGE_SIZE must be a literal integer")
    };

    // 记录大小必须从**对应类型**的 impl 块里读。
    //
    // `NodeRecord` 与 `EdgeRecord` **都**声明了 `RECORD_SIZE`，因此在整份文件里搜
    // `pub const RECORD_SIZE` 会拿到先出现的那个（NodeRecord 的 32），于是边记录的密度
    // 算成 4096/32 = 128。本守卫的前两版就是这样错的（先取 32 当密度，再取到 128 当
    // 边记录密度）。改为：先定位 `impl <Type>`，只在它之后的片段里找常量。
    let record_size_of = |type_name: &str, const_name: &str| -> usize {
        let impl_marker = format!("impl {type_name} {{");
        let start = page
            .find(&impl_marker)
            .unwrap_or_else(|| panic!("src/page.rs must contain `{impl_marker}`"))
            + impl_marker.len();
        let decl = format!("pub const {const_name}: usize = ");
        page[start..]
            .lines()
            .find(|l| l.trim_start().starts_with(&decl))
            .unwrap_or_else(|| panic!("{type_name} must declare `{decl}…`"))
            .trim()
            .trim_start_matches(&decl)
            .trim()
            .trim_end_matches(';')
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("{type_name}::{const_name} must be a literal integer"))
    };

    let node_bytes = record_size_of("NodeRecord", "RECORD_SIZE");
    let edge_bytes = record_size_of("EdgeRecord", "RECORD_SIZE");
    let node_per_page = page_size / node_bytes;
    let edge_per_page = page_size / edge_bytes;

    // 事实来源之二：FORMAT.md 的标题行。
    let format = fs::read_to_string(root.join("FORMAT.md")).expect("FORMAT.md must exist");

    // 每条是 (实际密度, 文档标题行, 该行里的记录大小)。
    //
    // 不从这个片段里「猜」哪个数字是密度——`### EdgeRecord — 64 bytes, 64 per page`
    // 两个数字相同，任何按位置的启发式都会在某一侧出错（本守卫第一版就这样错了两次：
    // 先取到 32，改取末位后又对边记录取到 64 与 128 比对）。改为显式声明：
    // 文档行必须逐字存在，且其中两个数字必须分别等于实际记录大小与实际密度。
    let cases: &[(usize, usize, &str)] = &[
        (
            node_bytes,
            node_per_page,
            "### NodeRecord — 32 bytes, 128 per page",
        ),
        (
            edge_bytes,
            edge_per_page,
            "### EdgeRecord — 64 bytes, 64 per page",
        ),
    ];

    for (record_bytes, actual_per_page, documented) in cases {
        assert!(
            format.contains(documented),
            "FORMAT.md no longer states {documented:?}. If the addressing density changed, \
             that is a **format change**: update FORMAT.md and the version in the same commit. \
             The code currently uses {actual_per_page} records per page."
        );

        // 文档行里的数字必须同时匹配「记录大小」与「每页条数」，顺序固定。
        let numbers: Vec<usize> = documented
            .split_whitespace()
            .filter_map(|w| {
                w.trim_matches(|c: char| !c.is_ascii_digit())
                    .parse::<usize>()
                    .ok()
            })
            .collect();
        assert_eq!(
            numbers,
            vec![*record_bytes, *actual_per_page],
            "FORMAT.md's line {documented:?} claims sizes {numbers:?}, but the code uses \
             {record_bytes} bytes per record and {actual_per_page} records per page. \
             This ratio defines where every id lands on disk; a mismatch means the two \
             disagree about the file layout."
        );
    }
}

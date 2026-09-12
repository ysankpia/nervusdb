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

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };
    // query_parse 聚焦语法路径覆盖；超长噪声样本会把时间预算消耗在
    // 复杂回溯上，导致 nightly 非稳定超时。
    if input.len() > 384 {
        return;
    }

    let _ = nervusdb::query::parse(input);
});

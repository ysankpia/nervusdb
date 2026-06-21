#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };
    // prepare 阶段只保留可控输入规模，避免无效超长噪声掩盖真实回归。
    if input.len() > 384 {
        return;
    }

    let _ = nervusdb::query::prepare(input);
});

//! CRC32（IEEE 802.3 多项式，反射算法）—— 手写实现，零依赖。
//!
//! ## 为什么手写
//!
//! 与 `codec.rs` 同理：校验和直接写入磁盘格式（WAL 帧头、页校验和目录），
//! 其算法与结果必须由本仓库定义，不能随第三方 crate 的发布节奏改变。
//! `crc32fast` 本身是可靠且维护良好的 crate，但既然格式已经全部手写，
//! 保留它一处就失去了「依赖树为空」这条可验证的不变量。
//!
//! ## 算法
//!
//! 标准 CRC-32/ISO-HDLC：多项式 `0xEDB88320`（`0x04C11DB7` 的反射形式），
//! 初始值 `0xFFFFFFFF`，输入输出均反射，结果取反。
//!
//! 逐位实现是 `O(8n)`，对 4 KiB 页而言足够；页级校验在落盘/读入时才发生，
//! 不在写入热路径上。查表优化保留在同文件内，避免每次调用重复移位计算。
//!
//! ## 与 `crc32fast` 的一致性
//!
//! 两者都实现同一个标准算法，因此对任意输入结果相同。`tests/zero_dependency_tests.rs`
//! 用已知向量（`"123456789"` → `0xCBF43926`）钉住这一点。

/// 预计算查表：`TABLE[b]` 是单个字节 `b` 的 CRC 贡献。
///
/// 用 `const fn` 在编译期生成，运行期零初始化开销。
static TABLE: [u32; 256] = build_table();

const fn build_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

/// 增量式 CRC32 计算器。
///
/// WAL 分块写入时需要「先算前缀、再算后缀」，因此必须能保留中间状态
/// （`crc32fast::Hasher` 的等价物）。
#[derive(Debug, Clone)]
pub struct Hasher {
    state: u32,
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    pub fn new() -> Self {
        // 初始值取反：标准算法要求寄存器预置为全 1
        Self { state: 0xFFFF_FFFF }
    }

    pub fn update(&mut self, data: &[u8]) {
        let mut crc = self.state;
        for &byte in data {
            let idx = ((crc ^ byte as u32) & 0xFF) as usize;
            crc = (crc >> 8) ^ TABLE[idx];
        }
        self.state = crc;
    }

    pub fn finalize(&self) -> u32 {
        // 输出取反：与输入侧的取反配对，是标准算法的一部分
        self.state ^ 0xFFFF_FFFF
    }
}

/// 一次性计算一段字节的 CRC32。
pub fn hash(data: &[u8]) -> u32 {
    let mut h = Hasher::new();
    h.update(data);
    h.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 标准测试向量（CRC-32/ISO-HDLC）。这一条同时证明与 `crc32fast`
    /// 以及任何其它标准实现的结果一致。
    #[test]
    fn standard_check_vector() {
        assert_eq!(hash(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn known_values() {
        assert_eq!(hash(b""), 0x0000_0000);
        assert_eq!(hash(b"a"), 0xE8B7_BE43);
        assert_eq!(hash(b"abc"), 0x3524_41C2);
        // 全零 4 KiB 页（新分配页的典型内容）。
        // 该值由 Python `zlib.crc32` 独立算出，不是手写常量——手写常量正是
        // 本条测试第一次失败的原因。
        assert_eq!(hash(&[0u8; 4096]), 0xC71C_0011);
    }

    /// 增量式与一次性必须给出相同结果，否则分块写入的 WAL 帧会校验失败。
    #[test]
    fn incremental_matches_one_shot() {
        let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();

        let one_shot = hash(&data);

        let mut h = Hasher::new();
        h.update(&data[..300]);
        h.update(&data[300..700]);
        h.update(&data[700..]);
        assert_eq!(h.finalize(), one_shot, "chunked hashing must match");
    }

    /// 空更新不应改变状态。
    #[test]
    fn empty_update_is_identity() {
        let mut h = Hasher::new();
        h.update(b"hello");
        let after = h.finalize();

        let mut h2 = Hasher::new();
        h2.update(b"");
        h2.update(b"hello");
        h2.update(b"");
        assert_eq!(h2.finalize(), after);
    }

    /// 单比特翻转必须改变结果——这正是坏页检测依赖的性质。
    #[test]
    fn single_bit_flip_changes_result() {
        let a = [0x5Au8; 4096];
        let mut b = a;
        b[2048] ^= 0x01;
        assert_ne!(hash(&a), hash(&b));
    }
}

//! 显式二进制编解码原语：供**直接构成磁盘格式**的结构体使用。
//!
//! ## 为什么手写而不用第三方序列化库
//!
//! 本模块服务的字节**就是磁盘格式本身**（WAL 帧、Page 0 元数据）。把格式的
//! 字节布局委托给第三方 crate，等于把数据库的寿命绑定在别人的发布节奏上。
//!
//! 这不是假设的风险，而是刚刚发生的事：`bincode` 曾同时编码本项目的 WAL 帧与
//! Page 0 元数据，而它在 2025-12 停止维护（作者遭遇人肉骚扰后终止项目，
//! 3.0.0 版本里只有一个编译错误和一段墓志铭）。格式一旦冻结就没有第二次免费
//! 更换的机会，因此这里全部手写，字节布局在 `FORMAT.md` 中明文规定。
//!
//! 各类型自己实现编解码（见 `WalRecord`、`StringDict`、`IndexCatalog`），
//! 本模块只提供字节原语，不越界干涉它们的语义。
//!
//! ## 通用约定
//!
//! - 整数一律**小端**
//! - 变长数据的长度前缀统一 `u32`（4 字节），上限 [`MAX_BLOB_LEN`]
//! - 读取时任何越界都返回 `GraphError`，**绝不让损坏的输入引发 panic 或巨额分配**
//!
//! 最后一条是安全要求而非风格偏好：长度前缀来自磁盘，可能被损坏或被恶意构造。
//! 若不加校验就 `Vec::with_capacity(claimed_len)`，一个 4 字节的坏值就能让进程
//! 尝试分配 4 GiB。

use crate::graph::GraphError;

/// 单个变长字段的长度上限（1 GiB）。
///
/// 取值远超任何合法值（页载荷 4 KiB、字典与目录都是 KB 级），因此正常数据
/// 永远不会触及；它的作用是给损坏的长度前缀一个立即失败的边界。
pub const MAX_BLOB_LEN: usize = 1024 * 1024 * 1024;

/// 字节读取游标。任何越界读取都返回错误，不 panic。
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// 已消耗的字节数
    pub fn position(&self) -> usize {
        self.pos
    }

    /// 还未读取的字节数
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    /// 输入是否已完全读完（用于拒绝「尾部有垃圾」的记录）
    pub fn is_exhausted(&self) -> bool {
        self.pos == self.buf.len()
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], GraphError> {
        let end = self.pos.checked_add(n).ok_or_else(|| {
            GraphError::SerializationError("length overflow while decoding".into())
        })?;
        if end > self.buf.len() {
            return Err(GraphError::SerializationError(format!(
                "truncated input: need {} byte(s) at offset {}, only {} available",
                n,
                self.pos,
                self.buf.len()
            )));
        }
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    pub fn u8(&mut self) -> Result<u8, GraphError> {
        Ok(self.take(1)?[0])
    }

    pub fn u32(&mut self) -> Result<u32, GraphError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn u64(&mut self) -> Result<u64, GraphError> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    /// 读取带 `u32` 长度前缀的字节串。
    pub fn bytes(&mut self) -> Result<Vec<u8>, GraphError> {
        let len = self.u32()? as usize;
        if len > MAX_BLOB_LEN {
            return Err(GraphError::SerializationError(format!(
                "declared length {} exceeds the {} byte limit",
                len, MAX_BLOB_LEN
            )));
        }
        // 先确认缓冲区真的有这么多字节，再分配：否则一个损坏的长度前缀
        // 就能触发巨额分配。
        if len > self.remaining() {
            return Err(GraphError::SerializationError(format!(
                "declared length {} exceeds the {} remaining byte(s)",
                len,
                self.remaining()
            )));
        }
        Ok(self.take(len)?.to_vec())
    }

    /// 读取带 `u32` 长度前缀的 UTF-8 字符串。
    pub fn string(&mut self) -> Result<String, GraphError> {
        let raw = self.bytes()?;
        String::from_utf8(raw)
            .map_err(|e| GraphError::SerializationError(format!("invalid UTF-8: {}", e)))
    }
}

/// 字节写入器。
#[derive(Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// 预分配：调用方知道大致规模时避免多次扩容
    pub fn with_capacity(n: usize) -> Self {
        Self {
            buf: Vec::with_capacity(n),
        }
    }

    pub fn position(&self) -> usize {
        self.buf.len()
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// 写入带 `u32` 长度前缀的字节串。
    ///
    /// 超长直接截断为错误信号由调用方负责（本函数无法返回错误），因此这里
    /// 用断言式检查：合法载荷最大 4 KiB，永远不会接近 [`MAX_BLOB_LEN`]。
    pub fn bytes(&mut self, b: &[u8]) {
        debug_assert!(
            b.len() <= MAX_BLOB_LEN,
            "blob length {} exceeds the encodable limit",
            b.len()
        );
        self.u32(b.len() as u32);
        self.buf.extend_from_slice(b);
    }

    /// 写入带 `u32` 长度前缀的 UTF-8 字符串。
    pub fn string(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_all_primitives() {
        let mut w = Writer::new();
        w.u8(0xAB);
        w.u32(0xDEAD_BEEF);
        w.u64(0x0102_0304_0506_0708);
        w.bytes(&[1, 2, 3]);
        w.string("青云门");
        let buf = w.finish();

        let mut r = Reader::new(&buf);
        assert_eq!(r.u8().unwrap(), 0xAB);
        assert_eq!(r.u32().unwrap(), 0xDEAD_BEEF);
        assert_eq!(r.u64().unwrap(), 0x0102_0304_0506_0708);
        assert_eq!(r.bytes().unwrap(), vec![1, 2, 3]);
        assert_eq!(r.string().unwrap(), "青云门");
        assert!(r.is_exhausted());
    }

    #[test]
    fn empty_blob_round_trips() {
        let mut w = Writer::new();
        w.bytes(&[]);
        w.string("");
        let buf = w.finish();

        let mut r = Reader::new(&buf);
        assert_eq!(r.bytes().unwrap(), Vec::<u8>::new());
        assert_eq!(r.string().unwrap(), "");
    }

    #[test]
    fn truncated_input_errors_instead_of_panicking() {
        let mut w = Writer::new();
        w.u64(42);
        let buf = w.finish();

        // 只给 3 字节，u64 需要 8 字节
        let mut r = Reader::new(&buf[..3]);
        assert!(r.u64().is_err(), "truncated u64 must error");

        let mut r = Reader::new(&[]);
        assert!(r.u8().is_err(), "empty input must error");
    }

    /// 损坏的长度前缀不得触发巨额分配，必须立即失败。
    #[test]
    fn oversized_length_prefix_is_rejected() {
        let mut w = Writer::new();
        w.u32(u32::MAX); // 谎报 4 GiB
        w.u8(1); // 实际只有 1 字节
        let buf = w.finish();

        let mut r = Reader::new(&buf);
        let err = r.bytes().expect_err("oversized length must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("declared length"),
            "error must explain the length problem, got: {}",
            msg
        );
    }

    #[test]
    fn invalid_utf8_is_rejected() {
        let mut w = Writer::new();
        w.bytes(&[0xFF, 0xFE]); // 非法 UTF-8
        let buf = w.finish();

        let mut r = Reader::new(&buf);
        assert!(r.string().is_err(), "invalid UTF-8 must be rejected");
    }
}

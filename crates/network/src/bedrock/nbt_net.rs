//! Bedrock **网络变体** NBT（nbtx 里的 `VarintEndian`）的最小读取器。
//!
//! 为什么不直接用 `nbtx::from_bytes`：那条路会把目标类型交给 `facet` 反射
//! （`nbtx::nbt::de::read_root` 里 `Partial::alloc::<T>()` → `p.set(...)`），
//! 在 release 下会踩到 `facet_core::PtrMut::as_mut_byte_ptr` 的宽指针断言
//! （`facet-core-0.50.0-rc.7/src/types/ptr/mod.rs:182`）。这里按 nbtx
//! `nbt/io.rs` 的 `VarintEndian` 分支自己读，只用到 `nbtx::Value` 这批**数据类型**，
//! 不经过任何反射，因此与构建模式无关。背景见 BACKLOG.md BUG-032 第 46 条。
//!
//! 支持范围：只支持网络变体（长度前缀是 varint，`i32`/`i64` 是 zigzag varint，
//! 其余小端），够解析 Geyser 自带的 `data/biome_definitions.dat` 与
//! `data/entity_identifiers.dat`。磁盘上的小端/大端变体不在本模块范围内。
//!
//! 约束（见 BUG-032 第 46 条质量门）：无 `unsafe`、无 `unwrap`/`expect`/`panic!`、
//! 不做越界索引、长度前缀先校验再分配、必须整字节消费、错误带偏移。
#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fmt;

/// 嵌套深度上限。两张 `.dat` 实测最深不到 8 层，这里留足余量，
/// 防止损坏/恶意数据把栈打穿（栈溢出会直接 abort，连 panic 都抓不到）。
pub(crate) const MAX_DEPTH: usize = 64;

/// 解析错误：`pos` 是出错位置（读到的字节偏移），`what` 是期望/实际的说明。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NbtError {
    pub pos: usize,
    pub what: String,
}

impl NbtError {
    fn new(pos: usize, what: impl Into<String>) -> Self {
        NbtError {
            pos,
            what: what.into(),
        }
    }
}

impl fmt::Display for NbtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NBT 解析失败 @{}: {}", self.pos, self.what)
    }
}

/// 解析一张根为 `TAG_Compound` 的网络 NBT 文档，返回顶层键值表。
///
/// 顶层 key 用 `String::from_utf8_lossy` 转成 `String`（调用方
/// `BiomeDefinitionListPacket` / `AvailableActorIdentifiersPacket` 的字段类型就是
/// `HashMap<String, nbtx::Value>`）；里面的 key/value 保持 `bstr::BString` 原始字节。
///
/// 成功时不接受尾部残留字节（少读/多读都算解析失败，见质量门第 3 条）。
pub(crate) fn parse_root_compound(bytes: &[u8]) -> Result<HashMap<String, nbtx::Value>, NbtError> {
    let mut r = Reader::new(bytes);
    let tag = r.u8()?;
    if tag != 10 {
        return Err(NbtError::new(
            r.pos,
            format!("根 tag 必须是 Compound(10)，实际 {tag}"),
        ));
    }
    // 根名的内容对这两张表没有意义，但要读掉它（varint 长度 + 原始字节）。
    let _root_name = r.str_bytes()?;
    let compound = r.read_compound(0)?;
    if r.remaining() != 0 {
        return Err(NbtError::new(
            r.pos,
            format!("尾部残留 {} 字节", r.remaining()),
        ));
    }

    let mut out = HashMap::with_capacity(compound.len());
    for (key, value) in compound {
        let raw = key.to_vec();
        out.insert(String::from_utf8_lossy(&raw).into_owned(), value);
    }
    Ok(out)
}

/// 带位置的游标。所有读取都走 `get`/`try_into`，不做会 panic 的越界索引。
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    fn err(&self, what: impl Into<String>) -> NbtError {
        NbtError::new(self.pos, what)
    }

    /// 取出 `n` 个字节并前进。先校验再切，不信任 wire 里的长度。
    fn take(&mut self, n: usize) -> Result<&'a [u8], NbtError> {
        let end = match self.pos.checked_add(n) {
            Some(end) => end,
            None => return Err(self.err(format!("长度溢出: {n}"))),
        };
        match self.buf.get(self.pos..end) {
            Some(slice) => {
                self.pos = end;
                Ok(slice)
            }
            None => Err(self.err(format!(
                "需要 {n} 字节，只剩 {} 字节",
                self.remaining()
            ))),
        }
    }

    fn u8(&mut self) -> Result<u8, NbtError> {
        match self.buf.get(self.pos) {
            Some(byte) => {
                self.pos += 1;
                Ok(*byte)
            }
            None => Err(self.err("读到末尾，期望还有 1 字节")),
        }
    }

    fn i8(&mut self) -> Result<i8, NbtError> {
        Ok(self.u8()? as i8)
    }

    fn i16_le(&mut self) -> Result<i16, NbtError> {
        let bytes = self.take(2)?;
        let arr: [u8; 2] = match bytes.try_into() {
            Ok(arr) => arr,
            Err(_) => return Err(self.err("i16 长度不符")),
        };
        Ok(i16::from_le_bytes(arr))
    }

    fn f32_le(&mut self) -> Result<f32, NbtError> {
        let bytes = self.take(4)?;
        let arr: [u8; 4] = match bytes.try_into() {
            Ok(arr) => arr,
            Err(_) => return Err(self.err("f32 长度不符")),
        };
        Ok(f32::from_le_bytes(arr))
    }

    fn f64_le(&mut self) -> Result<f64, NbtError> {
        let bytes = self.take(8)?;
        let arr: [u8; 8] = match bytes.try_into() {
            Ok(arr) => arr,
            Err(_) => return Err(self.err("f64 长度不符")),
        };
        Ok(f64::from_le_bytes(arr))
    }

    /// 无符号 LEB128，最多 5 字节（32 位）。
    fn varu32(&mut self) -> Result<u32, NbtError> {
        let mut result = 0u32;
        let mut shift = 0u32;
        loop {
            let byte = self.u8()?;
            if shift == 28 && byte > 0x0f {
                return Err(self.err(format!("u32 varint 溢出（第 5 字节 {byte:#04x}）")));
            }
            result |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
            shift += 7;
            if shift > 28 {
                return Err(self.err("u32 varint 超过 5 字节"));
            }
        }
    }

    /// 有符号 zigzag varint，最多 5 字节（32 位）。
    fn vari32(&mut self) -> Result<i32, NbtError> {
        let raw = self.varu32()?;
        Ok(((raw >> 1) as i32) ^ -((raw & 1) as i32))
    }

    /// 有符号 zigzag varint，最多 10 字节（64 位）。
    fn vari64(&mut self) -> Result<i64, NbtError> {
        let mut result = 0u64;
        let mut shift = 0u32;
        loop {
            let byte = self.u8()?;
            if shift == 63 && byte > 0x01 {
                return Err(self.err(format!("i64 varint 溢出（第 10 字节 {byte:#04x}）")));
            }
            result |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 63 {
                return Err(self.err("i64 varint 超过 10 字节"));
            }
        }
        Ok(((result >> 1) as i64) ^ -((result & 1) as i64))
    }

    /// String 的原始字节（varint **字节**长度，不是字符数）。
    fn str_bytes(&mut self) -> Result<&'a [u8], NbtError> {
        let len = self.varu32()? as usize;
        self.take(len)
    }

    /// List/数组的长度前缀：zigzag varint，负数一律拒绝。
    fn seq_len(&mut self, what: &str) -> Result<usize, NbtError> {
        let raw = self.vari32()?;
        if raw < 0 {
            return Err(self.err(format!("{what} 长度为负: {raw}")));
        }
        Ok(raw as usize)
    }

    /// 每个元素至少占 1 字节，所以长度不可能超过剩余字节数。
    /// 先挡掉荒谬长度，避免为一个坏长度白跑几亿次循环。
    fn ensure_len_fits(&self, len: usize, what: &str) -> Result<(), NbtError> {
        if len > self.remaining() {
            return Err(self.err(format!(
                "{what} 长度 {len} 超过剩余 {} 字节",
                self.remaining()
            )));
        }
        Ok(())
    }

    /// 读一个 tag 的 payload（tag 字节已被调用方消费）。
    fn read_value(&mut self, tag: u8, depth: usize) -> Result<nbtx::Value, NbtError> {
        match tag {
            1 => Ok(nbtx::Value::Byte(self.i8()?)),
            2 => Ok(nbtx::Value::Short(self.i16_le()?)),
            3 => Ok(nbtx::Value::Int(self.vari32()?)),
            4 => Ok(nbtx::Value::Long(self.vari64()?)),
            5 => Ok(nbtx::Value::Float(self.f32_le()?)),
            6 => Ok(nbtx::Value::Double(self.f64_le()?)),
            7 => {
                let len = self.seq_len("ByteArray")?;
                self.ensure_len_fits(len, "ByteArray")?;
                Ok(nbtx::Value::ByteArray(self.take(len)?.to_vec()))
            }
            8 => {
                // 顶层 key 才用 String，Value 里保持原始字节（BString）。
                let raw = self.str_bytes()?;
                Ok(nbtx::Value::String(raw.to_vec().into()))
            }
            9 => Ok(nbtx::Value::List(self.read_list(depth)?)),
            10 => Ok(nbtx::Value::Compound(self.read_compound(depth)?)),
            11 => {
                let len = self.seq_len("IntArray")?;
                self.ensure_len_fits(len, "IntArray")?;
                let mut out = Vec::new();
                for _ in 0..len {
                    out.push(self.vari32()?);
                }
                Ok(nbtx::Value::IntArray(out))
            }
            12 => {
                let len = self.seq_len("LongArray")?;
                self.ensure_len_fits(len, "LongArray")?;
                let mut out = Vec::new();
                for _ in 0..len {
                    out.push(self.vari64()?);
                }
                Ok(nbtx::Value::LongArray(out))
            }
            other => Err(self.err(format!("不认识的 tag: {other}"))),
        }
    }

    /// 读 List 的 payload（元素 tag 字节 + 长度 + 元素），保持元素 tag 原样。
    fn read_list(&mut self, depth: usize) -> Result<nbtx::ValueList, NbtError> {
        if depth > MAX_DEPTH {
            return Err(self.err(format!("嵌套超过 {MAX_DEPTH} 层")));
        }
        let elem = self.u8()?;
        let len = self.seq_len("List")?;
        if elem == 0 {
            if len != 0 {
                return Err(self.err(format!("TAG_End 列表长度必须为 0，实际 {len}")));
            }
            return Ok(nbtx::ValueList::End);
        }
        if elem > 12 {
            return Err(self.err(format!("列表元素 tag 非法: {elem}")));
        }
        self.ensure_len_fits(len, "List")?;

        // 递归元素类型单独处理，避免把 `&mut self` 借进闭包。
        if elem == 9 {
            let mut out = Vec::new();
            for _ in 0..len {
                out.push(self.read_list(depth + 1)?);
            }
            return Ok(nbtx::ValueList::List(out));
        }
        if elem == 10 {
            let mut out = Vec::new();
            for _ in 0..len {
                out.push(self.read_compound(depth + 1)?);
            }
            return Ok(nbtx::ValueList::Compound(out));
        }

        Ok(match elem {
            1 => {
                let mut out = Vec::new();
                for _ in 0..len {
                    out.push(self.i8()?);
                }
                nbtx::ValueList::Byte(out)
            }
            2 => {
                let mut out = Vec::new();
                for _ in 0..len {
                    out.push(self.i16_le()?);
                }
                nbtx::ValueList::Short(out)
            }
            3 => {
                let mut out = Vec::new();
                for _ in 0..len {
                    out.push(self.vari32()?);
                }
                nbtx::ValueList::Int(out)
            }
            4 => {
                let mut out = Vec::new();
                for _ in 0..len {
                    out.push(self.vari64()?);
                }
                nbtx::ValueList::Long(out)
            }
            5 => {
                let mut out = Vec::new();
                for _ in 0..len {
                    out.push(self.f32_le()?);
                }
                nbtx::ValueList::Float(out)
            }
            6 => {
                let mut out = Vec::new();
                for _ in 0..len {
                    out.push(self.f64_le()?);
                }
                nbtx::ValueList::Double(out)
            }
            7 => {
                let mut out = Vec::new();
                for _ in 0..len {
                    let n = self.seq_len("ByteArray")?;
                    self.ensure_len_fits(n, "ByteArray")?;
                    out.push(self.take(n)?.to_vec());
                }
                nbtx::ValueList::ByteArray(out)
            }
            8 => {
                let mut out = Vec::new();
                for _ in 0..len {
                    let raw = self.str_bytes()?;
                    out.push(raw.to_vec().into());
                }
                nbtx::ValueList::String(out)
            }
            11 => {
                let mut out = Vec::new();
                for _ in 0..len {
                    let n = self.seq_len("IntArray")?;
                    self.ensure_len_fits(n, "IntArray")?;
                    let mut ints = Vec::new();
                    for _ in 0..n {
                        ints.push(self.vari32()?);
                    }
                    out.push(ints);
                }
                nbtx::ValueList::IntArray(out)
            }
            12 => {
                let mut out = Vec::new();
                for _ in 0..len {
                    let n = self.seq_len("LongArray")?;
                    self.ensure_len_fits(n, "LongArray")?;
                    let mut longs = Vec::new();
                    for _ in 0..n {
                        longs.push(self.vari64()?);
                    }
                    out.push(longs);
                }
                nbtx::ValueList::LongArray(out)
            }
            other => return Err(self.err(format!("列表元素 tag 非法: {other}"))),
        })
    }

    /// 读 Compound 的 payload（`<tag><key><value>` 序列，`0x00` 结束）。
    fn read_compound(&mut self, depth: usize) -> Result<nbtx::Compound, NbtError> {
        if depth > MAX_DEPTH {
            return Err(self.err(format!("嵌套超过 {MAX_DEPTH} 层")));
        }
        let mut map = nbtx::Compound::default();
        loop {
            let tag = self.u8()?;
            if tag == 0 {
                return Ok(map);
            }
            if tag > 12 {
                return Err(self.err(format!("不认识的 tag: {tag}")));
            }
            let key = self.str_bytes()?.to_vec();
            let value = self.read_value(tag, depth + 1)?;
            map.insert(key.into(), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_root_compound, MAX_DEPTH};
    use nbtx::Value;

    fn varu32(out: &mut Vec<u8>, mut value: u32) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }

    fn varu64(out: &mut Vec<u8>, mut value: u64) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }

    fn vari32(out: &mut Vec<u8>, value: i32) {
        let zigzag = ((value as u32) << 1) ^ ((value >> 31) as u32);
        varu32(out, zigzag);
    }

    fn vari64(out: &mut Vec<u8>, value: i64) {
        let zigzag = ((value as u64) << 1) ^ ((value >> 63) as u64);
        varu64(out, zigzag);
    }

    fn put_string(out: &mut Vec<u8>, text: &str) {
        varu32(out, text.len() as u32);
        out.extend_from_slice(text.as_bytes());
    }

    /// 根：tag 10 + 根名（空）
    fn root() -> Vec<u8> {
        let mut out = vec![10u8];
        put_string(&mut out, "");
        out
    }

    #[test]
    fn decodes_all_value_types() {
        let mut buf = root();
        // i_neg = -1
        buf.push(3);
        put_string(&mut buf, "i_neg");
        vari32(&mut buf, -1);
        // i_pos = 300
        buf.push(3);
        put_string(&mut buf, "i_pos");
        vari32(&mut buf, 300);
        // l_neg
        buf.push(4);
        put_string(&mut buf, "l_neg");
        vari64(&mut buf, -1_234_567_890_123);
        // l_min
        buf.push(4);
        put_string(&mut buf, "l_min");
        vari64(&mut buf, i64::MIN);
        // short = -2（小端定长）
        buf.push(2);
        put_string(&mut buf, "short");
        buf.extend_from_slice(&(-2i16).to_le_bytes());
        // float = 1.5
        buf.push(5);
        put_string(&mut buf, "float");
        buf.extend_from_slice(&1.5f32.to_le_bytes());
        // double = -2.25
        buf.push(6);
        put_string(&mut buf, "double");
        buf.extend_from_slice(&(-2.25f64).to_le_bytes());
        // 多字节 UTF-8 key + value（长度按字节）
        buf.push(8);
        put_string(&mut buf, "生物");
        put_string(&mut buf, "河流");
        // 空列表（End）
        buf.push(9);
        put_string(&mut buf, "empty");
        buf.push(0);
        vari32(&mut buf, 0);
        // 嵌套 Compound
        buf.push(10);
        put_string(&mut buf, "nested");
        buf.push(3);
        put_string(&mut buf, "x");
        vari32(&mut buf, 7);
        buf.push(0);
        // ByteArray [1,2,3]
        buf.push(7);
        put_string(&mut buf, "bytes");
        vari32(&mut buf, 3);
        buf.extend_from_slice(&[1, 2, 3]);
        // IntArray [-1, 2]
        buf.push(11);
        put_string(&mut buf, "ints");
        vari32(&mut buf, 2);
        vari32(&mut buf, -1);
        vari32(&mut buf, 2);
        // LongArray [5]
        buf.push(12);
        put_string(&mut buf, "longs");
        vari32(&mut buf, 1);
        vari64(&mut buf, 5);
        // List<Int> [1,2,3]
        buf.push(9);
        put_string(&mut buf, "list_i");
        buf.push(3);
        vari32(&mut buf, 3);
        vari32(&mut buf, 1);
        vari32(&mut buf, 2);
        vari32(&mut buf, 3);
        // List<String> ["a","bc"]
        buf.push(9);
        put_string(&mut buf, "list_s");
        buf.push(8);
        vari32(&mut buf, 2);
        put_string(&mut buf, "a");
        put_string(&mut buf, "bc");
        // 结束根 Compound
        buf.push(0);

        let map = parse_root_compound(&buf).expect("应当解析成功");
        assert_eq!(map.get("i_neg"), Some(&Value::Int(-1)));
        assert_eq!(map.get("i_pos"), Some(&Value::Int(300)));
        assert_eq!(map.get("l_neg"), Some(&Value::Long(-1_234_567_890_123)));
        assert_eq!(map.get("l_min"), Some(&Value::Long(i64::MIN)));
        assert_eq!(map.get("short"), Some(&Value::Short(-2)));
        assert_eq!(map.get("float"), Some(&Value::Float(1.5)));
        assert_eq!(map.get("double"), Some(&Value::Double(-2.25)));
        assert_eq!(
            map.get("生物"),
            Some(&Value::String(String::from("河流").into()))
        );
        assert_eq!(
            map.get("empty"),
            Some(&Value::List(nbtx::ValueList::End)),
            "空列表必须是 ValueList::End"
        );
        match map.get("nested") {
            Some(Value::Compound(inner)) => {
                assert_eq!(inner.get(&b"x"[..]), Some(&Value::Int(7)))
            }
            other => panic!("nested 类型不对: {other:?}"),
        }
        assert_eq!(
            map.get("bytes"),
            Some(&Value::ByteArray(vec![1, 2, 3]))
        );
        assert_eq!(map.get("ints"), Some(&Value::IntArray(vec![-1, 2])));
        assert_eq!(map.get("longs"), Some(&Value::LongArray(vec![5])));
        assert_eq!(
            map.get("list_i"),
            Some(&Value::List(nbtx::ValueList::Int(vec![1, 2, 3])))
        );
        assert_eq!(
            map.get("list_s"),
            Some(&Value::List(nbtx::ValueList::String(vec![
                b"a".to_vec().into(),
                b"bc".to_vec().into()
            ])))
        );
    }

    /// 根 tag 不是 Compound 必须报错，不能默默返回空表。
    #[test]
    fn rejects_non_compound_root() {
        let mut buf = vec![1u8];
        put_string(&mut buf, "");
        buf.push(0);
        let err = parse_root_compound(&buf).expect_err("根 tag 为 Byte 时应当报错");
        assert!(err.what.contains("根 tag"), "错误信息: {err}");
    }

    /// 尾部残留字节必须报错（少读/多读都算失败）。
    #[test]
    fn rejects_trailing_bytes() {
        let mut buf = root();
        buf.push(0);
        buf.push(0xff);
        let err = parse_root_compound(&buf).expect_err("尾部残留应当报错");
        assert!(err.what.contains("尾部残留"), "错误信息: {err}");
    }

    /// 截断的字符串（长度前缀大于剩余字节）必须报错，且不能越界。
    #[test]
    fn rejects_truncated_string() {
        let mut buf = root();
        buf.push(8);
        put_string(&mut buf, "k");
        varu32(&mut buf, 10); // 声称 10 字节，实际一个都没给
        let err = parse_root_compound(&buf).expect_err("截断字符串应当报错");
        assert!(err.what.contains("字节"), "错误信息: {err}");
    }

    /// 未知 tag 必须报错。
    #[test]
    fn rejects_unknown_tag() {
        let mut buf = root();
        buf.push(13);
        put_string(&mut buf, "k");
        let err = parse_root_compound(&buf).expect_err("未知 tag 应当报错");
        assert!(err.what.contains("tag"), "错误信息: {err}");
    }

    /// 列表长度为负必须报错。
    #[test]
    fn rejects_negative_list_len() {
        let mut buf = root();
        buf.push(9);
        put_string(&mut buf, "k");
        buf.push(1);
        vari32(&mut buf, -1);
        let err = parse_root_compound(&buf).expect_err("负长度应当报错");
        assert!(err.what.contains("负"), "错误信息: {err}");
    }

    /// TAG_End 列表长度非 0 必须报错（pmmp/gophertunnel 同样拒绝）。
    #[test]
    fn rejects_nonempty_end_list() {
        let mut buf = root();
        buf.push(9);
        put_string(&mut buf, "k");
        buf.push(0);
        vari32(&mut buf, 1);
        let err = parse_root_compound(&buf).expect_err("非空 End 列表应当报错");
        assert!(err.what.contains("TAG_End"), "错误信息: {err}");
    }

    /// 深度上限：超过 MAX_DEPTH 层必须报错，不能把栈打穿。
    #[test]
    fn rejects_too_deep_nesting() {
        let levels = MAX_DEPTH + 5;
        let mut buf = root();
        for level in 0..levels {
            buf.push(10);
            put_string(&mut buf, &format!("k{level}"));
        }
        for _ in 0..=levels {
            buf.push(0);
        }
        let err = parse_root_compound(&buf).expect_err("超深嵌套应当报错");
        assert!(err.what.contains("嵌套"), "错误信息: {err}");
    }
}

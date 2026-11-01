//! Bedrock **网络变体** NBT（`nbtx::VarintEndian`）的读写器，以及不依赖 facet 的
//! [`NbtCompound`] 包装类型。
//!
//! 背景（BUG-032 第 46 条）：nbtx 的 `from_varint_bytes` / `to_varint_bytes_in` 会把
//! 目标类型交给 `facet` 反射，`facet-core 0.50.0-rc.7` 把"宽/细指针"标记压在指针第 63 位。
//! Android aarch64 的堆指针高字节本身就是 tag（`0x34...`），于是要么断言失败，
//! 要么 free 时 Scudo 报 "Pointer tag ... was truncated" 直接 SIGABRT。
//! x86_64 上高字节恒 0，所以这个 bug 在 PC 上永远复现不了。
//!
//! 因此：**运行路径上不允许出现 facet**。本模块按 nbtx `nbt/io.rs` 的 `VarintEndian`
//! 分支自己读、自己写，只把 `nbtx::Value` 当纯数据结构用。
//!
//! 编码要点（与 `VarintEndian` 逐条对齐）：
//! - 根：`tag(1B)` + 根名（**varu32** 字节长度 + 原始字节）+ payload；根 tag 必须是 10。
//! - Compound：`<tag><key><value>` 序列，`0x00` 结束；key 长度是 **varu32**。
//! - String / key：长度 **varu32**（字节数，不是字符数）。
//! - ByteArray / List / IntArray / LongArray 的长度：**zigzag varint i32**。
//! - `i32` / `i64`：zigzag varint；`i16` / `f32` / `f64`：小端定长。
#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fmt;
use std::io::{Read, Write};

use bedrock_protocol_core::ProtoCodec;

/// 嵌套深度上限。两张 `.dat` 实测不足 8 层，留足余量防止损坏数据打穿栈。
pub const MAX_DEPTH: usize = 64;

/// 单个字符串/数组/列表允许的最大长度（字节或元素数）。
///
/// 流式解码拿不到"剩余字节数"，只能给一个绝对上限挡住荒谬长度，
/// 避免为一个坏长度直接分配几 GB。
const MAX_ALLOC: usize = 16 * 1024 * 1024;

/// 解析错误：`pos` 是出错位置（已消费字节数），`what` 是期望/实际的说明。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NbtError {
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

impl std::error::Error for NbtError {}

/// 不经过 facet 的 NBT 复合标签包装类型。
///
/// 用法与 `HashMap<String, nbtx::Value>` 一致（实现了 `Deref`），但 `ProtoCodec`
/// 是手写的：序列化/反序列化直接走本模块的 writer/reader。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NbtCompound(pub HashMap<String, nbtx::Value>);

impl NbtCompound {
    pub fn new(map: HashMap<String, nbtx::Value>) -> Self {
        NbtCompound(map)
    }

    pub fn into_inner(self) -> HashMap<String, nbtx::Value> {
        self.0
    }

    pub fn as_map(&self) -> &HashMap<String, nbtx::Value> {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl From<HashMap<String, nbtx::Value>> for NbtCompound {
    fn from(map: HashMap<String, nbtx::Value>) -> Self {
        NbtCompound(map)
    }
}

impl std::ops::Deref for NbtCompound {
    type Target = HashMap<String, nbtx::Value>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ProtoCodec for NbtCompound {
    fn serialize<W: Write>(
        &self,
        stream: &mut W,
    ) -> Result<(), bedrock_protocol_core::error::ProtoCodecError> {
        write_compound_stream(&self.0, stream).map_err(to_codec_error)?;
        Ok(())
    }

    fn deserialize<R: Read>(
        stream: &mut R,
    ) -> Result<Self, bedrock_protocol_core::error::ProtoCodecError> {
        read_compound_stream(stream).map(NbtCompound).map_err(to_codec_error)
    }

    fn size_hint(&self) -> usize {
        // ProtoCodec::size_hint 不能返回错误，这里的写操作只会写进 Sink，不会失败。
        write_compound_stream(&self.0, &mut Sink).unwrap_or(0)
    }
}

fn to_codec_error(err: NbtError) -> bedrock_protocol_core::error::ProtoCodecError {
    bedrock_protocol_core::error::ProtoCodecError::IOError(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        err.to_string(),
    ))
}

/// 只数长度、丢弃内容的 writer，供 `size_hint` 用。
struct Sink;

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// ───────────────────────────── 读 ─────────────────────────────

/// 读一整张根为 `TAG_Compound` 的网络 NBT 文档，**要求整字节消费**。
///
/// 适合"整段字节就是一份 NBT"的场景（比如 Geyser 的两张 `.dat`）；
/// 字段嵌在别的数据中间时用 [`read_compound_stream`]。
pub fn read_root_compound(
    bytes: &[u8],
) -> Result<HashMap<String, nbtx::Value>, NbtError> {
    let mut cursor = std::io::Cursor::new(bytes);
    let map = read_compound_stream(&mut cursor)?;
    let consumed = cursor.position() as usize;
    if consumed != bytes.len() {
        return Err(NbtError::new(
            consumed,
            format!("尾部残留 {} 字节", bytes.len() - consumed),
        ));
    }
    Ok(map)
}

/// 从流里读一个网络 NBT 复合标签（根 tag + 根名 + 内容），读到 `TAG_End` 停。
///
/// **边界由 NBT 结构本身决定**，不依赖外层长度、也不读到 EOF，
/// 所以这个字段后面还可以跟别的字段。
pub fn read_compound_stream<R: Read>(
    stream: &mut R,
) -> Result<HashMap<String, nbtx::Value>, NbtError> {
    let mut r = Reader {
        inner: stream,
        pos: 0,
    };
    let tag = r.u8()?;
    if tag != 10 {
        return Err(NbtError::new(
            r.pos,
            format!("根 tag 必须是 Compound(10)，实际 {tag}"),
        ));
    }
    let _root_name = r.str_bytes()?;
    let compound = r.read_compound(0)?;
    let mut out = HashMap::with_capacity(compound.len());
    for (key, value) in compound {
        let raw = key.to_vec();
        out.insert(String::from_utf8_lossy(&raw).into_owned(), value);
    }
    Ok(out)
}

struct Reader<'a, R: Read> {
    inner: &'a mut R,
    pos: usize,
}

impl<R: Read> Reader<'_, R> {
    fn err(&self, what: impl Into<String>) -> NbtError {
        NbtError::new(self.pos, what)
    }

    fn take(&mut self, n: usize) -> Result<Vec<u8>, NbtError> {
        if n > MAX_ALLOC {
            return Err(self.err(format!("长度 {n} 超过上限 {MAX_ALLOC}")));
        }
        let mut buf = vec![0u8; n];
        match self.inner.read_exact(&mut buf) {
            Ok(()) => {
                self.pos += n;
                Ok(buf)
            }
            Err(e) => Err(self.err(format!("读取 {n} 字节失败: {e}"))),
        }
    }

    fn u8(&mut self) -> Result<u8, NbtError> {
        let mut byte = [0u8; 1];
        match self.inner.read_exact(&mut byte) {
            Ok(()) => {
                self.pos += 1;
                Ok(byte[0])
            }
            Err(e) => Err(self.err(format!("读到末尾，期望还有 1 字节: {e}"))),
        }
    }

    fn i8(&mut self) -> Result<i8, NbtError> {
        Ok(self.u8()? as i8)
    }

    fn fixed<const N: usize>(&mut self, what: &str) -> Result<[u8; N], NbtError> {
        match self.take(N)?.try_into() {
            Ok(arr) => Ok(arr),
            Err(_) => Err(self.err(format!("{what} 长度不符"))),
        }
    }

    fn i16_le(&mut self) -> Result<i16, NbtError> {
        Ok(i16::from_le_bytes(self.fixed::<2>("i16")?))
    }

    fn f32_le(&mut self) -> Result<f32, NbtError> {
        Ok(f32::from_le_bytes(self.fixed::<4>("f32")?))
    }

    fn f64_le(&mut self) -> Result<f64, NbtError> {
        Ok(f64::from_le_bytes(self.fixed::<8>("f64")?))
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

    fn vari32(&mut self) -> Result<i32, NbtError> {
        let raw = self.varu32()?;
        Ok(((raw >> 1) as i32) ^ -((raw & 1) as i32))
    }

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

    fn str_bytes(&mut self) -> Result<Vec<u8>, NbtError> {
        let len = self.varu32()? as usize;
        self.take(len)
    }

    fn seq_len(&mut self, what: &str) -> Result<usize, NbtError> {
        let raw = self.vari32()?;
        if raw < 0 {
            return Err(self.err(format!("{what} 长度为负: {raw}")));
        }
        let len = raw as usize;
        if len > MAX_ALLOC {
            return Err(self.err(format!("{what} 长度 {len} 超过上限 {MAX_ALLOC}")));
        }
        Ok(len)
    }

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
                Ok(nbtx::Value::ByteArray(self.take(len)?))
            }
            8 => {
                let raw = self.str_bytes()?;
                Ok(nbtx::Value::String(raw.into()))
            }
            9 => Ok(nbtx::Value::List(self.read_list(depth)?)),
            10 => Ok(nbtx::Value::Compound(self.read_compound(depth)?)),
            11 => {
                let len = self.seq_len("IntArray")?;
                let mut out = Vec::new();
                for _ in 0..len {
                    out.push(self.vari32()?);
                }
                Ok(nbtx::Value::IntArray(out))
            }
            12 => {
                let len = self.seq_len("LongArray")?;
                let mut out = Vec::new();
                for _ in 0..len {
                    out.push(self.vari64()?);
                }
                Ok(nbtx::Value::LongArray(out))
            }
            other => Err(self.err(format!("不认识的 tag: {other}"))),
        }
    }

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
                    out.push(self.take(n)?);
                }
                nbtx::ValueList::ByteArray(out)
            }
            8 => {
                let mut out = Vec::new();
                for _ in 0..len {
                    let raw = self.str_bytes()?;
                    out.push(raw.into());
                }
                nbtx::ValueList::String(out)
            }
            11 => {
                let mut out = Vec::new();
                for _ in 0..len {
                    let n = self.seq_len("IntArray")?;
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
            let key = self.str_bytes()?;
            let value = self.read_value(tag, depth + 1)?;
            map.insert(key.into(), value);
        }
    }
}

// ───────────────────────────── 写 ─────────────────────────────

/// 把一张复合标签写成完整的网络 NBT 字节（含根 tag 与空根名）。
pub fn write_compound_bytes(map: &HashMap<String, nbtx::Value>) -> Result<Vec<u8>, NbtError> {
    let mut out = Vec::new();
    write_compound_stream(map, &mut out)?;
    Ok(out)
}

/// 把一张复合标签写进流（含根 tag 与空根名），返回写出的字节数。
///
/// 字节布局与 `nbtx::to_varint_bytes_in` 的 `VarintEndian` 输出一致：
/// `0x0A` + 根名长度 `0x00` + 内容 + `0x00` 结尾。
pub fn write_compound_stream<W: Write>(
    map: &HashMap<String, nbtx::Value>,
    stream: &mut W,
) -> Result<usize, NbtError> {
    let mut w = Counter {
        inner: stream,
        written: 0,
    };
    w.write_all(&[10u8, 0u8]).map_err(io_err)?;
    write_compound_body(&mut w, map)?;
    Ok(w.written)
}

/// 写复合标签的**内容**（不含根 tag/根名），以 `0x00` 结尾。
pub fn write_compound_body<W: Write>(
    stream: &mut W,
    map: &HashMap<String, nbtx::Value>,
) -> Result<(), NbtError> {
    for (key, value) in map {
        write_entry(stream, key.as_bytes(), value)?;
    }
    stream.write_all(&[0u8]).map_err(io_err)?;
    Ok(())
}

/// 写 `nbtx::Compound`（`IndexMap<BString, Value>`）的内容，以 `0x00` 结尾。
fn write_compound_indexed<W: Write>(
    stream: &mut W,
    map: &nbtx::Compound,
) -> Result<(), NbtError> {
    for (key, value) in map {
        let raw: &[u8] = key.as_ref();
        write_entry(stream, raw, value)?;
    }
    stream.write_all(&[0u8]).map_err(io_err)?;
    Ok(())
}

fn write_entry<W: Write>(stream: &mut W, key: &[u8], value: &nbtx::Value) -> Result<(), NbtError> {
    stream.write_all(&[value.discriminant()]).map_err(io_err)?;
    write_str(stream, key)?;
    write_value(stream, value)
}

fn write_value<W: Write>(stream: &mut W, value: &nbtx::Value) -> Result<(), NbtError> {
    match value {
        nbtx::Value::Byte(v) => stream.write_all(&[*v as u8]).map_err(io_err)?,
        nbtx::Value::Short(v) => stream.write_all(&v.to_le_bytes()).map_err(io_err)?,
        nbtx::Value::Int(v) => write_vari32(stream, *v)?,
        nbtx::Value::Long(v) => write_vari64(stream, *v)?,
        nbtx::Value::Float(v) => stream.write_all(&v.to_le_bytes()).map_err(io_err)?,
        nbtx::Value::Double(v) => stream.write_all(&v.to_le_bytes()).map_err(io_err)?,
        nbtx::Value::ByteArray(bytes) => {
            write_seq_len(stream, bytes.len(), "ByteArray")?;
            stream.write_all(bytes).map_err(io_err)?;
        }
        nbtx::Value::String(text) => {
            let raw: &[u8] = text.as_ref();
            write_str(stream, raw)?;
        }
        nbtx::Value::List(list) => write_list(stream, list)?,
        nbtx::Value::Compound(map) => write_compound_indexed(stream, map)?,
        nbtx::Value::IntArray(ints) => {
            write_seq_len(stream, ints.len(), "IntArray")?;
            for v in ints {
                write_vari32(stream, *v)?;
            }
        }
        nbtx::Value::LongArray(longs) => {
            write_seq_len(stream, longs.len(), "LongArray")?;
            for v in longs {
                write_vari64(stream, *v)?;
            }
        }
    }
    Ok(())
}

/// 列表的元素 tag 字节（`ValueList::End` → 0）。
fn list_tag(list: &nbtx::ValueList) -> u8 {
    match list {
        nbtx::ValueList::End => 0,
        nbtx::ValueList::Byte(_) => 1,
        nbtx::ValueList::Short(_) => 2,
        nbtx::ValueList::Int(_) => 3,
        nbtx::ValueList::Long(_) => 4,
        nbtx::ValueList::Float(_) => 5,
        nbtx::ValueList::Double(_) => 6,
        nbtx::ValueList::ByteArray(_) => 7,
        nbtx::ValueList::String(_) => 8,
        nbtx::ValueList::List(_) => 9,
        nbtx::ValueList::Compound(_) => 10,
        nbtx::ValueList::IntArray(_) => 11,
        nbtx::ValueList::LongArray(_) => 12,
    }
}

fn write_list<W: Write>(stream: &mut W, list: &nbtx::ValueList) -> Result<(), NbtError> {
    stream.write_all(&[list_tag(list)]).map_err(io_err)?;
    write_seq_len(stream, list.len(), "List")?;
    match list {
        nbtx::ValueList::End => {}
        nbtx::ValueList::Byte(items) => {
            for v in items {
                stream.write_all(&[*v as u8]).map_err(io_err)?;
            }
        }
        nbtx::ValueList::Short(items) => {
            for v in items {
                stream.write_all(&v.to_le_bytes()).map_err(io_err)?;
            }
        }
        nbtx::ValueList::Int(items) => {
            for v in items {
                write_vari32(stream, *v)?;
            }
        }
        nbtx::ValueList::Long(items) => {
            for v in items {
                write_vari64(stream, *v)?;
            }
        }
        nbtx::ValueList::Float(items) => {
            for v in items {
                stream.write_all(&v.to_le_bytes()).map_err(io_err)?;
            }
        }
        nbtx::ValueList::Double(items) => {
            for v in items {
                stream.write_all(&v.to_le_bytes()).map_err(io_err)?;
            }
        }
        nbtx::ValueList::ByteArray(items) => {
            for bytes in items {
                write_seq_len(stream, bytes.len(), "ByteArray")?;
                stream.write_all(bytes).map_err(io_err)?;
            }
        }
        nbtx::ValueList::String(items) => {
            for text in items {
                let raw: &[u8] = text.as_ref();
                write_str(stream, raw)?;
            }
        }
        nbtx::ValueList::List(items) => {
            for inner in items {
                write_list(stream, inner)?;
            }
        }
        nbtx::ValueList::Compound(items) => {
            for inner in items {
                write_compound_indexed(stream, inner)?;
            }
        }
        nbtx::ValueList::IntArray(items) => {
            for ints in items {
                write_seq_len(stream, ints.len(), "IntArray")?;
                for v in ints {
                    write_vari32(stream, *v)?;
                }
            }
        }
        nbtx::ValueList::LongArray(items) => {
            for longs in items {
                write_seq_len(stream, longs.len(), "LongArray")?;
                for v in longs {
                    write_vari64(stream, *v)?;
                }
            }
        }
    }
    Ok(())
}

/// String 的长度前缀：**varu32**（字节数）。
fn write_str<W: Write>(stream: &mut W, raw: &[u8]) -> Result<(), NbtError> {
    if raw.len() > MAX_ALLOC {
        return Err(NbtError::new(0, format!("字符串长度 {} 超过上限", raw.len())));
    }
    write_varu32(stream, raw.len() as u32)?;
    stream.write_all(raw).map_err(io_err)
}

/// 数组/列表的长度前缀：**zigzag varint i32**（与 nbtx `read_seq_len` 一致）。
fn write_seq_len<W: Write>(stream: &mut W, len: usize, what: &str) -> Result<(), NbtError> {
    let len = i32::try_from(len)
        .map_err(|_| NbtError::new(0, format!("{what} 长度 {len} 超出 i32")))?;
    write_vari32(stream, len)
}

fn write_varu32<W: Write>(stream: &mut W, mut value: u32) -> Result<(), NbtError> {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            stream.write_all(&[byte]).map_err(io_err)?;
            return Ok(());
        }
        stream.write_all(&[byte | 0x80]).map_err(io_err)?;
    }
}

fn write_vari32<W: Write>(stream: &mut W, value: i32) -> Result<(), NbtError> {
    let zigzag = ((value as u32) << 1) ^ ((value >> 31) as u32);
    write_varu32(stream, zigzag)
}

fn write_vari64<W: Write>(stream: &mut W, value: i64) -> Result<(), NbtError> {
    let zigzag = ((value as u64) << 1) ^ ((value >> 63) as u64);
    let mut value = zigzag;
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            stream.write_all(&[byte]).map_err(io_err)?;
            return Ok(());
        }
        stream.write_all(&[byte | 0x80]).map_err(io_err)?;
    }
}

fn io_err(e: std::io::Error) -> NbtError {
    NbtError::new(0, format!("写入失败: {e}"))
}

/// 带计数的 writer（用于 `write_compound_stream` 的返回值）。
struct Counter<'a, W: Write> {
    inner: &'a mut W,
    written: usize,
}

impl<W: Write> Write for Counter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.written += n;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

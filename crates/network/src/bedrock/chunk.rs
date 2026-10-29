//! 手写 Bedrock 区块负载编码（1.21.50）。
//!
//! 两套调色板写法都实现，便于隔离问题：
//! * `Mode::Runtime`：层头 (bits<<1)|1，条目是 zigzag varint 的 block state id
//! * `Mode::Persistent`：层头 (bits<<1)|0，条目是「varint 长度 + NBT（littleVarint 字符串）」
//!
//! 其余结构与 PMMP ChunkSerializer 一致：子区块 v8 堆叠 → 全部段生物群系 →
//! 边界方块计数(varint 0) → 方块实体字节数组(varint 长度 0)。

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Runtime,
    Persistent,
}

fn varint(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn zigzag(out: &mut Vec<u8>, v: i32) {
    varint(out, ((v << 1) ^ (v >> 31)) as u32);
}

/// 一个方块状态的 NBT（littleVarint 字符串长度）。
fn block_nbt(name: &str, version: i32) -> Vec<u8> {
    fn head(out: &mut Vec<u8>, tag: u8, key: &str) {
        out.push(tag);
        varint(out, key.len() as u32);
        out.extend(key.as_bytes());
    }
    let mut out = Vec::new();
    out.push(10); // TAG_Compound（根）
    varint(&mut out, 0);
    head(&mut out, 8, "name"); // TAG_String
    varint(&mut out, name.len() as u32);
    out.extend(name.as_bytes());
    head(&mut out, 10, "states"); // 空 TAG_Compound
    out.push(0);
    head(&mut out, 3, "version"); // TAG_Int
    out.extend(version.to_le_bytes());
    out.push(0); // 根结束
    out
}

/// 整段同一种方块的子区块（版本 9，带 subchunk index，见 GeyserChunkSection）。
/// 参考实现：Geyser core/level/chunk/GeyserChunkSection.writeToNetwork
///   版本字节 9 → 层数 → subChunkIndex → 每层 BlockStorage
/// subChunkIndex = 段在 y 轴上的编号（y/16），负数按补码写（-4 → 0xFC）。
pub fn uniform_subchunk(mode: Mode, state_id: i32, name: &str, version: i32, index: i8) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(9); // 子区块版本 9（1.19.80 以后）
    out.push(1); // 层数
    out.push(index as u8); // subchunk index：y/16，负数按补码（-4 → 0xFC）
    match mode {
        Mode::Runtime => {
            out.push(1); // (bits=0)<<1 | 1
            zigzag(&mut out, state_id);
        }
        Mode::Persistent => {
            out.push(0); // (bits=0)<<1 | 0
            let nbt = block_nbt(name, version);
            varint(&mut out, nbt.len() as u32);
            out.extend(nbt);
        }
    }
    out
}

pub struct ChunkSpec {
    pub min_section: i8,
    pub max_section: i8,
    pub highest_non_empty: i8,
    pub floor_section: i8,
    pub stone_state: i32,
    pub air_state: i32,
    pub biome_id: i32,
    pub block_version: i32,
    pub mode: Mode,
}

/// 多值调色板子区块（与 Geyser `BlockStorage.writeToNetwork` 一致）：
/// 版本 9 → 层数 → subchunk index → 层头 → words（位打包，LE）→ 调色板长度 → 调色板项
pub fn paletted_subchunk(index: i8, blocks: &[i32; 4096]) -> Vec<u8> {
    let mut palette: Vec<i32> = Vec::new();
    let mut ids = [0u16; 4096];
    for (i, &b) in blocks.iter().enumerate() {
        match palette.iter().position(|&p| p == b) {
            Some(p) => ids[i] = p as u16,
            None => {
                palette.push(b);
                ids[i] = (palette.len() - 1) as u16;
            }
        }
    }

    let bits: u8 = if palette.len() <= 1 {
        0
    } else {
        (16 - ((palette.len() as u16 - 1).leading_zeros() as u8)).min(8)
    };

    let mut out = Vec::new();
    out.push(9); // 子区块版本 9
    out.push(1); // 层数
    out.push(index as u8);
    out.push((bits << 1) | 1); // 运行时调色板

    if bits > 0 {
        let mut words = vec![0u32; (4096 * bits as usize).div_ceil(32)];
        for (i, &id) in ids.iter().enumerate() {
            let bit = i * bits as usize;
            let w = bit / 32;
            let off = bit % 32;
            words[w] |= (id as u32) << off;
            if off + bits as usize > 32 {
                words[w + 1] |= (id as u32) >> (32 - off);
            }
        }
        for w in words {
            out.extend_from_slice(&w.to_le_bytes());
        }
        zigzag(&mut out, palette.len() as i32);
    }

    for p in palette {
        zigzag(&mut out, p);
    }
    out
}

pub fn build_payload(spec: &ChunkSpec) -> Vec<u8> {
    let mut out = Vec::new();

    let mut y = spec.min_section;
    while y <= spec.highest_non_empty {
        let mut blocks = [spec.air_state; 4096];
        if y == spec.floor_section {
            // 让地面只占该段的下半部分（y offset 8..15），这样这一段是多值调色板，
            // 与 Geyser 发真实地形时的编码路径一致。
            for (i, b) in blocks.iter_mut().enumerate() {
                if (i & 0xF) >= 8 {
                    *b = spec.stone_state;
                }
            }
        }
        out.extend(paletted_subchunk(y, &blocks));
        y += 1;
    }

    let mut y = spec.min_section;
    while y <= spec.max_section {
        out.push(1); // 生物群系层头：(bits=0)<<1 | 1
        zigzag(&mut out, spec.biome_id);
        y += 1;
    }

    // 参考实现（Geyser ChunkUtils/JavaLevelChunkWithLightTranslator）：
    // 边界方块只写 1 字节 0，随后直接是 NBT 方块实体流（无实体则什么都不写）。
    out.push(0); // 边界方块计数
    out
}

pub fn sub_chunk_count(min_section: i8, highest_non_empty: i8) -> u32 {
    (highest_non_empty as i32 - min_section as i32 + 1) as u32
}

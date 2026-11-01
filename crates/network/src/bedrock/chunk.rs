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

    // 位宽必须是 Bedrock 允许的那几个值（见 `vendor/bedrock-rs/crates/level/src/bits.rs`：
    // `VALID_BITS = [1, 2, 3, 4, 5, 6, 8, 16]`）。用 ceil(log2(n)) 会算出 7/9/10/… 这类
    // 客户端不接受的值。单值时走下面的 bits = 0 特例。
    //
    // **不要 `.min(8)`**：一段 4096 个方块最多 4096 种状态 → 需要 16 位；
    // 截到 8 位会让索引静默截断成错方块（BUG-032 第 18 条）。
    const VALID_BITS: [u8; 8] = [1, 2, 3, 4, 5, 6, 8, 16];
    let bits: u8 = if palette.len() <= 1 {
        0
    } else {
        *VALID_BITS
            .iter()
            .find(|&&b| (1usize << b) >= palette.len())
            .unwrap_or(&16)
    };

    // 每个 u32 只能放 `32 / bits` 个索引（Bedrock 是**按 word 补位**，索引不跨 word）。
    //
    // 这一点极其关键：`bits = 1/2/4/8/16` 时补位与"连续打包"结果相同，所以过去的区块
    // 看起来都正常；一旦调色板涨到 5..=8 种（bits = 3），两种算法完全不同 ——
    // 客户端会把整段解析错，表现为**脚下的区块消失、玩家掉进虚空、甚至客户端崩溃**。
    // 参考实现：`vendor/bedrock-rs/crates/level/src/greedy.rs::to_disk`
    // （`let per_word = u32::BITS / bits; ... self.array.chunks(per_word)`）。
    let per_word: usize = 32 / bits.max(1) as usize;

    let mut out = Vec::new();
    out.push(9); // 子区块版本 9
    out.push(1); // 层数
    out.push(index as u8);
    out.push((bits << 1) | 1); // 运行时调色板

    if bits > 0 {
        let word_count = 4096usize.div_ceil(per_word);
        let mut words = vec![0u32; word_count];
        for (i, &id) in ids.iter().enumerate() {
            let w = i / per_word;
            let shift = (i % per_word) * bits as usize;
            words[w] |= (id as u32) << shift;
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

/// MCHPRS 的 y 从 0 开始，Bedrock 主世界的 y 从 -64 开始，两者相差 64。
pub const Y_OFFSET: i32 = 64;

/// Bedrock 主世界最低的子区块序号（y = -64）。
pub const BEDROCK_MIN_SECTION: i8 = -4;

/// Bedrock 主世界一共 24 个 16 格段（y = -64..319）。
pub const BEDROCK_SECTION_COUNT: i32 = 24;

// ---- 区块流送参数（BUG-032 第 26 条增量区块 + 第 19 条快照出 plot 线程）----

/// 登录时**同步**生成的初始半径（区块）。
///
/// 2 → 5×5 = 25 个区块。实测每区块 ≈ 0.71ms，所以这一批约 18ms，
/// 安全地留在 50ms 的 tick 预算里（以前一次性 289 区块要 205~260ms）。
pub const INITIAL_RADIUS_CHUNKS: i32 = 2;

/// 每 tick 最多流送的区块数。16 × 0.71ms ≈ 11ms。
pub const STREAM_BUDGET_CHUNKS: usize = 16;

/// 每 tick 流送的时间上限（毫秒）：先到哪个先停，双保险。
pub const STREAM_BUDGET_MS: u64 = 8;

/// 方形窗口内**还没发过**的区块坐标，由近到远排序。
///
/// 排序键：先按到中心的 Chebyshev 距离（方形一圈一圈填），再按欧氏距离 + 坐标，
/// 保证同一圈内顺序稳定（便于日志复现与测试）。
pub fn pending_chunks(
    center: (i32, i32),
    radius: i32,
    have: &std::collections::HashSet<(i32, i32)>,
) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            let chunk = (center.0 + dx, center.1 + dz);
            if have.contains(&chunk) {
                continue;
            }
            out.push(chunk);
        }
    }
    out.sort_by_key(|&(x, z)| {
        let dx = x - center.0;
        let dz = z - center.1;
        let ring = dx.abs().max(dz.abs());
        (ring, dx * dx + dz * dz, x, z)
    });
    out
}

/// 已发区块里**完整覆盖**的最大方形半径（0 = 只有中心那一块）。
///
/// publisher 半径只能按"真的都发完了"的方形来报 —— 第 27 条的坑就是这么来的：
/// 报得比发得大，客户端以为那里有区块，实际一片虚空。
pub fn covered_radius(
    center: (i32, i32),
    radius: i32,
    have: &std::collections::HashSet<(i32, i32)>,
) -> i32 {
    let mut covered = 0;
    for r in 1..=radius {
        let mut complete = true;
        'ring: for dx in -r..=r {
            for dz in -r..=r {
                if dx.abs().max(dz.abs()) != r {
                    continue;
                }
                if !have.contains(&(center.0 + dx, center.1 + dz)) {
                    complete = false;
                    break 'ring;
                }
            }
        }
        if complete {
            covered = r;
        } else {
            break;
        }
    }
    covered
}


/// 从任意方块来源生成一个 Bedrock `LevelChunk` 负载。
///
/// `get_block(x, y, z)` 返回 **MCHPRS（Java 1.20.4）的方块状态 id**，
/// 坐标是 MCHPRS 的方块坐标（y ∈ 0..383）。这里**不依赖 `mchprs_world::World`**：
/// 调用方传一个闭包即可（plot 线程传 `|x,y,z| self.world.get_block_raw(...)`），
/// 这样协议层不会反向依赖世界层，测试也不需要伪造整个 `World` trait。
///
/// `chunk_x` / `chunk_z` 是 **Bedrock 的区块坐标**。Bedrock 与 MCHPRS 的方块 x/z 直接对应，
/// y 相差 [`Y_OFFSET`]：Bedrock `y` 的方块 = MCHPRS 的 `y + 64`。
pub fn build_chunk_from_world<F>(
    get_block: F,
    chunk_x: i32,
    chunk_z: i32,
    biome_id: i32,
) -> Vec<u8>
where
    F: FnMut(i32, i32, i32) -> u32,
{
    build_chunk_from_world_with_signs(get_block, chunk_x, chunk_z, biome_id, &[])
}

/// 同 [`build_chunk_from_world`]，但把这一块区块里的**告示牌**一起写进负载（BUG-032 A2）。
///
/// 位置在"边界方块计数(0)"**之后**，是**直接串写的网络 NBT 流**（无数量前缀、无长度前缀），
/// 出处：Geyser `JavaLevelChunkWithLightTranslator`：
/// ```text
/// byteBuf.writeByte(0); // Border blocks
/// NbtUtils.createNetworkWriter(...); for (tag : bedrockBlockEntities) nbtStream.writeTag(tag);
/// ```
/// 漏掉这段 = 玩家进 plot 时所有告示牌都是空白（只有当场改字才会收到 `BlockActorData`）。
pub fn build_chunk_from_world_with_signs<F>(
    mut get_block: F,
    chunk_x: i32,
    chunk_z: i32,
    biome_id: i32,
    signs: &[crate::bedrock::bridge::BedrockSign],
) -> Vec<u8>
where
    F: FnMut(i32, i32, i32) -> u32,
{
    let mut out = Vec::new();

    // 诊断（BUG-032 第 33 条）：`MCHPRS_BEDROCK_DEBUG_CHUNK=1` 时把每个区块里出现的
    // 方块对（java 状态 id → 基岩 runtime id）打出来。方块映射出问题时唯一可靠的办法就是
    // 看客户端**实际收到了什么**，而不是推测映射表。
    let debug_chunk = std::env::var_os("MCHPRS_BEDROCK_DEBUG_CHUNK").is_some();
    let mut seen: Vec<(u32, i32)> = Vec::new();

    for section in 0..BEDROCK_SECTION_COUNT {
        // 索引顺序：Bedrock 是 `(x << 8) | (z << 4) | y` —— **y 是最低 4 位**。
        // 之前写成 `y*256 + z*16 + x`（x 最低位），方块会整体错位，表现为“条纹状”渲染。
        let mut blocks = [crate::bedrock::AIR_STATE; 4096];
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..16 {
                    let id = get_block(
                        chunk_x * 16 + x,
                        section * 16 + y, // Bedrock y + 64 == MCHPRS y
                        chunk_z * 16 + z,
                    );
                    let bedrock_id = crate::bedrock::bedrock_block_state(id);
                    blocks[((x << 8) | (z << 4) | y) as usize] = bedrock_id;
                    if debug_chunk && !seen.contains(&(id, bedrock_id)) && seen.len() < 64 {
                        seen.push((id, bedrock_id));
                    }
                }
            }
        }
        let index = (section as i32 + BEDROCK_MIN_SECTION as i32) as i8;
        out.extend(paletted_subchunk(index, &blocks));
    }

    // 全部 24 段都要有生物群系数据，否则客户端会自己猜。
    for _ in 0..BEDROCK_SECTION_COUNT {
        out.push(1); // 生物群系层头：(bits=0)<<1 | 1
        zigzag(&mut out, biome_id);
    }

    out.push(0); // 边界方块计数

    // 方块实体：告示牌（BUG-032 A2）。**没有数量前缀**，直接串写 NBT 文档。
    for sign in signs {
        out.extend(crate::bedrock::block_entity::sign_chunk_bytes(sign));
    }

    if debug_chunk && !seen.is_empty() {
        let list: Vec<String> = seen
            .iter()
            .map(|(java, bedrock)| format!("{java}->{bedrock}"))
            .collect();
        println!(
            "[CHUNK] ({chunk_x},{chunk_z}) block pairs {}: {}",
            seen.len(),
            list.join(" ")
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **钉死"按 word 补位"的位打包**（BUG-032 第 33 条的真凶）。
    ///
    /// 5 种方块 → bits = 3、每 word 只能放 10 个索引（`32 / 3 = 10`），
    /// 所以是 `ceil(4096 / 10) = 410` 个 u32，而不是"连续打包"的 384 个。
    /// 过去写成连续打包，`bits ≤ 2` 时两者恰好一致所以看不出问题；一到 bits = 3，
    /// 客户端整段解析错位 —— 脚下区块消失、玩家掉进虚空、甚至崩溃。
    #[test]
    fn palette_packs_words_without_straddling() {
        // 5 种状态循环，保证调色板大小正好是 5
        let mut blocks = [0i32; 4096];
        for (i, b) in blocks.iter_mut().enumerate() {
            *b = (i % 5) as i32;
        }
        let payload = paletted_subchunk(0, &blocks);

        let bits = 3usize;
        let per_word = 32 / bits; // 10
        let word_count = 4096usize.div_ceil(per_word); // 410

        // 头部 4 字节：版本、层数、subchunk index、层头
        assert_eq!(payload[0], 9);
        assert_eq!(payload[1], 1);
        assert_eq!(payload[3], (bits as u8) << 1 | 1);

        // 第一个 word：索引 0..=9 依次是 0,1,2,3,4,0,1,2,3,4
        let mut expected: u32 = 0;
        for k in 0..per_word {
            expected |= ((k % 5) as u32) << (k * bits);
        }
        let w0 = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
        assert_eq!(w0, expected, "第一个 word 必须是补位打包");

        // 最后一个 word 之前的所有 word 都必须能被精确写出
        let after_words = 4 + word_count * 4;
        assert!(
            payload.len() > after_words,
            "负载里必须真的写了 {word_count} 个 word"
        );
        // 紧接着是调色板长度（zigzag(5) = 10）与 5 个调色板项
        assert_eq!(payload[after_words], 10, "调色板长度应为 zigzag(5)");
        assert_eq!(payload.len(), after_words + 1 + 5);

        // 抽样校验第 10 个索引（应当进入第二个 word 的第 0 位）
        let w1 = u32::from_le_bytes([
            payload[8],
            payload[9],
            payload[10],
            payload[11],
        ]);
        assert_eq!(w1 & 0b111, (10 % 5) as u32, "索引 10 必须落在第二个 word");
    }

    /// 单值子区块的字节数：版本(9) + 层数(1) + index + 层头 + zigzag(状态 id)。
    fn uniform_section_len(state: i32) -> usize {
        let zigzag_len = {
            let mut out = Vec::new();
            zigzag(&mut out, state);
            out.len()
        };
        4 + zigzag_len
    }

    /// 全空气世界：24 段单值空气 + 24 段生物群系 + 边界方块计数。
    #[test]
    fn empty_world_payload_has_expected_size() {
        let payload = build_chunk_from_world(|_, _, _| 0, 0, 0, 1);
        let expected = 24 * uniform_section_len(crate::bedrock::AIR_STATE) + 24 * 2 + 1;
        assert_eq!(payload.len(), expected);
    }

    /// 整段都是石头的 4 个段会用到**更短**的 zigzag（石头 2403 比空气 11996 小），
    /// 于是整块负载比全空气**略小**。这条固化“单值段按 zigzag 长度变化”这个事实，
    /// 免得以后有人误以为区块负载长度是固定的。
    #[test]
    fn uniform_stone_sections_are_shorter_than_air() {
        let empty = build_chunk_from_world(|_, _, _| 0, 0, 0, 1);
        // MCHPRS 的 y ∈ 0..63 全是石头 → 段 0..3 是纯石头单值段
        let stone = build_chunk_from_world(|_, y, _| if y < 64 { 1 } else { 0 }, 0, 0, 1);
        assert!(
            stone.len() < empty.len(),
            "纯石头段应当比纯空气段短：{} vs {}",
            stone.len(),
            empty.len()
        );
    }

    /// 段内**混合**时才会走高位的调色板路径（写 128 个 u32），负载显著变大。
    #[test]
    fn mixed_section_is_much_larger() {
        let empty = build_chunk_from_world(|_, _, _| 0, 0, 0, 1);
        // 每段下半是石头、上半是空气 → 24 段全部变多值
        let mixed = build_chunk_from_world(|_, y, _| if y % 16 < 8 { 1 } else { 0 }, 0, 0, 1);
        assert!(
            mixed.len() > empty.len() + 1000,
            "混合段应当显著更大：{} vs {}",
            mixed.len(),
            empty.len()
        );
    }

    /// 区块坐标必须真的作用到取方块上：chunk (1,0) 读的是 x ∈ 16..31。
    #[test]
    fn chunk_coordinates_are_applied() {
        let mut max_x = i32::MIN;
        let _ = build_chunk_from_world(
            |x, _, _| {
                max_x = max_x.max(x);
                0
            },
            1,
            0,
            1,
        );
        assert_eq!(max_x, 31);
    }

    /// **钉死段内索引顺序**：Bedrock 是 `(x << 8) | (z << 4) | y`，y 最低 4 位。
    ///
    /// 世界为空，只在 (1, 0, 0) 放一块石头 → 该段变两值调色板（bits=1），
    /// 石头在索引 (1<<8)|(0<<4)|0 = 256 处，即第 8 个 u32 word 的第 0 位。
    /// 段头固定 4 字节，所以 payload[4 + 8*4] 必须是 0x01。
    #[test]
    fn block_index_uses_bedrock_order() {
        let payload = build_chunk_from_world(
            |x, y, z| if (x, y, z) == (1, 0, 0) { 1 } else { 0 },
            0,
            0,
            1,
        );
        assert_eq!(payload[4 + 8 * 4], 0x01, "石头应当落在索引 256（y 为最低位）");

        // 再验一次 y 方向：在 (0, 1, 0) 放石头 → 索引 1，即 word 0 的第 1 位 = 0x02。
        let payload = build_chunk_from_world(
            |x, y, z| if (x, y, z) == (0, 1, 0) { 1 } else { 0 },
            0,
            0,
            1,
        );
        assert_eq!(payload[4], 0x02, "y=1 的方块应当落在索引 1");
    }

    /// 告示牌（A2）：负载尾部多出来的就是那一串方块实体 NBT，其余字节必须**完全不变**
    /// （不能因为加了牌子就挪动段/生物群系/边界字节的位置）。
    #[test]
    fn signs_are_appended_after_border_byte() {
        use crate::bedrock::bridge::BedrockSign;
        let plain = build_chunk_from_world(|_, _, _| 0, 0, 0, 1);
        let sign = BedrockSign {
            x: 5,
            y: 70,
            z: 6,
            front: [String::from("hi"), String::new(), String::new(), String::new()],
            back: Default::default(),
        };
        let with = build_chunk_from_world_with_signs(|_, _, _| 0, 0, 0, 1, &[sign]);
        assert!(with.len() > plain.len(), "带牌子的区块必须更长");
        assert_eq!(&with[..plain.len()], &plain[..], "前缀必须一字不差");
        assert_eq!(*plain.last().unwrap(), 0, "边界字节是 0");
        // 边界字节之后紧接着就是区块里那块牌的根标签
        assert_eq!(with[plain.len()], 10, "方块实体流必须以 TAG_Compound 开头");
    }

    /// 流送队列：已发过的不能重复入队；顺序由近到远（一圈一圈填）。
    #[test]
    fn pending_chunks_skips_sent_and_orders_by_ring() {
        let mut have = std::collections::HashSet::new();
        have.insert((0, 0));
        have.insert((1, 0));
        let pending = pending_chunks((0, 0), 1, &have);
        // 半径 1 的方形 = 9 块，去掉已有的 2 块 → 7 块
        assert_eq!(pending.len(), 7);
        assert!(!pending.contains(&(0, 0)));
        assert!(!pending.contains(&(1, 0)));
        // 第一个必然是紧挨中心的（Chebyshev 距离 1）
        assert_eq!(pending[0].0.abs().max(pending[0].1.abs()), 1);
        // "由近到远"：环号单调不减
        let rings: Vec<i32> = pending
            .iter()
            .map(|(x, z)| x.abs().max(z.abs()))
            .collect();
        assert!(rings.windows(2).all(|w| w[0] <= w[1]), "环号必须单调不减：{rings:?}");
    }

    /// publisher 半径只能按"真的发满了的方形"来报（第 27 条：报大了会一片虚空）。
    #[test]
    fn covered_radius_grows_only_for_complete_squares() {
        let center = (0, 0);
        let mut have = std::collections::HashSet::new();
        have.insert(center);
        assert_eq!(covered_radius(center, 4, &have), 0);

        // 补满半径 1 的一圈 → 1
        for dx in -1..=1 {
            for dz in -1..=1 {
                have.insert((dx, dz));
            }
        }
        assert_eq!(covered_radius(center, 4, &have), 1);

        // 半径 2 少一块 → 仍然是 1
        for dx in -2..=2 {
            for dz in -2..=2 {
                if (dx, dz) != (2, -2) {
                    have.insert((dx, dz));
                }
            }
        }
        assert_eq!(covered_radius(center, 4, &have), 1);

        // 补上那块 → 2
        have.insert((2, -2));
        assert_eq!(covered_radius(center, 4, &have), 2);
    }
}

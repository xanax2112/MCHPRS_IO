//! Bedrock 的方块实体（方块 actor）数据：告示牌（BUG-032 A2）。
//!
//! 两个出口，语义相同、编码相同：
//! 1. `BlockActorDataPacket` —— 单块牌的**实时更新**（玩家改字、放牌之后立刻刷）；
//! 2. 区块负载尾部直接串写的 NBT 流 —— 玩家**进入世界/进 plot** 时随区块一起下发
//!    （Geyser `JavaLevelChunkWithLightTranslator` 就是在边界方块字节之后
//!    `nbtStream.writeTag(blockEntity)` 逐个写，**没有数量前缀**）。
//!
//! 编码口径全部照 Geyser `SignBlockEntityTranslator` + `BlockEntityTranslator`：
//!
//! | 键 | 值 | 出处 |
//! | --- | --- | --- |
//! | `id` | `"Sign"` | `BlockEntityUtils.getBedrockBlockEntityId`（`SIGN` → `Sign`） |
//! | `x`/`y`/`z` | Int（**Bedrock 坐标**） | `getConstantBedrockTag` |
//! | `FrontText`/`BackText` | Compound | `translateTag` |
//! | ↳ `Text` | String（四行用 `\n` 拼、去掉尾部空行） | `translateSide` |
//! | ↳ `SignTextColor` | Int `0xFF000000` | `getBedrockSignColor("black")`（default 0 + 不透明） |
//! | ↳ `IgnoreLighting` | Byte(0)（MCHPRS 没有发光字） | `has_glowing_text` |
//! | `IsWaxed` | Byte(0) | `translateTag` |
//!
//! **已知限制**（登记在 BACKLOG §9.21）：Geyser 还会按字符宽度裁剪超长行
//! （`SignUtils.SIGN_WIDTH_MAX`），我们没做 —— MCHPRS 的牌子来自 schematic，
//! 行一般不会超宽；真超宽时 Bedrock 会自己换行。

use std::collections::HashMap;

use bedrock::protocol::v662::packets::BlockActorDataPacket;
use bedrock::protocol::v662::types::NetworkBlockPosition;

use super::bridge::BedrockSign;
use super::chunk::Y_OFFSET;
use super::protocol::Protocol;

/// 一条 NBT 复合标签（键是 `String`，值是 `nbtx::Value`）。
///
/// 直接用 `String` 而不是 `nbtx::Compound`（它的键是 `bstr::BString`）：
/// 外面两个出口（`BlockActorDataPacket.actor_data_tags` 与 NBT 编码器）都接受
/// 「`String` 键的 map」，而 `HashMap<String, _>` 序列化出来正是复合标签。
pub type Tags = HashMap<String, nbtx::Value>;

fn compound(fields: Vec<(&str, nbtx::Value)>) -> nbtx::Value {
    let mut map = nbtx::Compound::default();
    for (key, value) in fields {
        map.insert(key.into(), value);
    }
    nbtx::Value::Compound(map)
}

/// 告示牌文字颜色：**不写这个键**，用客户端默认色。
///
/// 踩坑记录（BUG-032 第 43 条，三轮真机）：
/// - 写 `0xFF000000`（"不透明黑"）→ 真机**看不见字**（两次确认）；
/// - 写 `0xFFFFFFFF` + `IgnoreLighting=1`（发光白）→ 可见，但是发光；
/// - 写 `0xFFFFFFFF` + `IgnoreLighting=0` → 可见，但**显示成灰色**（用户：不像原来的样子）。
///
/// 结论：客户端对 `SignTextColor` 的通道语义和我们以为的不一样（亮色会被光照压成灰，
/// 而"黑"直接看不见），所以干脆**不发这个键** —— Geyser `SignBlockEntityTranslator`
/// 也是只在 Java 牌子带 `Color` 标签时才写它，其余走客户端默认色。
/// `IgnoreLighting` 保持 0（不发光）。
fn side_nbt(rows: &[String; 4]) -> nbtx::Value {
    let mut text = rows.join("\n");
    // Geyser：去掉尾部换行（否则编辑时光标从牌子最底下开始）。
    while text.ends_with('\n') {
        text.pop();
    }
    compound(vec![
        ("Text", nbtx::Value::String(text.as_str().into())),
        ("IgnoreLighting", nbtx::Value::Byte(0)),
    ])
}

/// 一块告示牌的完整 NBT（`sign` 里是 **MCHPRS 坐标**，这里换成 Bedrock 的 `y - 64`）。
pub fn sign_nbt(sign: &BedrockSign) -> Tags {
    let mut map = Tags::new();
    map.insert("x".to_string(), nbtx::Value::Int(sign.x));
    map.insert(
        "y".to_string(),
        nbtx::Value::Int(sign.y - Y_OFFSET),
    );
    map.insert("z".to_string(), nbtx::Value::Int(sign.z));
    map.insert("id".to_string(), nbtx::Value::String("Sign".into()));
    map.insert("FrontText".to_string(), side_nbt(&sign.front));
    map.insert("BackText".to_string(), side_nbt(&sign.back));
    map.insert("IsWaxed".to_string(), nbtx::Value::Byte(0));
    map
}

/// 单块告示牌的实时更新包（`BlockActorData`）。
///
/// 包里的坐标是**Bedrock 坐标**（`y - 64`），和 `UpdateBlock` 一致。
pub fn sign_actor_data(sign: &BedrockSign) -> BlockActorDataPacket<Protocol> {
    BlockActorDataPacket {
        block_position: NetworkBlockPosition {
            x: sign.x,
            // vendored `NetworkBlockPosition.y` 是 **u32**（不是 i32）：
            // Bedrock 主世界 y ≥ -64，负数会被截断成天文数字，所以必须显式 cast。
            y: (sign.y - Y_OFFSET) as u32,
            z: sign.z,
        },
        actor_data_tags: sign_nbt(sign),
    }
}

/// 从客户端发来的 `BlockActorData` 里取出告示牌两面各四行（BUG-032 第 43 条）。
///
/// 键与我们下发的一模一样（`FrontText` / `BackText` → `Text`，四行用 `\n` 拼），
/// 所以反解就是照着自己的编码表读。
///
/// 返回 `None` = 这份 NBT 里**两面都没有**，说明不是告示牌（我们只处理牌子，
/// 容器类方块实体属 §9.18.3 的 A7，已裁决不做）。其余情况一律返回四行，
/// 全空也算 —— 玩家把字清空必须能把旧字覆盖掉。
///
/// 这是**外部输入**：缺键、类型不对、非 UTF-8 都当空行处理，绝不 panic；
/// 多过四行直接丢弃（客户端也不会发第五行）。
pub fn sign_rows(tags: &Tags) -> Option<([String; 4], [String; 4])> {
    if !tags.contains_key("FrontText") && !tags.contains_key("BackText") {
        return None;
    }
    Some((
        side_rows(tags, "FrontText"),
        side_rows(tags, "BackText"),
    ))
}

/// 读一侧（`FrontText` / `BackText`）的四行。
fn side_rows(tags: &Tags, key: &str) -> [String; 4] {
    let mut rows: [String; 4] = std::array::from_fn(|_| String::new());
    let Some(nbtx::Value::Compound(side)) = tags.get(key) else {
        return rows;
    };
    // `nbtx::Compound` 的键是 `bstr::BString`（保存原始字节），按 `&[u8]` 取
    // 走的是 `BString: Borrow<[u8]>`，不需要先把键转成 `String`。
    let Some(nbtx::Value::String(text)) = side.get(b"Text".as_slice()) else {
        return rows;
    };
    let raw: &[u8] = text.as_ref();
    for (row, line) in rows
        .iter_mut()
        .zip(String::from_utf8_lossy(raw).split('\n'))
    {
        *row = line.to_string();
    }
    rows
}

/// 区块负载尾部要串写的方块实体 NBT 字节（网络 NBT：little-endian + varint 长度）。
pub fn sign_chunk_bytes(sign: &BedrockSign) -> Vec<u8> {
    let mut out = Vec::new();
    // 写失败只可能是内存问题；这里不 panic，直接返回已经写出的部分（区块仍可用）。
    let _ = nbtx::to_varint_bytes_in(&mut out, &sign_nbt(sign));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bedrock::protocol::ProtoCodec;

    fn sample() -> BedrockSign {
        BedrockSign {
            x: 100,
            y: 70,
            z: -20,
            front: [
                "第一行".to_string(),
                "line 2".to_string(),
                String::new(),
                String::new(),
            ],
            back: [
                String::new(),
                "back".to_string(),
                String::new(),
                String::new(),
            ],
        }
    }

    /// 从编码后的字节里找一段 UTF-8 子串（NBT 字符串就是原样写字节，没有转义）。
    ///
    /// 用它代替"解出 `FrontText` 再查 `Text`"：`nbtx::Compound` 的键是
    /// `bstr::BString`（原始字节），按 `&str` 取值要绕一圈，而我们要验的其实就是
    /// "这四行到底是不是按 `\n` 拼进去了"，字节比对更直接。
    fn contains(bytes: &[u8], needle: &str) -> bool {
        let needle = needle.as_bytes();
        bytes.windows(needle.len()).any(|window| window == needle)
    }

    /// 告示牌 NBT 的键必须和 Geyser 一致（少一个键客户端就不认这块牌子）。
    ///
    /// 一面两个键：`Text` + `IgnoreLighting`（0 = 不发光）；**不写 `SignTextColor`**。
    /// 为什么不写颜色（黑看不见、白显示成灰），见 `side_nbt` 的踩坑记录。
    #[test]
    fn sign_nbt_matches_geyser_keys() {
        let map = sign_nbt(&sample());
        for key in ["x", "y", "z", "id", "FrontText", "BackText", "IsWaxed"] {
            assert!(map.contains_key(key), "缺少 {key}，实际 {:?}", map.keys());
        }
        assert_eq!(map.len(), 7, "不该多出 Geyser 没有的键");
        // y 必须已经减 64（MCHPRS 70 → Bedrock 6）
        assert_eq!(map["y"], nbtx::Value::Int(6));
        assert_eq!(map["id"], nbtx::Value::String("Sign".into()));
        assert_eq!(map["IsWaxed"], nbtx::Value::Byte(0));
        let nbtx::Value::Compound(front) = &map["FrontText"] else {
            panic!("FrontText 必须是 Compound");
        };
        assert_eq!(front.len(), 2, "一面两个键：Text / IgnoreLighting");
        assert!(
            !front.contains_key(b"SignTextColor".as_slice()),
            "不写 SignTextColor：写黑看不见、写白会显示成灰色"
        );
        assert_eq!(
            front.get(b"IgnoreLighting".as_slice()),
            Some(&nbtx::Value::Byte(0)),
            "不能发光（用户要求去掉发光）"
        );
    }

    /// 正面四行按 `\n` 拼接、**尾部空行去掉**；反面必须用 back 那四行
    /// （core 的 `BlockEntity::to_nbt` 里曾经把 front 抄给 back）。
    #[test]
    fn text_joins_rows_and_keeps_sides_apart() {
        let bytes = sign_chunk_bytes(&sample());
        assert!(contains(&bytes, "第一行\nline 2"), "正面四行应当用 \\n 拼接");
        assert!(
            !contains(&bytes, "第一行\nline 2\n"),
            "尾部空行必须去掉，不能以换行结尾"
        );
        // back 的第一行是空的，所以它是 "\nback"
        assert!(contains(&bytes, "\nback"), "反面应当写成 \"\\nback\"");
        assert!(!contains(&bytes, "back\n第一行"), "两面不能串在一起");
    }

    /// `BlockActorData` 的坐标是 Bedrock 坐标，且整个包装完能原样解回来。
    #[test]
    fn block_actor_data_roundtrips() {
        let packet = sign_actor_data(&sample());
        assert_eq!(packet.block_position.x, 100);
        assert_eq!(packet.block_position.y, 6);
        // `NetworkBlockPosition.y` 是 u32，负数坐标要能被正确 cast 而不是截断
        assert_eq!(packet.block_position.z, -20);
        let mut buf = Vec::new();
        packet.serialize(&mut buf).expect("serialize");
        assert_eq!(*buf.last().unwrap(), 0, "复合标签以 TAG_End 收尾");
        let decoded = BlockActorDataPacket::<Protocol>::deserialize(&mut buf.as_slice())
            .expect("deserialize");
        assert_eq!(decoded.block_position.y, 6);
        assert_eq!(decoded.actor_data_tags.len(), 7);
    }

    /// 区块尾部字节：直接就是网络 NBT 根标签，**没有长度前缀、没有数量前缀**。
    #[test]
    fn chunk_bytes_are_a_bare_nbt_tag() {
        let bytes = sign_chunk_bytes(&sample());
        assert_eq!(bytes[0], 10, "第一个字节必须是 TAG_Compound(10)");
        assert_eq!(*bytes.last().unwrap(), 0, "复合标签必须以 TAG_End(0) 收尾");
        // 能被解析回来（证明是完整的 NBT 文档）
        let decoded: Tags = nbtx::from_varint_bytes(&mut bytes.as_slice()).expect("parse nbt");
        assert_eq!(decoded.len(), 7);
    }

    /// 我们发出去的 NBT，客户端原样发回来时必须解回同样的四行
    /// （进 plot 发一份 → 玩家改字 → 客户端把整份 NBT 发回来，这就是第 43 条的主路径）。
    #[test]
    fn sign_rows_roundtrip_our_own_nbt() {
        let sample = sample();
        let (front, back) = sign_rows(&sign_nbt(&sample)).expect("是告示牌");
        assert_eq!(front, sample.front);
        assert_eq!(back, sample.back);
    }

    /// 外部输入不能炸：缺键 / 类型不对 / 没有告示牌字段。
    #[test]
    fn sign_rows_tolerates_junk() {
        let empty = Tags::new();
        assert!(sign_rows(&empty).is_none(), "没有 FrontText/BackText 就不该当牌子");

        // 有键、但值是 Int 而不是 Compound → 全空行，不 panic
        let mut wrong = Tags::new();
        wrong.insert("FrontText".to_string(), nbtx::Value::Int(1));
        let (front, back) = sign_rows(&wrong).expect("有键就算牌子");
        assert_eq!(front, ["", "", "", ""]);
        assert_eq!(back, ["", "", "", ""]);

        // 复合标签里缺 `Text`
        let mut no_text = Tags::new();
        no_text.insert(
            "BackText".to_string(),
            compound(vec![("IsWaxed", nbtx::Value::Byte(0))]),
        );
        let (front, back) = sign_rows(&no_text).expect("有键就算牌子");
        assert_eq!(front, ["", "", "", ""]);
        assert_eq!(back, ["", "", "", ""]);
    }

    /// **对拍**：Cloudburst v766 编出来的真实字节（`refs/ProbeSign.java`）必须能被我们
    /// 完整解出来，且解出来的文字与输入一致。
    ///
    /// 为什么用"解参考字节"而不是"比字节串"：NBT 复合标签的键序不参与语义
    /// （Cloudburst 写 `x,y,z,id,FrontText,BackText,IsWaxed`，我们的 `HashMap` 顺序随机），
    /// 所以比字节串会误报；比"解出来的内容"才是真等价。
    /// 自己 serialize、自己 deserialize 抓不到系统性格式错误，这条对拍就是判据。
    #[test]
    fn sign_actor_data_reads_cloudburst_bytes() {
        // refs/ProbeSign.java 输出：pos=(54,-56,483)，FrontText="hello\nworld"，BackText=""
        const CB_HEX: &str = "6cc8ffffff0fc6070a000301786c0301796f03017ac60708026964045369676e\
0a0946726f6e74546578740804546578740b68656c6c6f0a776f726c64030d5369676e54657874436f6c6f72ffffff0f\
010e49676e6f72654c69676874696e6700000a084261636b5465787408045465787400030d5369676e54657874436f6c\
6f72ffffff0f010e49676e6f72654c69676874696e6700000107497357617865640000";
        let mut bytes: Vec<u8> = Vec::with_capacity(CB_HEX.len() / 2);
        let clean: String = CB_HEX.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        for i in (0..clean.len()).step_by(2) {
            bytes.push(u8::from_str_radix(&clean[i..i + 2], 16).expect("hex"));
        }
        let decoded =
            BlockActorDataPacket::<Protocol>::deserialize(&mut bytes.as_slice()).expect("deserialize");
        assert_eq!(decoded.block_position.x, 54);
        assert_eq!(decoded.block_position.y, (-56i32) as u32);
        assert_eq!(decoded.block_position.z, 483);
        let (front, back) = sign_rows(&decoded.actor_data_tags).expect("是告示牌");
        assert_eq!(front, ["hello", "world", "", ""]);
        assert_eq!(back, ["", "", "", ""]);
    }

    /// 超过四行只取前四行（不然会越界写内存）。
    #[test]
    fn sign_rows_drops_extra_lines() {
        let mut tags = Tags::new();
        tags.insert(
            "FrontText".to_string(),
            compound(vec![("Text", nbtx::Value::String("1\n2\n3\n4\n5".into()))]),
        );
        let (front, _) = sign_rows(&tags).expect("是告示牌");
        assert_eq!(front, ["1", "2", "3", "4"]);
    }
}

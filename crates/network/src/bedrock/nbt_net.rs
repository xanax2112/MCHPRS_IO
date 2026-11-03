//! 薄转发层：实现已上移到 `vendor/bedrock-rs/crates/protocol/src/nbt_net.rs`，
//! 由基岩包 codec 与本 crate 共用**同一份**（不再复制第二份实现）。
//!
//! 本模块只做三件事：转发公开 API、给 `crate::bedrock` 内部调用点用、放测试。
//! 为什么不把测试放 vendor：vendor 是独立 workspace（被仓库 `exclude`），
//! `cargo test -p mchprs_network --lib` 根本不会编译它；测试必须在这里跑。

pub(crate) use bedrock::protocol::nbt_net::{
    NbtCompound, read_root_compound,
    write_compound_stream,
};

#[cfg(test)]
mod tests {
    use super::{
        NbtCompound, read_root_compound,
        write_compound_stream,
    };
    use bedrock::protocol::v662::packets::{
        AvailableActorIdentifiersPacket, BiomeDefinitionListPacket, BlockActorDataPacket,
        ItemComponentPacket,
    };
    use bedrock::protocol::v662::types::NetworkBlockPosition;
    use bedrock::protocol::{ProtoCodec, V766 as Protocol};
    use std::collections::HashMap;
    use std::io::{Cursor, Read};

    // 与 `session.rs` 里内嵌的是同一份文件（这里单独 include，避免把常量改成 pub(crate)）。
    const BIOME: &[u8] = include_bytes!("data/biome_definitions.dat");
    const ENTITIES: &[u8] = include_bytes!("data/entity_identifiers.dat");

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

    /// 全类型：每种 tag 一条，覆盖 zigzag 正负、i64::MIN、多字节 UTF-8、空列表、嵌套。
    fn all_types_bytes() -> Vec<u8> {
        let mut buf = root();
        buf.push(3);
        put_string(&mut buf, "i_neg");
        vari32(&mut buf, -1);
        buf.push(3);
        put_string(&mut buf, "i_pos");
        vari32(&mut buf, 300);
        buf.push(4);
        put_string(&mut buf, "l_neg");
        vari64(&mut buf, -1_234_567_890_123);
        buf.push(4);
        put_string(&mut buf, "l_min");
        vari64(&mut buf, i64::MIN);
        buf.push(2);
        put_string(&mut buf, "short");
        buf.extend_from_slice(&(-2i16).to_le_bytes());
        buf.push(5);
        put_string(&mut buf, "float");
        buf.extend_from_slice(&1.5f32.to_le_bytes());
        buf.push(6);
        put_string(&mut buf, "double");
        buf.extend_from_slice(&(-2.25f64).to_le_bytes());
        // 多字节 UTF-8 key + value（长度前缀按字节）
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
        buf.push(0);
        buf
    }

    #[test]
    fn decodes_all_value_types() {
        let map = read_root_compound(&all_types_bytes()).expect("应当解析成功");
        use nbtx::Value;
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
        assert_eq!(map.get("bytes"), Some(&Value::ByteArray(vec![1, 2, 3])));
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

    /// 写 → 读 往返：同一份 map 编解码后必须完全相等（含 i64::MIN 与多字节 UTF-8）。
    #[test]
    fn write_read_roundtrip() {
        let original = read_root_compound(&all_types_bytes()).expect("解析");
        let bytes = write_compound_bytes(&original).expect("写出");
        assert_eq!(bytes[0], 10, "根 tag 必须是 TAG_Compound(10)");
        assert_eq!(*bytes.last().expect("非空"), 0, "以 TAG_End 收尾");
        let decoded = read_root_compound(&bytes).expect("回读");
        assert_eq!(decoded, original, "写→读 必须逐键相等");
    }

    /// writer 的字节级用例：空 compound 只有 3 字节（10, 0, 0）。
    #[test]
    fn writes_empty_compound_as_three_bytes() {
        let empty: HashMap<String, nbtx::Value> = HashMap::new();
        let bytes = write_compound_bytes(&empty).expect("写出");
        assert_eq!(bytes, vec![10, 0, 0]);
    }

    /// 两张 Geyser `.dat`：读出来的内容再写回去、再读回来，键值必须完全一致。
    #[test]
    fn dat_roundtrips_through_writer() {
        for (name, raw) in [
            ("biome_definitions.dat", BIOME),
            ("entity_identifiers.dat", ENTITIES),
        ] {
            let first = read_root_compound(raw).expect(name);
            assert!(!first.is_empty(), "{name} 解析为空");
            let bytes = write_compound_bytes(&first).expect(name);
            let second = read_root_compound(&bytes).expect(name);
            assert_eq!(second, first, "{name} 写→读 不一致");
        }
        // 结构断言（不用魔法数字）
        let entities = read_root_compound(ENTITIES).expect("entity_identifiers.dat");
        match entities.get("idlist") {
            Some(nbtx::Value::List(list)) => assert!(!list.is_empty(), "idlist 不应为空"),
            other => panic!("idlist 缺失或类型不对: {other:?}"),
        }
    }

    /// 三个带非空 NBT 的包：serialize → deserialize → 逐键相等。
    #[test]
    fn packets_roundtrip_with_nbt() {
        let biomes = read_root_compound(BIOME).expect("biomes");
        let entities = read_root_compound(ENTITIES).expect("entities");

        let pkt = BiomeDefinitionListPacket {
            biome_definition_data: NbtCompound(biomes.clone()),
        };
        let mut buf = Vec::new();
        pkt.serialize(&mut buf).expect("serialize biome");
        assert_eq!(pkt.size_hint(), buf.len(), "size_hint 必须等于实际字节数");
        let back = BiomeDefinitionListPacket::deserialize(&mut buf.as_slice()).expect("deserialize");
        assert_eq!(back.biome_definition_data.as_map(), &biomes);

        let pkt = AvailableActorIdentifiersPacket {
            actor_info_list: NbtCompound(entities.clone()),
        };
        let mut buf = Vec::new();
        pkt.serialize(&mut buf).expect("serialize entities");
        let back =
            AvailableActorIdentifiersPacket::deserialize(&mut buf.as_slice()).expect("deserialize");
        assert_eq!(back.actor_info_list.as_map(), &entities);

        let mut tags: HashMap<String, nbtx::Value> = HashMap::new();
        tags.insert("id".to_string(), nbtx::Value::String("Sign".into()));
        tags.insert("IsWaxed".to_string(), nbtx::Value::Byte(0));
        let pkt = BlockActorDataPacket::<Protocol> {
            block_position: NetworkBlockPosition { x: 1, y: 2, z: 3 },
            actor_data_tags: NbtCompound(tags.clone()),
        };
        let mut buf = Vec::new();
        pkt.serialize(&mut buf).expect("serialize actor data");
        let back = BlockActorDataPacket::<Protocol>::deserialize(&mut buf.as_slice())
            .expect("deserialize actor data");
        assert_eq!(back.actor_data_tags.as_map(), &tags);
    }

    /// 空 vec 不触 nbtx：`ItemComponentPacket { items: vec![] }` 只写一个 0 字节。
    #[test]
    fn empty_vec_does_not_touch_nbt() {
        let pkt = ItemComponentPacket { items: vec![] };
        let mut buf = Vec::new();
        pkt.serialize(&mut buf).expect("serialize");
        assert_eq!(buf, vec![0u8], "空 vec 应当只有长度 0，不写任何 NBT");
    }

    /// `read_compound_stream` 只消费自己的字节，后面的数据留给调用方。
    #[test]
    fn stream_reader_stops_at_its_own_end() {
        let mut buf = vec![10u8, 0u8, 3u8];
        put_string(&mut buf, "k");
        vari32(&mut buf, 42);
        buf.push(0);
        buf.extend_from_slice(b"TAIL");
        let mut cursor = Cursor::new(buf);
        let map = read_compound_stream(&mut cursor).expect("解析");
        assert_eq!(map.get("k"), Some(&nbtx::Value::Int(42)));
        let mut rest = Vec::new();
        cursor.read_to_end(&mut rest).expect("读剩余");
        assert_eq!(rest, b"TAIL", "不能把后面的数据吃掉");
    }

    #[test]
    fn rejects_non_compound_root() {
        let mut buf = vec![1u8];
        put_string(&mut buf, "");
        buf.push(0);
        let err = read_root_compound(&buf).expect_err("根 tag 为 Byte 时应当报错");
        assert!(err.what.contains("root tag"), "错误信息: {err}");
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut buf = root();
        buf.push(0);
        buf.push(0xff);
        let err = read_root_compound(&buf).expect_err("尾部残留应当报错");
        assert!(err.what.contains("trailing bytes"), "错误信息: {err}");
    }

    #[test]
    fn rejects_truncated_string() {
        let mut buf = root();
        buf.push(8);
        put_string(&mut buf, "k");
        varu32(&mut buf, 10);
        let err = read_root_compound(&buf).expect_err("截断字符串应当报错");
        assert!(err.what.contains("failed to read"), "错误信息: {err}");
    }

    #[test]
    fn rejects_unknown_tag() {
        let mut buf = root();
        buf.push(13);
        put_string(&mut buf, "k");
        let err = read_root_compound(&buf).expect_err("未知 tag 应当报错");
        assert!(err.what.contains("tag"), "错误信息: {err}");
    }

    #[test]
    fn rejects_negative_list_len() {
        let mut buf = root();
        buf.push(9);
        put_string(&mut buf, "k");
        buf.push(1);
        vari32(&mut buf, -1);
        let err = read_root_compound(&buf).expect_err("负长度应当报错");
        assert!(err.what.contains("negative length"), "错误信息: {err}");
    }

    #[test]
    fn rejects_nonempty_end_list() {
        let mut buf = root();
        buf.push(9);
        put_string(&mut buf, "k");
        buf.push(0);
        vari32(&mut buf, 1);
        let err = read_root_compound(&buf).expect_err("非空 End 列表应当报错");
        assert!(err.what.contains("TAG_End"), "错误信息: {err}");
    }

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
        let err = read_root_compound(&buf).expect_err("超深嵌套应当报错");
        assert!(err.what.contains("nesting"), "错误信息: {err}");
    }

    /// `size_hint` 与实际写出的字节数必须严格一致（包长度靠它算）。
    #[test]
    fn size_hint_matches_serialized_len() {
        for raw in [BIOME, ENTITIES] {
            let map = read_root_compound(raw).expect("解析");
            let nbt = NbtCompound(map);
            let mut buf = Vec::new();
            write_compound_stream(nbt.as_map(), &mut buf).expect("写出");
            assert_eq!(nbt.size_hint(), buf.len());
        }
    }
}

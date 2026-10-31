//! Bedrock 创造背包（`CreativeContentPacket`），对应 BUG-032 R1。
//!
//! 条目表由 `crates/network/generators/gen_creative.js` 生成到 [`crate::bedrock::creative_gen`]。
//! 数据源是 **Geyser 自带的 Bedrock 1.21.50 资源**（`creative_items.1_21_50.json` +
//! `block_palette.1_21_50.nbt` + `runtime_item_states.1_21_50.json`），也就是 Geyser 自己
//! 启动时读的那三份 —— 见铁律 7「基岩版行为以 Geyser 为唯一权威」。
//!
//! 每条 `creative_net_id` 从 1 递增 —— 与 Geyser 的做法一致
//! （`ItemRegistryPopulator`：`creativeNetId.incrementAndGet()`），客户端拿它回指"我要这个物品"。

use bedrock::protocol::v662::packets::{CreativeContentPacket, WriteEntry};
use bedrock::protocol::v662::types::NetworkItemInstanceDescriptor;

use crate::bedrock::creative_gen::CREATIVE_ITEMS;
use crate::bedrock::protocol::Protocol;

/// Cloudburst 对"这个物品没有 NBT"写出来的是 **10 个 0 字节**。
///
/// 逐字节实测见 `refs/ProbeText.java` / `refs/ProbeItemNbt.java`：条目在
/// `block_runtime_id` 之后是 `0a` 后面跟 10 个 `00`，其中 `0a` 正是我们
/// `Option<Vec<u8>>` 编码器写出的长度前缀（=10），所以这里只要把这 10 个字节交给它，
/// 整段就与参考实现完全一致。
///
/// **带 NBT 的变体不进表**（用户 2026-10-31 决定："附魔书，旗帜，烟花火箭，烟花之星全部删掉"）：
/// 那 172 条（附魔书 122、旗帜 17、烟花火箭 17、烟花之星 16）只靠 NBT 区分，而 Bedrock 的
/// 「网络 NBT」编码（整数是 zigzag varint）我们还没对齐，发空 NBT 只会得到一堆一模一样、
/// 名字还错的条目。生成器直接把带 `nbt_b64` 的条目跳掉，所以这里对所有条目都发空 NBT。
/// 剩下会用到 `user_data_buffer` 的地方（将来的容器/物品同步）再单独处理。
const EMPTY_ITEM_NBT: [u8; 10] = [0; 10];

/// 创造面板里每条条目携带的 `stack_size`。
///
/// Geyser 用 JSON 里的 `count`，而原版 `creative_items` 一条都没写 `count`，
/// 所以有效值就是默认的 **1**。这个数字只影响"拿到手时是一组几个"，
/// 不影响面板图标/名称的显示。
const CREATIVE_STACK_SIZE: u16 = 1;

/// 用完整条目表构造 `CreativeContentPacket`。
pub fn creative_content() -> CreativeContentPacket<Protocol> {
    build(CREATIVE_ITEMS, CREATIVE_STACK_SIZE)
}

/// 按给定条目构造（测试用它喂探针里的两条数据、以及不同的 `stack_size`）。
///
/// 条目元组：`(Bedrock 物品标识, 物品网络 id, 方块运行时 id, aux value)`。
/// 纯物品的方块运行时 id 是 0；注意 0 同时也是合法方块（`minecraft:cyan_terracotta`），
/// 所以不要拿 0 当"不是方块"的哨兵。
pub fn build(items: &[(&str, i32, i32, u32)], stack_size: u16) -> CreativeContentPacket<Protocol> {
    CreativeContentPacket::<Protocol> {
        write_entries: items
            .iter()
            .enumerate()
            .map(|(index, (_name, item_id, block_runtime_id, aux_value))| WriteEntry::<Protocol> {
                creative_net_id: (index + 1) as u32,
                item_instance: NetworkItemInstanceDescriptor {
                    // 物品网络 id（`runtime_item_states` 里的数字）
                    id: *item_id,
                    stack_size: Some(stack_size),
                    aux_value: Some(*aux_value),
                    block_runtime_id: Some(*block_runtime_id),
                    user_data_buffer: Some(EMPTY_ITEM_NBT.to_vec()),
                },
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bedrock::protocol::ProtoCodec;

    fn hex(pkt: &impl ProtoCodec) -> String {
        let mut buf = Vec::new();
        pkt.serialize(&mut buf).expect("serialize");
        buf.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// 期望字节来自 `refs/ProbeText.java`（Cloudburst `Bedrock_v766.CODEC`），
    /// 两条：stone（物品 1 / 方块 2403）、glowstone（物品 89 / 方块 5588）。
    ///
    /// 这条守的是**编码布局**（`stack_size` 用 LE u16、`aux`/`block_runtime_id` 用 varint、
    /// 空 NBT 是 10 个 0 字节）。当时探针按 `stack_size = 64` 编的，所以这里也传 64；
    /// 实际运行用的是 `CREATIVE_STACK_SIZE`（=1，跟着 Geyser 的 `count` 走）。
    #[test]
    fn creative_content_matches_geyser_codec() {
        let pkt = build(
            &[
                ("minecraft:stone", 1, 2403, 0),
                ("minecraft:glowstone", 89, 5588, 0),
            ],
            64,
        );
        assert_eq!(
            hex(&pkt),
            "020102400000c6250a0000000000000000000002b201400000a8570a00000000000000000000"
        );
    }

    /// 生成表本身要守住：**条目数 = Geyser 的 1712 条 − 172 条带 NBT 的变体 = 1540 条**，
    /// 没有空气、没有"物品 id 为 0"的坏条目、也没有那只靠 NBT 区分的四种物品。
    ///
    /// 这里**不能**要求名字唯一：原版面板本来就有同名多条目（药水 47 条、箭 43 条…），
    /// 靠 `aux_value` 区分。
    #[test]
    fn generated_table_is_sane() {
        assert_eq!(
            CREATIVE_ITEMS.len(),
            1540,
            "条目数应与 Geyser 的 creative_items.1_21_50.json 减去 172 条 NBT 变体一致"
        );
        for (name, item_id, block_runtime_id, _aux) in CREATIVE_ITEMS {
            assert_ne!(*name, "minecraft:air", "空气不该进创造背包");
            assert_ne!(*item_id, 0, "{name} 的物品 id 是 0（空气）");
            assert!(*block_runtime_id >= 0, "{name} 的方块运行时 id 非法");
        }
        // 只靠 NBT 区分的四种物品已全部删掉（用户 2026-10-31 决定）
        for removed in [
            "minecraft:enchanted_book",
            "minecraft:banner",
            "minecraft:firework_rocket",
            "minecraft:firework_star",
        ] {
            assert!(
                !CREATIVE_ITEMS.iter().any(|(name, ..)| *name == removed),
                "{removed} 应该已经被删掉"
            );
        }
    }

    /// 抽查几条与 Geyser 数据源一致的关键条目（防止生成器悄悄换错表）。
    #[test]
    fn generated_table_matches_geyser_source() {
        // stone：物品 1、方块 2403（这两条早已被 `refs/ProbeText.java` 逐字节确认过）
        assert!(CREATIVE_ITEMS.contains(&("minecraft:stone", 1, 2403, 0)));
        assert!(CREATIVE_ITEMS.contains(&("minecraft:glowstone", 89, 5588, 0)));
        // 原版第一条就是橡木木板；运行时 id 0 是 cyan_terracotta，说明 0 是合法方块
        assert_eq!(CREATIVE_ITEMS[0].0, "minecraft:oak_planks");
        assert!(CREATIVE_ITEMS.contains(&("minecraft:cyan_terracotta", -732, 0, 0)));
    }
}

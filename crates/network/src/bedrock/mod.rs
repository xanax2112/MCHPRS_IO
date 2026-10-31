//! 基岩版（Bedrock）协议栈接入层。
//!
//! 与同为协议实现的 [`crate::packets`]（Java 协议）平级：`packets/` 放 Java 包定义，
//! 本模块放 Bedrock 侧的协议常量与包构造。
//!
//! 背景与验收记录见仓库根目录 `BACKLOG.md` 的 `BUG-032`，
//! 协议细节与踩坑证据见 `docs/BEDROCK-SPIKE-STATUS.md`。
//!
//! 设计约束（来自项目硬要求）：
//! - 不引入第三方运行时（无 JVM / 无 Geyser / 无 ViaProxy），只用 Rust 依赖。
//! - 任何二进制编码都必须先与开源参考实现（Geyser 内置的 Cloudburst v766 codec）
//!   逐字节对齐，见本模块的 `#[cfg(test)]` 回归测试。

pub mod block_map;
pub mod bridge;
pub mod chunk;
pub mod creative;
pub mod creative_gen;
pub mod entity;
pub mod inventory;
pub mod items_gen;
pub mod item_map;
pub mod protocol;
pub mod scoreboard;
pub mod session;
pub mod text;
pub mod translate;

/// 本服务端锁定的 Bedrock 协议版本：766 = 1.21.51。
pub const PROTOCOL_VERSION: i32 = 766;

/// 客户端会在登录包里报告自己这一项，不匹配直接拒绝。
pub const GAME_VERSION: &str = "1.21.50";

/// Bedrock 的“空气”运行时方块状态 id。
pub const AIR_STATE: i32 = 11996;

/// 把 MCHPRS（Java 1.20.4）的方块状态 id 翻译成 Bedrock 运行时状态 id。
///
/// 未匹配到的方块（映射表里是 `-1`）一律回落到空气，
/// 这样即使某种方块没映射上，客户端也只会看不到它，而不会收到非法状态 id。
pub fn bedrock_block_state(mchprs_id: u32) -> i32 {
    match block_map::BLOCK_MAP.get(mchprs_id as usize) {
        Some(&id) if id >= 0 => id,
        _ => AIR_STATE,
    }
}

/// 把 Bedrock 的物品网络 id 翻译成 MCHPRS（Java）的物品 id。
///
/// 用于"放方块 / 右键交互"：Bedrock 的 `InventoryTransaction` 里带的是它自己的物品 id，
/// 而 plot 需要 `mchprs_blocks::items::Item`（见 BUG-032 R5）。
/// 表里没有 = MCHPRS 不认识这个物品，返回 `None`（空手处理，不会瞎放方块）。
pub fn mchprs_item_id(bedrock_item_id: i32) -> Option<u32> {
    item_map::ITEM_MAP
        .binary_search_by_key(&bedrock_item_id, |(_, bedrock_id, _)| *bedrock_id)
        .ok()
        .map(|index| item_map::ITEM_MAP[index].2)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 这四个值是**独立验证过**的 Bedrock 运行时 id（见 docs/BEDROCK-SPIKE-STATUS.md：
    /// 用 Geyser 内的 BDS 原始调色板核对过 stone=2403、glowstone=5588、glass=11468、air=11996）。
    /// 方块状态映射若被改坏，先炸这里。
    #[test]
    fn block_map_matches_known_runtime_ids() {
        assert_eq!(bedrock_block_state(0), AIR_STATE); // air
        assert_eq!(bedrock_block_state(1), 2403); // stone
        assert_eq!(bedrock_block_state(519), 11468); // glass
        assert_eq!(bedrock_block_state(5863), 5588); // glowstone
    }

    /// BUG-032 第 33 条：拉杆/按钮这类"有朝向和开关状态"的方块，每个 Java 状态都必须映射到
    /// **不同**的 Bedrock 运行时状态。
    ///
    /// v1 只按方块名取"属性全默认"的那一条，于是拉杆 24 个状态全指向同一个基岩状态：朝向永远
    /// 朝上，点下去也看不出任何变化（看起来就是"点了没反应"）。
    /// 1.20.4 里 lever 的 24 个状态是 5626..=5649。
    #[test]
    fn lever_states_are_not_collapsed() {
        let ids: Vec<i32> = (5626..=5649).map(bedrock_block_state).collect();
        assert!(
            ids.iter().all(|&v| v > 0),
            "每个拉杆状态都要有映射，实际 {ids:?}"
        );
        let mut distinct = ids.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert!(
            distinct.len() >= 8,
            "拉杆至少要映射到 8 种不同的基岩状态，实际 {} 种",
            distinct.len()
        );
        // 同一朝向、只差 powered 的两个状态必须不同，否则"点拉杆"在客户端看不出变化
        assert_ne!(bedrock_block_state(5626), bedrock_block_state(5627));
    }

    /// 越界必须回落到空气，绝不能把 -1 或随机值发给客户端。
    ///
    /// v2 之后 `BLOCK_MAP` 里已经没有 -1 了（26,644 条全部命中，见文件头 `unmapped 0`），
    /// 所以不再要求"表里必须存在未映射项"；但若以后重新生成又出现空洞，也必须回落成空气。
    #[test]
    fn unmapped_falls_back_to_air() {
        assert_eq!(bedrock_block_state(u32::MAX), AIR_STATE);
        assert_eq!(
            bedrock_block_state(block_map::BLOCK_MAP.len() as u32),
            AIR_STATE
        );
        if let Some(unmapped) = block_map::BLOCK_MAP.iter().position(|&v| v < 0) {
            assert_eq!(bedrock_block_state(unmapped as u32), AIR_STATE);
        }
    }

    /// 物品映射：拿 Geyser 的 Bedrock 物品表对出来的两条（stone 1→1、glowstone 89→331），
    /// 以及"不认识就返回 None"（绝不能默认成某个方块去放）。
    #[test]
    fn item_map_matches_bedrock_ids() {
        assert_eq!(mchprs_item_id(1), Some(1)); // minecraft:stone
        assert_eq!(mchprs_item_id(89), Some(331)); // minecraft:glowstone（Java 物品 id 是 331）
        assert_eq!(mchprs_item_id(0), None); // 空手（空气）不该被当成物品
        assert_eq!(mchprs_item_id(999_999), None);
        assert!(item_map::ITEM_MAP.len() >= 100);
    }
}

//! Bedrock 服务端包的构造与取值。
//!
//! 所有取值都以 Geyser 的实际行为为基准，并用 Geyser 自带的 Cloudburst v766 codec
//! 序列化出的字节做逐字节校验（见文末测试）。**改动这里任何数值前，先跑测试。**

use bedrock::protocol::v662::enums::{DataItemType, PlayerPermissionLevel, CommandPermissionLevel};
use bedrock::protocol::v685::enums::ActorDataIDs;
use bedrock::protocol::v662::types::{
    DataItem, PropertySyncData, SerializedAbilitiesData, SerializedAbilitiesLayer, SerializedLayer,
};
use bedrock::protocol::v662::packets::{SetActorDataPacket, UpdateAbilitiesPacket};
use bedrock::protocol::v662::types::ActorRuntimeID;
use bedrock::protocol::V766;

/// 本项目固定使用的协议类型别名（766 = 1.21.51）。
pub type Protocol = V766;

/// 玩家的运行时实体 ID。服务端只允许一个连接，固定为 1 与 `StartGame` 保持一致。
pub const PLAYER_RUNTIME_ID: u64 = 1;

// ---- 实体标志位 ----
// 位号不是数枚举序号得出的，而是让 Geyser 自带的 v766 codec 把每个 `EntityFlag`
// 单独序列化、再反解置位位置得到的（工具脚本见 `docs/bedrock-integration.md`）。
// `FLAGS` 用元数据 id 0，`FLAGS_2` 用 id 92，两者都是 Int64（无符号 varint）。
const FLAG_CAN_SHOW_NAME: u64 = 1 << 15;
const FLAG_SILENT: u64 = 1 << 18;
const FLAG_CAN_CLIMB: u64 = 1 << 20;
const FLAG_BREATHING: u64 = 1 << 36;
const FLAG_HAS_COLLISION: u64 = 1 << 49;
const FLAG_HAS_GRAVITY: u64 = 1 << 50;
const FLAG2_HIDDEN_WHEN_INVISIBLE: u64 = 1 << 26;

// ---- 能力位 ----
// 同上，由 `refs/ProbeCreativeAbilities.java` 用 codec 逐项反解。
const ABILITY_BUILD: u32 = 1 << 0;
const ABILITY_MINE: u32 = 1 << 1;
const ABILITY_DOORS_AND_SWITCHES: u32 = 1 << 2;
const ABILITY_OPEN_CONTAINERS: u32 = 1 << 3;
const ABILITY_ATTACK_PLAYERS: u32 = 1 << 4;
const ABILITY_ATTACK_MOBS: u32 = 1 << 5;
const ABILITY_TELEPORT: u32 = 1 << 7;
const ABILITY_INVULNERABLE: u32 = 1 << 8;
const ABILITY_FLYING: u32 = 1 << 9;
const ABILITY_MAY_FLY: u32 = 1 << 10;
const ABILITY_INSTABUILD: u32 = 1 << 11;

/// 后端声明“支持全部能力”（19 位），与 Geyser 的发法一致。
pub const ABILITY_SET: u32 = 0x0007_FFFF;

/// 创造模式 + 允许飞行 + 正在飞行 + 无限资源。
///
/// 本服务端只支持创造/旁观（见 `crates/core`），所以直接按创造模式下发。
pub const ABILITY_VALUES: u32 = ABILITY_BUILD
    | ABILITY_MINE
    | ABILITY_DOORS_AND_SWITCHES
    | ABILITY_OPEN_CONTAINERS
    | ABILITY_ATTACK_PLAYERS
    | ABILITY_ATTACK_MOBS
    | ABILITY_TELEPORT
    | ABILITY_INVULNERABLE
    | ABILITY_FLYING
    | ABILITY_MAY_FLY
    | ABILITY_INSTABUILD;

/// 玩家实体的初始元数据。
///
/// 等价于 Geyser 的 `Entity.initializeMetadata()` + `SessionPlayerEntity.setAirSupply(getMaxAir())`。
///
/// **为什么必须发**：Bedrock 1.21 起，`FLAGS` 里缺 `BREATHING` 位会让客户端把氧气条画成空的，
/// 并且移动速度明显变慢（Geyser 源码注释原话：
/// “Seemingly required to be sent as of Bedrock 1.21. Otherwise, bubbles will appear as empty”）。
pub fn player_metadata() -> Vec<DataItem<Protocol>> {
    let flags = FLAG_CAN_SHOW_NAME
        | FLAG_SILENT
        | FLAG_CAN_CLIMB
        | FLAG_BREATHING
        | FLAG_HAS_COLLISION
        | FLAG_HAS_GRAVITY;
    let flags2 = FLAG2_HIDDEN_WHEN_INVISIBLE;
    vec![
        DataItem {
            data_item_id: ActorDataIDs::Reserved038, // SCALE
            data_item_type: DataItemType::Float(1.0),
        },
        DataItem {
            data_item_id: ActorDataIDs::ColorIndex, // COLOR
            data_item_type: DataItemType::Byte(0),
        },
        DataItem {
            data_item_id: ActorDataIDs::AirSupplyMax,
            data_item_type: DataItemType::Short(300),
        },
        DataItem {
            data_item_id: ActorDataIDs::AirSupply,
            data_item_type: DataItemType::Short(300),
        },
        DataItem {
            data_item_id: ActorDataIDs::Reserved054, // HEIGHT
            data_item_type: DataItemType::Float(1.8),
        },
        DataItem {
            data_item_id: ActorDataIDs::Reserved053, // WIDTH
            data_item_type: DataItemType::Float(0.6),
        },
        DataItem {
            data_item_id: ActorDataIDs::Reserved0, // FLAGS
            data_item_type: DataItemType::Int64(flags),
        },
        DataItem {
            data_item_id: ActorDataIDs::Reserved092, // FLAGS_2
            data_item_type: DataItemType::Int64(flags2),
        },
    ]
}

/// 玩家初始元数据包（`SetActorData`）。
pub fn set_actor_data() -> SetActorDataPacket<Protocol> {
    SetActorDataPacket {
        target_runtime_id: ActorRuntimeID(PLAYER_RUNTIME_ID),
        actor_data: player_metadata(),
        synced_properties: PropertySyncData {
            int_entries_list: vec![],
            float_entries_list: vec![],
        },
        tick: 0,
    }
}

/// 能力包（`UpdateAbilities`），创造模式 + 飞行 + 无限资源。
pub fn update_abilities() -> UpdateAbilitiesPacket<Protocol> {
    UpdateAbilitiesPacket {
        data: SerializedAbilitiesData {
            target_player_raw_id: PLAYER_RUNTIME_ID as i64,
            player_permissions: PlayerPermissionLevel::Member,
            command_permissions: CommandPermissionLevel::Any,
            layers: vec![SerializedLayer {
                serialized_layer: SerializedAbilitiesLayer::Base,
                abilities_set: ABILITY_SET,
                ability_values: ABILITY_VALUES,
                fly_speed: 0.05,
                walk_speed: 0.1,
            }],
        },
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

    /// 参考字节来自 Geyser 自带的 Cloudburst `Bedrock_v766.CODEC`：
    /// `refs/ProbeMeta.java` 与 `refs/ProbeCreativeAbilities.java`。
    ///
    /// 这两条断言是这次接入最贵的知识：一条守住 `BREATHING` 标志位与
    /// “LONG 型元数据是无符号 varint”这个上游坑（我们为此打了 PATCH-005），
    /// 一条守住创造模式能力位。
    #[test]
    fn set_actor_data_matches_geyser_codec() {
        assert_eq!(
            hex(&set_actor_data()),
            "010826030000803f0300002a012c0107012c0136036666e63f35039a99193f\
             00078080d280808280035c0780808020000000"
        );
    }

    #[test]
    fn update_abilities_matches_geyser_codec() {
        assert_eq!(
            hex(&update_abilities()),
            "01000000000000000100010100ffff0700bf0f0000cdcc4c3dcdcccc3d"
        );
    }
}

//! Bedrock 服务端包的构造与取值。
//!
//! 所有取值都以 Geyser 的实际行为为基准，并用 Geyser 自带的 Cloudburst v766 codec
//! 序列化出的字节做逐字节校验（见文末测试）。**改动这里任何数值前，先跑测试。**

use bedrock::protocol::v662::enums::{
    CommandPermissionLevel, DataItemType, GameType, PlayerPermissionLevel, SpawnPositionType,
};
use bedrock::protocol::v685::enums::ActorDataIDs;
use bedrock::protocol::v662::types::{
    DataItem, PropertySyncData, SerializedAbilitiesData, SerializedAbilitiesLayer, SerializedLayer,
};
use bedrock::protocol::v729::packets::{AttributeData, UpdateAttributesPacket};
use bedrock::protocol::v662::packets::{
    MovePlayerPacket, OpenSignPacket, SetActorDataPacket, SetPlayerGameTypePacket,
    UpdateAbilitiesPacket, UpdateBlockPacket, SetSpawnPositionPacket,
};
use bedrock::protocol::v662::types::ActorRuntimeID;
use bedrock::protocol::v662::types::NetworkBlockPosition;
use bedrock::protocol::v662::enums::PlayerPositionMode;
use bedrock::protocol::V766;

/// 本项目固定使用的协议类型别名（766 = 1.21.51）。
pub type Protocol = V766;

/// 玩家实体的默认运行时 ID（**只用于测试与单连接场景的历史默认值**）。
///
/// 真正上线时每个连接都会分配一个（`session::next_runtime_id()`，BUG-032 第 22 条）：
/// 写死 1 会让"服务端位置纠正"和将来的多玩家互通全部撞号。
pub const PLAYER_RUNTIME_ID: u64 = 1;

// ---- 实体标志位 ----
// 位号不是数枚举序号得出的，而是让 Geyser 自带的 v766 codec 把每个 `EntityFlag`
// 单独序列化、再反解置位位置得到的（工具脚本见 `docs/bedrock-integration.md`）。
// `FLAGS` 用元数据 id 0，`FLAGS_2` 用 id 92，两者都是 Int64（无符号 varint）。

/// 客户端**低位区**的 `FLAGS` 位号相对 Cloudburst v766 `EntityFlag` 序号的偏移。
///
/// **真机实测（2026-11-01，两轮才定死：BUG-032 第 38 条 + 第 42 条）**：
///
/// | 位 | codec 说 | 客户端实际 | 结论 |
/// | --- | --- | --- | --- |
/// | `1<<1` | `SNEAKING` | **着火** | 客户端把 bit1 当 `ON_FIRE` |
/// | `1<<2` | `RIDING` | **正常蹲下** | 客户端的 `SNEAKING` 是 bit2 |
/// | `1<<4` | `INVISIBLE` | **正常疾跑** | 客户端的 `SPRINTING` 是 bit4 |
/// | `1<<15` | `CAN_SHOW_NAME` | 名字正常显示 | **没平移** |
/// | `1<<50` | `HAS_GRAVITY` | 降落正常 | **没平移** |
///
/// 所以**不是整体平移**：客户端只是在 `ON_FIRE` 之后多了一个标志（低位区错开一位），
/// 高位的名字/静音/爬梯/呼吸/碰撞/重力与 codec 完全一致。
///
/// **教训（第 42 条）**：一开始按"整体 +1"改，把 `HAS_GRAVITY` 挪到 bit51，
/// 真机立刻表现成"降落速度非常慢"（客户端认为该实体没有重力）。改位号必须一位一位验。
const CLIENT_LOW_FLAG_SHIFT: u32 = 1;

/// 低位标志（潜行 / 疾跑）的客户端位号：codec 序号 + 偏移。
const fn low_flag_bit(codec_index: u32) -> u32 {
    codec_index + CLIENT_LOW_FLAG_SHIFT
}

// 高位标志（名字/静音/爬梯/呼吸/碰撞/重力）**按 codec 原序号**，不平移。
const FLAG_CAN_SHOW_NAME: u64 = 1 << 15;
const FLAG_SILENT: u64 = 1 << 18;
const FLAG_CAN_CLIMB: u64 = 1 << 20;
const FLAG_BREATHING: u64 = 1 << 36;
const FLAG_HAS_COLLISION: u64 = 1 << 49;
const FLAG_HAS_GRAVITY: u64 = 1 << 50;
/// `FLAGS_2`（元数据 id 92）是**另一张表**，没有证据表明也错位，保持 codec 原值。
const FLAG2_HIDDEN_WHEN_INVISIBLE: u64 = 1 << 26;

/// `FLAGS` 里"所有玩家都该有的那几位"（名字、静音、爬梯、呼吸、碰撞、重力）。
///
/// 单独提出来是因为 A3 要按玩家拼潜行/疾跑位：`FLAGS` 是**整份覆盖**的，
/// 不是增量，每次改一位都得把基础位一起发。
pub const PLAYER_BASE_FLAGS: u64 = FLAG_CAN_SHOW_NAME
    | FLAG_SILENT
    | FLAG_CAN_CLIMB
    | FLAG_BREATHING
    | FLAG_HAS_COLLISION
    | FLAG_HAS_GRAVITY;

/// 当前生效的 `(潜行位, 疾跑位)`（日志用）。
pub const fn flag_bits() -> (u32, u32) {
    (low_flag_bit(1), low_flag_bit(3))
}

/// 一个玩家的 `FLAGS` 值：基础位 + 潜行/疾跑（A3）。
pub fn player_flags(sneaking: bool, sprinting: bool) -> u64 {
    let (sneak_bit, sprint_bit) = flag_bits();
    PLAYER_BASE_FLAGS
        | if sneaking { 1u64 << sneak_bit } else { 0 }
        | if sprinting { 1u64 << sprint_bit } else { 0 }
}

/// 玩家站立时的碰撞箱（Geyser `EntityDefinitions.PLAYER`：高 1.8、宽 0.6）。
pub const PLAYER_HEIGHT: f32 = 1.8;
pub const PLAYER_WIDTH: f32 = 0.6;
/// 潜行时的碰撞箱高度（Geyser `GeyserSession#setSneakingPose` → `1.5f`）。
pub const PLAYER_SNEAKING_HEIGHT: f32 = 1.5;

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

/// 创造模式 + 允许飞行 + 无限资源，**但不在飞行**。
///
/// 本服务端只支持创造/旁观（见 `crates/core`），所以直接按创造模式下发。
///
/// **`FLYING`(bit9) 不能预置**：置了就等于告诉客户端"你正在飞"，客户端不施加重力，
/// 新玩家会悬在出生点下不来（BUG-032 第 25 条，真机实测悬停 8 秒）。
/// Geyser 的 `sendAbilities` 也只在 `entity.isFlying()` 为真时才加这一位；
/// 跟随后端真实飞行状态要用 [`update_abilities_for`]（等出站通道接上再做）。
pub const ABILITY_VALUES: u32 = ABILITY_BUILD
    | ABILITY_MINE
    | ABILITY_DOORS_AND_SWITCHES
    | ABILITY_OPEN_CONTAINERS
    | ABILITY_ATTACK_PLAYERS
    | ABILITY_ATTACK_MOBS
    | ABILITY_TELEPORT
    | ABILITY_INVULNERABLE
    | ABILITY_MAY_FLY
    | ABILITY_INSTABUILD;

/// 玩家的能力值：`flying` 决定是否包含 `FLYING` 位。
pub fn ability_values(flying: bool) -> u32 {
    if flying {
        ABILITY_VALUES | ABILITY_FLYING
    } else {
        ABILITY_VALUES
    }
}

/// 玩家实体的初始元数据。
///
/// 等价于 Geyser 的 `Entity.initializeMetadata()` + `SessionPlayerEntity.setAirSupply(getMaxAir())`。
///
/// **为什么必须发**：Bedrock 1.21 起，`FLAGS` 里缺 `BREATHING` 位会让客户端把氧气条画成空的，
/// 并且移动速度明显变慢（Geyser 源码注释原话：
/// “Seemingly required to be sent as of Bedrock 1.21. Otherwise, bubbles will appear as empty”）。
pub fn player_metadata() -> Vec<DataItem<Protocol>> {
    player_metadata_with_flags(false, false)
}

/// 同 [`player_metadata`]，但 `FLAGS` 里带上潜行/疾跑位（A3：生成别人时就要对）。
pub fn player_metadata_with_flags(sneaking: bool, sprinting: bool) -> Vec<DataItem<Protocol>> {
    let flags = player_flags(sneaking, sprinting);
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
            // 潜行时 1.5（Geyser `setDimensions(Pose.SNEAKING)`），站立 1.8。
            data_item_type: DataItemType::Float(if sneaking {
                PLAYER_SNEAKING_HEIGHT
            } else {
                PLAYER_HEIGHT
            }),
        },
        DataItem {
            data_item_id: ActorDataIDs::Reserved053, // WIDTH
            data_item_type: DataItemType::Float(PLAYER_WIDTH),
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

/// 玩家初始元数据包（`SetActorData`）。`runtime_id` 是本连接自己的实体运行时 id。
pub fn set_actor_data(runtime_id: u64) -> SetActorDataPacket<Protocol> {
    SetActorDataPacket {
        target_runtime_id: ActorRuntimeID(runtime_id),
        actor_data: player_metadata(),
        synced_properties: PropertySyncData {
            int_entries_list: vec![],
            float_entries_list: vec![],
        },
        tick: 0,
    }
}

/// 只更新**别的一个玩家**的姿态（潜行 / 疾跑，BUG-032 第 38 条）。
///
/// 这里发的是**和生成时同一整套元数据**（不是只发 `FLAGS`），原因有两条：
///
/// 1. Geyser 改姿态是 `setFlag(...)` **加** `setDimensions(pose)`：潜行要把碰撞箱
///    从 1.8×0.6 改成 **1.5×0.6**（`Entity#setPose` → `setDimensions`）；
/// 2. 真机反馈"Java 玩家一蹲下，基岩端看到他在着火"——只发 `FLAGS` 时，客户端手里
///    只有那一条，其余字段的旧值/默认值组合可能触发意想不到的渲染分支。整套重发
///    把"元数据是不是残缺"这个变量彻底排除掉（一次切换才几十字节，频率是"玩家按一下
///    蹲下"，不值得为省字节冒风险）。
pub fn flags_actor_data(
    runtime_id: u64,
    sneaking: bool,
    sprinting: bool,
) -> SetActorDataPacket<Protocol> {
    let mut actor_data = player_metadata_with_flags(sneaking, sprinting);
    actor_data.push(DataItem {
        data_item_id: ActorDataIDs::NametagAlwaysShow,
        data_item_type: DataItemType::Byte(1),
    });
    SetActorDataPacket {
        target_runtime_id: ActorRuntimeID(runtime_id),
        actor_data,
        synced_properties: PropertySyncData {
            int_entries_list: vec![],
            float_entries_list: vec![],
        },
        tick: 0,
    }
}

/// 游戏模式变化（A5）：Bedrock 的 `SetPlayerGameType`。
///
/// 取值照 Geyser `JavaGameEventTranslator`（CHANGE_GAMEMODE 分支）：
/// 只发这一个包；Geyser 之后还会 `sendAdventureSettings()`，我们本来就在登录时
/// 把能力值按创造模式发足了，这里不重复发（改模式不会改能力位）。
pub fn set_game_mode(mode: crate::bedrock::bridge::BedrockGameMode) -> SetPlayerGameTypePacket<Protocol> {
    use crate::bedrock::bridge::BedrockGameMode;
    SetPlayerGameTypePacket {
        player_game_type: match mode {
            BedrockGameMode::Creative => GameType::Creative,
            BedrockGameMode::Spectator => GameType::Spectator,
        },
    }
}

/// 能力值（`SerializedAbilitiesData`）：玩家自己的 `UpdateAbilities` 和
/// 别的玩家的 `AddPlayer` 共用这一份，保证两边权限/速度口径一致。
pub fn abilities_data(runtime_id: u64, flying: bool, fly_speed: f32) -> SerializedAbilitiesData<Protocol> {
    SerializedAbilitiesData {
        target_player_raw_id: runtime_id as i64,
        player_permissions: PlayerPermissionLevel::Member,
        command_permissions: CommandPermissionLevel::Any,
        layers: vec![SerializedLayer {
            serialized_layer: SerializedAbilitiesLayer::Base,
            abilities_set: ABILITY_SET,
            ability_values: ability_values(flying),
            fly_speed,
            walk_speed: 0.1,
        }],
    }
}

/// 能力包（`UpdateAbilities`），创造模式 + 无限资源，飞行位按参数给。
///
/// `flying = false` 是新玩家进世界的默认值（否则会悬空，见 [`ABILITY_VALUES`]）。
/// `fly_speed` 由 core 侧的 `Player::fly_speed` 倍率乘出来（Java 是 `0.05 * 倍率`），
/// 以前这里写死 0.05，Bedrock 玩家的飞行速度就永远跟 Java 不一样。
pub fn update_abilities_for(
    flying: bool,
    runtime_id: u64,
    fly_speed: f32,
) -> UpdateAbilitiesPacket<Protocol> {
    UpdateAbilitiesPacket {
        data: abilities_data(runtime_id, flying, fly_speed),
    }
}

/// 玩家初始能力包：创造模式 + 允许飞行，**不在飞行**，单连接默认参数。
pub fn update_abilities() -> UpdateAbilitiesPacket<Protocol> {
    update_abilities_for(false, PLAYER_RUNTIME_ID, 0.05)
}

/// 移动速度属性包（`minecraft:movement = 0.1`）。
///
/// **必须用本条连接自己的 `runtime_id`**（BUG-032 第 31 条）：
/// 客户端靠这个属性决定走路速度与 FOV。目标实体写错（例如写死 1）时，第二次连接
/// （runtime id = 2）收不到属性，客户端会"以离谱的速度移动"：实测 30~39 m/s、视场角变宽。
/// 抽成 builder 就是为了让这个参数**必须显式传入**，并由下面的字节级测试盯住。
pub fn movement_attributes(runtime_id: u64) -> UpdateAttributesPacket<Protocol> {
    UpdateAttributesPacket::<Protocol> {
        target_runtime_id: bedrock::protocol::v662::types::ActorRuntimeID(runtime_id),
        attribute_list: vec![AttributeData {
            min_value: 0.0,
            max_value: 1024.0,
            current_value: 0.1,
            default_min: 0.0,
            default_max: 1024.0,
            default_value: 0.1,
            attribute_name: String::from("minecraft:movement"),
            attribute_modifiers: vec![],
        }],
        ticks_since_sim_started: 0,
    }
}

/// 单个方块更新的 `flags`：`NEIGHBORS | NETWORK`。
///
/// 与 Geyser `Block.sendBlockUpdatePacket` 一致（`blockPacket.getFlags().add(NEIGHBORS); .add(NETWORK);`）。
/// **`NETWORK` 位不能省**：缺了它客户端不会重绘这个方块，等于没发。
pub const UPDATE_BLOCK_FLAGS: u32 = 0b11;

/// 单个方块更新（BUG-032 第 3 条）。
///
/// `position` 是 **Bedrock 坐标**（调用方已经做 `y - chunk::Y_OFFSET`），
/// `bedrock_runtime_id` 由 `crate::bedrock::bedrock_block_state` 换算；两者都由网络层负责，
/// core 只传 MCHPRS 的坐标与状态 id。
pub fn block_update(
    position: (i32, i32, i32),
    bedrock_runtime_id: u32,
) -> UpdateBlockPacket<Protocol> {
    UpdateBlockPacket {
        block_position: NetworkBlockPosition {
            x: position.0,
            // Bedrock 的 y 允许为负（-64..319），按 32 位二补数写无符号 varint
            y: position.1 as u32,
            z: position.2,
        },
        block_runtime_id: bedrock_runtime_id,
        flags: UPDATE_BLOCK_FLAGS,
        layer: 0,
    }
}

/// 打开告示牌编辑界面（BUG-032 第 43 条）：Java `COpenSignEditor` 的 Bedrock 等价物。
///
/// 出处：Geyser `JavaOpenSignEditorTranslator` → `OpenSignPacket{position, frontSide}`。
/// **少了这个包，基岩客户端永远不会弹编辑框**（它不会自己开）—— 真机现象就是
/// "放下牌子后根本不能写字"。以前这条路上 core 只发 Java 包，Bedrock 后端直接丢掉。
///
/// `(x, y, z)` 是 **MCHPRS（Java）坐标**，和 `block_entity::sign_actor_data` 一样由这里
/// 减掉 `Y_OFFSET`；`front` 对应 Java 的 `is_front_text`（MCHPRS 目前只编辑正面）。
pub fn open_sign(x: i32, y: i32, z: i32, front: bool) -> OpenSignPacket<Protocol> {
    OpenSignPacket {
        pos: NetworkBlockPosition {
            x,
            // `NetworkBlockPosition.y` 是 u32，负高度按 32 位二补数写（与 `block_update` 一致）
            y: (y - crate::bedrock::chunk::Y_OFFSET) as u32,
            z,
        },
        is_front: front,
    }
}

/// 服务端强制定位（`MovePlayer` + `Teleport`），对应 Java 的 `CSynchronizePlayerPosition`。
/// 以前 core 侧的 `Player::teleport()` 只发 Java 包，Bedrock 后端直接丢掉 —— 于是
/// `/tp`、传送门、服务端位置纠正对 Bedrock 玩家全部无效（BUG-032 第 8 条）。
/// `position` 是 **Bedrock 坐标**（调用方已 `y - 64`）。
pub fn teleport(
    runtime_id: u64,
    position: (f32, f32, f32),
    rotation: (f32, f32),
) -> MovePlayerPacket<Protocol> {
    MovePlayerPacket {
        player_runtime_id: ActorRuntimeID(runtime_id),
        position,
        rotation,
        y_head_rotation: rotation.1,
        position_mode: PlayerPositionMode::Teleport {
            // `TeleportationCause::Behavior = 4`：Geyser 每次连接都用这个值发出生点传送
            // （2026-10-31 抓包：mode=TELEPORT, teleportationCause=BEHAVIOR）。
            teleportation_cause: 4,
            source_actor_type: 0,
        },
        on_ground: false,
        riding_runtime_id: ActorRuntimeID(0),
        tick: 0,
    }
}

/// 出生/重生的 `MovePlayerPacket`（`mode=RESPAWN`）。
///
/// Geyser 每次连接都**先**发 RESPAWN、**再**发 TELEPORT(BEHAVIOR)，两条的位置和朝向完全相同
/// （BEDROCK.md 第 31 条抓包）。只发 TELEPORT 时客户端不会重置相机状态，重进服务器会
/// 沿用上一局的视场角/移动表现 —— 这就是"第二次进入视场角变大、移动变快"。
pub fn respawn(
    runtime_id: u64,
    position: (f32, f32, f32),
    rotation: (f32, f32),
) -> MovePlayerPacket<Protocol> {
    MovePlayerPacket {
        player_runtime_id: ActorRuntimeID(runtime_id),
        position,
        rotation,
        y_head_rotation: rotation.1,
        position_mode: PlayerPositionMode::Respawn,
        on_ground: false,
        riding_runtime_id: ActorRuntimeID(0),
        tick: 0,
    }
}

/// 世界出生点（`SetSpawnPositionPacket`，BUG-032 第 23 条）。
///
/// Geyser 从来不往 `StartGamePacket` 里写出生点（`defaultSpawnBlockPosition` 保持 Cloudburst
/// 默认值），世界出生点是靠 Java 的 `ClientboundSetDefaultSpawnPositionPacket` →
/// `JavaSetDefaultSpawnPositionTranslator` → 这个包（`Type.WORLD_SPAWN`）下发的。
/// 我们的登录流程里 `StartGame` 早于世界快照，出生点要等快照回来才知道，所以同样用这个包补发。
///
/// `block` 是 **Bedrock 坐标**（调用方已做 `y - 64`）。
///
/// **与 Geyser 的已知差异**：`JavaSetDefaultSpawnPositionTranslator` 把 Java 的方块坐标
/// 原样塞进 `blockPosition`（没做 y-64）。我们按 Bedrock 坐标空间发，因为客户端此时确实
/// 站在 `y-64` 的位置，原样传会让出生点标记高 64 格。这一条记在 BACKLOG BUG-032 第 23 条，
/// 真机上用指南针/地图核对一次就能定论。
pub fn set_spawn_position(block: (i32, i32, i32)) -> SetSpawnPositionPacket<Protocol> {
    let position = NetworkBlockPosition {
        x: block.0,
        // 与 `block_update` 一样：y 按 32 位二补数写无符号 varint
        y: block.1 as u32,
        z: block.2,
    };
    SetSpawnPositionPacket::<Protocol> {
        spawn_position_type: SpawnPositionType::WorldSpawn,
        // `NetworkBlockPosition` 不是 `Copy`，两个字段要用同一份坐标得克隆
        block_position: position.clone(),
        dimension_type: 0,
        // Cloudburst 的 `spawnPosition` 与 `blockPosition` 是两个字段，这里都填同一个值，
        // 免得客户端拿哪个都不一致。（探针确认 `spawnForced` 不进包。）
        spawn_block_pos: position,
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
    /// **基础位与 codec 逐字节一致**（`FLAGS` 那段是 `8080d28080828003`）——因为
    /// 真机验证过 `CAN_SHOW_NAME(15)` 与 `HAS_GRAVITY(50)` 的位置本来就是对的
    /// （名字正常、降落正常）。只有**低位区**的潜行/疾跑错开一位，
    /// 而它们在 `sneaking=false, sprinting=false` 时不会出现在这个包里，
    /// 所以这条测试仍应与 Cloudburst 完全一致（见 BUG-032 第 38/42 条）。
    #[test]
    fn set_actor_data_matches_geyser_codec() {
        assert_eq!(
            hex(&set_actor_data(PLAYER_RUNTIME_ID)),
            "010826030000803f0300002a012c0107012c0136036666e63f35039a99193f\
             00078080d280808280035c0780808020000000"
        );
        // 基础位不能被悄悄平移（第 42 条的教训：挪 HAS_GRAVITY 会让降落变慢）
        let flags = player_metadata()
            .into_iter()
            .find(|d| format!("{:?}", d.data_item_id) == "Reserved0")
            .map(|d| match d.data_item_type {
                DataItemType::Int64(value) => value,
                other => panic!("FLAGS 必须是 Int64，实际 {other:?}"),
            })
            .expect("必须有 FLAGS");
        assert_eq!(flags & (1 << 15), 1 << 15, "CAN_SHOW_NAME 应在 bit15");
        assert_eq!(flags & (1 << 36), 1 << 36, "BREATHING 应在 bit36");
        assert_eq!(flags & (1 << 50), 1 << 50, "HAS_GRAVITY 应在 bit50");
    }

    /// BUG-032 第 38 条：姿态更新必须带上 `SNEAKING` 位**和** 1.5 的碰撞箱高度
    /// （Geyser `setFlag` + `setDimensions(Pose.SNEAKING)` 两步），并且整套元数据
    /// 与生成时一致（不残缺）。
    #[test]
    fn pose_update_carries_sneaking_flags_and_height() {
        let packet = flags_actor_data(7, true, false);
        assert_eq!(packet.target_runtime_id.0, 7);
        // 生成时那 8 条 + NametagAlwaysShow
        assert_eq!(packet.actor_data.len(), player_metadata().len() + 1);

        let field = |name: &str| {
            packet
                .actor_data
                .iter()
                .find(|d| format!("{:?}", d.data_item_id) == name)
                .unwrap_or_else(|| panic!("缺少 {name}"))
        };
        let DataItemType::Int64(flags) = field("Reserved0").data_item_type else {
            panic!("FLAGS 必须是 Int64");
        };
        // 客户端位号 = codec 序号 + 1（见 `CLIENT_FLAG_SHIFT`）：
        // 潜行是 `1<<2`，而 `1<<1`（客户端眼里的 ON_FIRE）绝不能置位。
        assert_eq!(flags & (1 << 2), 1 << 2, "缺 SNEAKING 位（客户端 bit2）");
        assert_eq!(flags & (1 << 1), 0, "客户端 bit1 = ON_FIRE，绝不能置位");
        let DataItemType::Float(height) = field("Reserved054").data_item_type else {
            panic!("HEIGHT 必须是 Float");
        };
        assert!((height - 1.5).abs() < 1e-6, "潜行高度应为 1.5，实际 {height}");
        // 站起来要回到 1.8，并且 FLAGS 里不带 SNEAKING
        let standing = flags_actor_data(7, false, false);
        let DataItemType::Float(height) = standing
            .actor_data
            .iter()
            .find(|d| format!("{:?}", d.data_item_id) == "Reserved054")
            .unwrap()
            .data_item_type
        else {
            panic!("HEIGHT 必须是 Float");
        };
        assert!((height - PLAYER_HEIGHT).abs() < 1e-6);
    }

    /// 能力包：两种飞行状态都要逐字节对齐。    ///
    /// 期望值来自 `refs/ProbeCreativeNoFly.java`（Cloudburst `Bedrock_v766.CODEC`，
    /// 2026-10-31 重新生成）：两份只差 `bf0f` → `bf0d`，即 LE u32 `0x0FBF` → `0x0DBF`，
    /// 正好是 `FLYING`(bit9) 那一位。
    #[test]
    fn update_abilities_matches_geyser_codec() {
        // 不在飞行（新玩家进世界的默认值）
        assert_eq!(
            hex(&update_abilities_for(false, PLAYER_RUNTIME_ID, 0.05)),
            "01000000000000000100010100ffff0700bf0d0000cdcc4c3dcdcccc3d"
        );
        // 正在飞行
        assert_eq!(
            hex(&update_abilities_for(true, PLAYER_RUNTIME_ID, 0.05)),
            "01000000000000000100010100ffff0700bf0f0000cdcc4c3dcdcccc3d"
        );
        // 对外入口就是"不在飞行"那份
        assert_eq!(hex(&update_abilities()), hex(&update_abilities_for(false, PLAYER_RUNTIME_ID, 0.05)));
    }

    /// `CommandRequestPacket` 的解码：喂 Cloudburst 自己编出来的字节，必须能解出来。
    ///
    /// 参考串来自 `refs/ProbeCommandRequest.java`（Cloudburst `Bedrock_v766.CODEC`）：
    /// `command="/tp 150 20 150"`、`origin=PLAYER`、`uuid=01234567-89ab-cdef-0123-456789abcdef`、
    /// `requestId="req-1"`、`internal=false`、`version=52`，共 40 字节。
    ///
    /// **背景**：真机上客户端一输入 `/tp` 就掉线，服务端报
    /// `failed to de/serialize packet CommandRequestPacket (id: 77): failed to fill whole buffer`
    /// （BUG-032 第 29 条）。这条测试就是用来分清"我们结构错"还是"客户端发的与参考不同"。
    #[test]
    fn command_request_decodes_cloudburst_bytes() {
        let raw = "0e2f7470203135302032302031353000efcdab8967452301efcdab8967452301057265712d3100\
                   68";
        let raw = raw.replace(char::is_whitespace, "");
        let bytes: Vec<u8> = (0..raw.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&raw[i..i + 2], 16).expect("hex"))
            .collect();
        let pkt = bedrock::protocol::v662::packets::CommandRequestPacket::<Protocol>::deserialize(
            &mut bytes.as_slice(),
        )
        .expect("Cloudburst 的字节必须能解出来");
        assert_eq!(pkt.command, "/tp 150 20 150");
        assert!(!pkt.internal);
        assert_eq!(pkt.version, 52);
        // 双向：解出来再编回去，必须和 Cloudburst 一字不差（PATCH-006 的验收标准）。
        assert_eq!(hex(&pkt), raw);
    }

    /// 拾取请求（拿物品）：喂 Cloudburst 自己编出来的字节，必须能解出来，且**再编回去一字不差**。
    ///
    /// 参考串来自 `refs/ProbeCraftCreative.java`（Cloudburst `Bedrock_v766.CODEC`）：
    /// `CraftCreative(创意条目 id, 请求次数)` + `Take(64, CREATED_OUTPUT[50] → HOTBAR[0])`。
    ///
    /// **背景**（BUG-032 R5）：vendored `CraftCreative` 原来只有 `creativeItemNetworkId` 一个字段，
    /// 少了解 `numberOfRequestedCrafts`，会把后面的 `TakeAction` 整体读错位（PATCH-007）。
    /// 这里同时钉住两个字段的真实宽度：第二字段 1 字节、`creativeItemNetworkId` 是无符号 varint。
    #[test]
    fn item_stack_request_decodes_cloudburst_bytes() {
        // (原始字节, 期望的创意条目 id, 期望的请求次数)
        let cases: [(&str, u32, u8); 4] = [
            ("0154020e0101004000003200000000000000000000", 1, 1),
            ("0154020e0180004000003200000000000000000000", 1, 128),
            ("0154020e800101004000003200000000000000000000", 128, 1),
            ("0154020eb00d01004000003200000000000000000000", 1712, 1),
        ];
        for (raw, expected_id, expected_crafts) in cases {
            let bytes: Vec<u8> = (0..raw.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&raw[i..i + 2], 16).expect("hex"))
                .collect();
            let pkt =
                bedrock::protocol::v662::packets::ItemStackRequestPacket::<Protocol>::deserialize(
                    &mut bytes.as_slice(),
                )
                .expect("Cloudburst 的字节必须能解出来");
            let request = &pkt.requests[0];
            assert_eq!(request.client_request_id, 42);
            assert_eq!(request.actions.len(), 2);
            match &request.actions[0] {
                // 注意：`V766` 的拾取动作枚举实际用的是 **v712** 那一份（不是 v662），
                // 所以 PATCH-007 真正生效的文件是 `v712/enums/item_stack_request_action_type.rs`。
                bedrock::protocol::v712::enums::ItemStackRequestActionType::CraftCreative {
                    creative_item_network_id,
                    number_of_requested_crafts,
                } => {
                    assert_eq!(*creative_item_network_id, expected_id);
                    assert_eq!(*number_of_requested_crafts, expected_crafts);
                }
                other => panic!("第一个动作应该是 CraftCreative，实际是 {other:?}"),
            }
            // 双向：解出来再编回去，必须和 Cloudburst 一字不差（PATCH-007 的验收标准）。
            assert_eq!(hex(&pkt), raw);
        }
    }

    /// BUG-032 B1（PATCH-008）：`ItemStackResponsePacket` 的参考字节来自
    /// `refs/ProbeItemStackResponse.java`（Cloudburst v766）。
    ///
    /// 两件事一次钉住：
    /// ① **多条目**能解（上游 `deserialize` 用 `read_to_end` 会把整包吃成第一个响应的容器，
    ///    `RESP2` 这条就会只剩 1 个 response、还解错 requestId）；
    /// ② `result(1B) | requestId(varint) | containers(...)` 的顺序与字段宽度，
    ///    通过"解出来再编回去必须一字不差"验证。
    #[test]
    fn item_stack_response_decodes_cloudburst_bytes() {
        // 单条：OK(requestId=7)，容器 INVENTORY，1 个槽 (3,4,5,netId=42,"abc",dur=7,"abcF")
        const RESP1: &str = "01000e011d0001030405540361626304616263460e";
        // 两条：上面那条 + ERROR(requestId=9)，ERROR 后面**没有**容器列表
        const RESP2: &str = "02000e011d0001030405540361626304616263460e0112";

        for (raw, expected_len) in [(RESP1, 1usize), (RESP2, 2usize)] {
            let bytes: Vec<u8> = (0..raw.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&raw[i..i + 2], 16).expect("hex"))
                .collect();
            let pkt = bedrock::protocol::v662::packets::ItemStackResponsePacket::<Protocol>::deserialize(
                &mut bytes.as_slice(),
            )
            .expect("Cloudburst 的字节必须能解出来");
            assert_eq!(
                pkt.responses.len(),
                expected_len,
                "响应条数解错了（run-on 解码的典型症状）"
            );
            assert_eq!(pkt.responses[0].client_request_id, 7);
            if expected_len == 2 {
                assert_eq!(pkt.responses[1].client_request_id, 9, "第二条被吃掉了");
            }
            // 双向：编回去必须和 Cloudburst 一字不差（容器列表也在里面）。
            assert_eq!(hex(&pkt), raw);
        }
    }

    /// 方块更新：期望字节来自 `refs/ProbeItemRequest.java`
    /// （`UpdateBlock(pos=(10,64,20), stone=2403, NEIGHBORS|NETWORK, layer=0)`）。
    /// 逐段对照：`14`=x(10 zigzag)、`40`=y(64 无符号)、`28`=z(20 zigzag)、
    /// `e312`=运行时 id 2403（无符号 varint，不是 zigzag）、`03`=flags、`00`=layer。
    #[test]
    fn block_update_matches_geyser_codec() {
        assert_eq!(
            hex(&block_update((10, 64, 20), 2403)),
            "144028e3120300"
        );
    }

    /// 服务端定位包：没有 Cloudburst 逐字节参考串（Geyser 的传送走另一条路径），
    /// 这里只做"编出来能原样解回去 + 关键字段不丢"的往返检查。
    /// 真正的验收要靠真机（`/tp`）——别把这条当成等价证明。
    #[test]
    fn teleport_roundtrips_through_codec() {
        let pkt = teleport(7, (128.5, 64.0, 128.5), (12.0, 34.0));
        let mut buf = Vec::new();
        pkt.serialize(&mut buf).expect("serialize");
        let back = bedrock::protocol::v662::packets::MovePlayerPacket::<Protocol>::deserialize(
            &mut buf.as_slice(),
        )
        .expect("deserialize");
        assert_eq!(back.player_runtime_id.0, 7);
        assert_eq!(back.position, (128.5, 64.0, 128.5));
        assert_eq!(back.rotation, (12.0, 34.0));
    }

    /// 世界出生点：两段参考字节都来自 `refs/ProbeSetSpawn.java`（Cloudburst `Bedrock_v766.CODEC`）。
    ///
    /// 逐段对照（`028002408002008002408002`）：`02`=WORLD_SPAWN(1 zigzag)、
    /// `8002`=x=128、`40`=y=64（无符号，不是 zigzag）、`8002`=z=128、`00`=维度 0，
    /// 后面再重复一遍同样的三个坐标（`spawnPosition` 字段）。
    #[test]
    fn set_spawn_position_matches_geyser_codec() {
        assert_eq!(
            hex(&set_spawn_position((128, 64, 128))),
            "028002408002008002408002"
        );
        // 负坐标：x/z 是 zigzag、y 是无符号 32 位二补数（-60 → c4ffffff0f）
        assert_eq!(
            hex(&set_spawn_position((-7, -60, 33))),
            "020dc4ffffff0f42000dc4ffffff0f42"
        );
    }

    /// BUG-032 第 31 条：登录出生握手三个包的**字节级**对照。
    ///
    /// 参考字节由 Cloudburst 的 v766 编解码器生成（`refs/ProbeLoginHex.java`），固定值：
    /// 位置 `(100.5, -50.25, 200.75)`、rotation `(0,0)`、runtime entity id `1`、
    /// Respawn 的 runtimeEntityId 为 `0`（Bedrock 原版行为）。
    ///
    /// BUG-032 第 31 条：移动速度属性包的字节对照 + **目标实体不能写死**。
    ///
    /// 参考字节由 Cloudburst v766 生成（`refs/ProbeAttributes.java`）：
    /// runtimeEntityId=1、`minecraft:movement` 0..1024、当前值 0.1、默认 0..1024/0.1、无 modifier。
    #[test]
    fn movement_attributes_bytes_and_target_entity() {
        assert_eq!(
            hex(&movement_attributes(1)),
            "01010000000000008044cdcccc3d0000000000008044cdcccc3d126d696e6563726166743a6d6f76656d656e740000",
            "UpdateAttributes 字节与 Cloudburst 不一致"
        );
        // 目标实体必须是调用方给的 id：第二次连接时它是 2，不能还是 1
        let pkt = movement_attributes(2);
        assert_eq!(pkt.target_runtime_id.0, 2, "目标实体不能被写死");
    }

    /// 字段级对比曾经显示"两边一致"，但客户端行为不同 —— 所以必须比字节。
    #[test]
    fn login_handshake_bytes_match_cloudburst() {
        use bedrock::protocol::v662::enums::PlayerRespawnState;
        use bedrock::protocol::v662::packets::RespawnPacket;
        use bedrock::protocol::v662::types::ActorRuntimeID;

        let pos = (100.5f32, -50.25f32, 200.75f32);

        let respawn_packet = RespawnPacket::<Protocol> {
            position: pos,
            state: PlayerRespawnState::ReadyToSpawn,
            player_runtime_id: ActorRuntimeID(0),
        };
        assert_eq!(
            hex(&respawn_packet),
            "0000c942000049c200c048430100",
            "RespawnPacket(SERVER_READY) 字节对不上"
        );

        assert_eq!(
            hex(&respawn(1, pos, (0.0, 0.0))),
            "010000c942000049c200c0484300000000000000000000000001000000",
            "MovePlayer(RESPAWN) 字节对不上"
        );

        assert_eq!(
            hex(&teleport(1, pos, (0.0, 0.0))),
            "010000c942000049c200c04843000000000000000000000000020000040000000000000000",
            "MovePlayer(TELEPORT, BEHAVIOR) 字节对不上"
        );
    }
}

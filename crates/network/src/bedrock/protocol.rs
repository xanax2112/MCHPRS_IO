//! Bedrock 服务端包的构造与取值。
//!
//! 所有取值都以 Geyser 的实际行为为基准，并用 Geyser 自带的 Cloudburst v766 codec
//! 序列化出的字节做逐字节校验（见文末测试）。**改动这里任何数值前，先跑测试。**

use bedrock::protocol::v662::enums::{DataItemType, PlayerPermissionLevel, CommandPermissionLevel, SpawnPositionType};
use bedrock::protocol::v685::enums::ActorDataIDs;
use bedrock::protocol::v662::types::{
    DataItem, PropertySyncData, SerializedAbilitiesData, SerializedAbilitiesLayer, SerializedLayer,
};
use bedrock::protocol::v729::packets::{AttributeData, UpdateAttributesPacket};
use bedrock::protocol::v662::packets::{
    MovePlayerPacket, SetActorDataPacket, UpdateAbilitiesPacket, UpdateBlockPacket,
    SetSpawnPositionPacket,
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
        data: SerializedAbilitiesData {
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
        },
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

/// 服务端强制定位（`MovePlayer` + `Teleport`），对应 Java 的 `CSynchronizePlayerPosition`。
///
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
/// （BACKLOG.md BUG-032 第 31 条抓包）。只发 TELEPORT 时客户端不会重置相机状态，重进服务器会
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
    /// 这两条断言是这次接入最贵的知识：一条守住 `BREATHING` 标志位与
    /// “LONG 型元数据是无符号 varint”这个上游坑（我们为此打了 PATCH-005），
    /// 一条守住创造模式能力位。
    #[test]
    fn set_actor_data_matches_geyser_codec() {
        assert_eq!(
            hex(&set_actor_data(PLAYER_RUNTIME_ID)),
            "010826030000803f0300002a012c0107012c0136036666e63f35039a99193f\
             00078080d280808280035c0780808020000000"
        );
    }

    /// 能力包：两种飞行状态都要逐字节对齐。
    ///
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

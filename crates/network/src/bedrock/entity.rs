//! 其它玩家在 Bedrock 客户端里的实体：玩家列表 → 生成 → 移动 → 销毁。
//!
//! BUG-032 第 22 条完整版：以前只有服务端知道"线上有谁"，Bedrock 客户端看不到任何
//! 别的玩家（连自己的队友也看不见）。这一模块只负责**构造**这些包，调用时序在
//! `session.rs`，谁在哪由 core 每 tick 推一份快照过来。
//!
//! 口径全部对齐 Geyser（唯一权威，铁律 7）：
//! - 先 `PlayerListPacket::Add`（带皮肤）再 `AddPlayerPacket`
//!   （`JavaPlayerInfoUpdateTranslator` → `PlayerEntity#spawnEntity`）；
//! - 移动用 `MovePlayerPacket`（`PlayerEntity#moveAbsolute`），
//!   **不是** `MoveActorAbsolutePacket`；传送到远处用 `mode = Teleport(cause = unknown)`；
//! - 消失 = `RemoveActorPacket`（按 unique id）+ `PlayerListPacket::Remove`（按 uuid）。
//!
//! 关于 id：Bedrock 的 runtime id 对客户端只是"本地句柄"，每个客户端看到同一个玩家
//! 可以是不同的数字。所以这里不去和服务端全局号对齐，由会话线程自己分配
//! （`session::next_runtime_id()`），只要求**同一条连接内稳定**。

use bedrock::protocol::v662::enums::{BuildPlatform, DataItemType, GameType, PlayerPositionMode};
use bedrock::protocol::v662::packets::{
    AddPlayerPacket, MovePlayerPacket, PlayerListPacket, RemoveActorPacket,
};
use bedrock::protocol::v662::types::{
    ActorRuntimeID, ActorUniqueID, DataItem, NetworkItemStackDescriptor, PropertySyncData,
    SerializedSkin,
};
use bedrock::protocol::v685::enums::ActorDataIDs;
use uuid::Uuid;

use super::bridge::BedrockPlayerInfo;
use super::chunk::Y_OFFSET;
use super::protocol::{abilities_data, player_metadata, Protocol};

/// Bedrock 玩家实体的**眼睛高度**偏移（Geyser `EntityDefinitions.PLAYER.offset()` = 1.62）。
///
/// Bedrock 的两套位置约定**不一样**，这是 ±1.62 格错位的根源（真机实测"Java 看基岩玩家
/// 高 2 格 / 基岩看 Java 玩家低 2 格"）：
///
/// - **客户端上报**（`PlayerAuthInput.player_position`）、**`MovePlayer`**、**`Respawn`**
///   用的是**眼睛高度**（脚 + 1.62）——
///   证据：Geyser `BedrockMovePlayer.java:54`
///   `confirmTeleport(packet.getPosition().sub(0, EntityDefinitions.PLAYER.offset(), 0))`；
///   `PlayerEntity#moveAbsolute` 先 `setPosition()`（内部动作里给玩家加 offset）再发 `this.position`。
/// - **`AddPlayer`** 用的是**脚**——
///   证据：`PlayerEntity#spawnEntity` 里 `addPlayerPacket.setPosition(position.sub(0, offset(), 0))`。
///
/// 所以：入站要 `- 1.62`、`MovePlayer` 要 `+ 1.62`、`AddPlayer` 不要加。
pub const PLAYER_EYE_HEIGHT: f32 = 1.62;

/// Java 坐标 → Bedrock 坐标的**脚**位置（`y - 64`）。**只给 `AddPlayer` 用**。
pub fn bedrock_feet(position: (f64, f64, f64)) -> (f32, f32, f32) {
    (
        position.0 as f32,
        (position.1 - Y_OFFSET as f64) as f32,
        position.2 as f32,
    )
}

/// Java 坐标 → Bedrock 坐标的**眼睛**位置（`y - 64 + 1.62`）。
/// 给 `MovePlayer` / `Respawn` 这类"玩家位置"包用。
pub fn bedrock_eye(position: (f64, f64, f64)) -> (f32, f32, f32) {
    let feet = bedrock_feet(position);
    (feet.0, feet.1 + PLAYER_EYE_HEIGHT, feet.2)
}

/// Java 的 `(yaw, pitch)` → Bedrock 的 `(pitch, yaw)`。
///
/// 这个顺序坑过一次（BUG-032 复查第 1 条：把 `player_rotation` 当 `(yaw, pitch)` 解析，
/// 结果所有朝向都错）。这里统一用一个函数，别在各处手写。
pub fn bedrock_rotation(rotation: (f32, f32)) -> (f32, f32) {
    (rotation.1, rotation.0)
}

/// 空皮肤。
///
/// Geyser 会为每个玩家准备真实皮肤（`SkinManager#buildCachedEntry` → 默认皮肤兜底），
/// 我们没有任何皮肤资源，只能发"空皮肤"让客户端退回默认外观。
/// **这是本轮唯一的已知风险点**：如果真机上别的玩家显示成隐形人，就要在这里换成
/// 内嵌的默认皮肤（64×64 PNG + `geometry.humanoid.custom`）。
fn empty_skin() -> SerializedSkin<Protocol> {
    SerializedSkin {
        skin_id: String::new(),
        play_fab_id: String::new(),
        skin_resource_patch: String::new(),
        skin_image_width: 0,
        skin_image_height: 0,
        skin_image_bytes: Vec::new(),
        animations: Vec::new(),
        cape_image_width: 0,
        cape_image_height: 0,
        cape_image_bytes: Vec::new(),
        geometry_data: String::new(),
        geometry_data_engine_version: String::new(),
        animation_data: String::new(),
        cape_id: String::new(),
        full_id: String::new(),
        arm_size: String::new(),
        skin_color: String::new(),
        persona_pieces: Vec::new(),
        piece_tint_colors: Vec::new(),
        is_premium_skin: false,
        is_persona_skin: false,
        is_persona_cape_on_classic_skin: false,
        is_primary_user: false,
        overrides_player_appearance: false,
    }
}

/// 玩家列表加入一项（必须先于 `AddPlayerPacket` 发，否则客户端不认这个实体）。
pub fn player_list_add(info: &BedrockPlayerInfo, unique_id: i64) -> PlayerListPacket<Protocol> {
    PlayerListPacket::Add {
        add_player_list: vec![bedrock::protocol::v662::packets::AddPlayerListEntry {
            uuid: Uuid::from_u128(info.uuid),
            target_actor_id: ActorUniqueID(unique_id),
            player_name: info.username.clone(),
            xbl_xuid: String::new(),
            platform_chat_id: String::new(),
            // Geyser 对 Java 玩家用的是 `BuildPlatform.UNKNOWN`（0）
            build_platform: BuildPlatform::Unknown(0),
            serialized_skin: empty_skin(),
            is_teacher: false,
            is_host: false,
            is_sub_client: false,
        }],
        // Geyser：`entry.setTrustedSkin(true)`
        is_trusted_skin: vec![true],
    }
}

/// 玩家列表移除一项。
pub fn player_list_remove(uuid: u128) -> PlayerListPacket<Protocol> {
    PlayerListPacket::Remove {
        remove_player_list: vec![Uuid::from_u128(uuid)],
    }
}

/// 生成一个玩家实体。
///
/// 字段取值照抄 Geyser `PlayerEntity#spawnEntity`：
/// - 线上**只有 runtime id**（Cloudburst `AddPlayerSerializer_v557` 实测字段序：
///   uuid → username → runtimeEntityId → platformChatId → position → motion → rotation →
///   hand → gameType → metadata → properties → abilities → links → deviceId → buildPlatform），
///   `uniqueEntityId` 不上线；Geyser 把两者设成同一个 `geyserId`，我们照做（见会话层）；
/// - 游戏模式固定 `Creative`（MCHPRS 只有创造/旁观，而且 StartGame 声明的就是 Creative）；
/// - 权限给 `MEMBER` + `ANY`（和本地玩家一致，见 `protocol::abilities_data`）；
/// - 元数据 = 本地玩家那套 + `NAMETAG_ALWAYS_SHOW = 1`
///   （Geyser 注释："Since 1.20.60, the nametag does not show properly if this is not set"）。
pub fn add_player(info: &BedrockPlayerInfo, runtime_id: u64) -> AddPlayerPacket<Protocol> {
    let mut entity_data = player_metadata();
    entity_data.push(DataItem {
        data_item_id: ActorDataIDs::NametagAlwaysShow,
        data_item_type: DataItemType::Byte(1),
    });
    AddPlayerPacket {
        uuid: Uuid::from_u128(info.uuid),
        player_name: info.username.clone(),
        target_runtime_id: ActorRuntimeID(runtime_id),
        platform_chat_id: String::new(),
        // **`AddPlayer` 用脚**（Geyser `spawnEntity` 里 `sub(offset)`），别用眼睛高度。
        position: bedrock_feet(info.position),
        velocity: (0.0, 0.0, 0.0),
        rotation: bedrock_rotation(info.rotation),
        y_head_rotation: info.rotation.0,
        carried_item: empty_item(),
        player_game_type: GameType::Creative,
        entity_data,
        synced_properties: PropertySyncData {
            int_entries_list: vec![],
            float_entries_list: vec![],
        },
        abilities_data: abilities_data(runtime_id, false, 0.05),
        actor_links: vec![],
        device_id: String::new(),
        build_platform: BuildPlatform::Unknown(0),
    }
}

/// 空手（`NetworkItemStackDescriptor` 的 `id = 0` 表示没有物品）。
fn empty_item() -> NetworkItemStackDescriptor {
    NetworkItemStackDescriptor {
        id: 0,
        stack_size: None,
        aux_value: None,
        net_id_variant: None,
        block_runtime_id: None,
        user_data_buffer: None,
    }
}

/// 别的玩家移动/转头。
///
/// **这里的 y 是眼睛高度**（脚 + 1.62）：Geyser `PlayerEntity#moveAbsolute` 发的是
/// 加过 offset 的内部位置；只有 `AddPlayer` 才发脚（见 [`PLAYER_EYE_HEIGHT`]）。
///
/// `teleported = true` 用于跨大段的位移（`/tp`、坠落纠正），走 `mode = Teleport`，
/// 与 Geyser `PlayerEntity#moveAbsolute` 一致（它只在 teleported 时设 cause = UNKNOWN）。
pub fn move_player(
    runtime_id: u64,
    info: &BedrockPlayerInfo,
    teleported: bool,
) -> MovePlayerPacket<Protocol> {
    let rotation = bedrock_rotation(info.rotation);
    MovePlayerPacket {
        player_runtime_id: ActorRuntimeID(runtime_id),
        position: bedrock_eye(info.position),
        rotation,
        y_head_rotation: info.rotation.0,
        position_mode: if teleported {
            PlayerPositionMode::Teleport {
                teleportation_cause: 0, // TeleportationCause::Unknown
                source_actor_type: 0,
            }
        } else {
            PlayerPositionMode::Normal
        },
        on_ground: info.on_ground,
        riding_runtime_id: ActorRuntimeID(0),
        tick: 0,
    }
}

/// 销毁实体。`unique_id` 必须和 `add_player` 里用的是同一个。
pub fn remove_actor(unique_id: i64) -> RemoveActorPacket<Protocol> {
    RemoveActorPacket {
        target_actor_id: ActorUniqueID(unique_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bedrock::protocol::ProtoCodec;

    fn sample() -> BedrockPlayerInfo {
        BedrockPlayerInfo {
            uuid: 0x0123_4567_89ab_cdef_0123_4567_89ab_cdef,
            username: "liuyitong1".to_string(),
            position: (128.5, 192.0, 128.5),
            rotation: (90.0, 12.0),
            on_ground: true,
        }
    }

    fn roundtrip<T: ProtoCodec + std::fmt::Debug>(packet: &T) -> T {
        let mut buf = Vec::new();
        packet.serialize(&mut buf).expect("serialize");
        T::deserialize(&mut buf.as_slice()).expect("deserialize")
    }

    /// 玩家列表项必须是 u128 → `Uuid` 的**大端**表示（Bedrock 的 UUID 是 16 字节），
    /// 并且 unique id 要能被客户端用来把实体和列表项对上。
    #[test]
    fn player_list_add_carries_uuid_name_and_actor_id() {
        let info = sample();
        let decoded = roundtrip(&player_list_add(&info, 42));
        let PlayerListPacket::Add {
            add_player_list,
            is_trusted_skin,
        } = decoded
        else {
            panic!("expected ADD");
        };
        assert_eq!(add_player_list.len(), 1);
        assert_eq!(is_trusted_skin, vec![true]);
        let entry = &add_player_list[0];
        assert_eq!(entry.uuid, Uuid::from_u128(info.uuid));
        assert_eq!(entry.target_actor_id.0, 42);
        assert_eq!(entry.player_name, "liuyitong1");
        assert_eq!(entry.serialized_skin.skin_image_width, 0);
    }

    /// 生成包：位置必须已经做过 `y - 64`，旋转必须是 `(pitch, yaw)`，
    /// 并且 `runtime id` 用的是**传进来的那个**（第 31 条的教训：不许写死）。
    #[test]
    fn add_player_y_offsets_position_and_keeps_target_ids() {
        let info = sample();
        let decoded = roundtrip(&add_player(&info, 7));
        assert_eq!(decoded.target_runtime_id.0, 7);
        assert_eq!(decoded.uuid, Uuid::from_u128(info.uuid));
        assert_eq!(decoded.player_name, "liuyitong1");
        // MCHPRS y = 192.0 → Bedrock y = 128.0
        assert_eq!(decoded.position, (128.5, 128.0, 128.5));
        // Java (yaw, pitch) = (90, 12) → Bedrock (pitch, yaw) = (12, 90)
        assert_eq!(decoded.rotation, (12.0, 90.0));
        assert_eq!(decoded.y_head_rotation, 90.0);
        // `NAMETAG_ALWAYS_SHOW = 81`（v685 元数据 id；没设的话 1.20.60+ 不显示名字）。
        // `ActorDataIDs` 既没实现 `PartialEq`、也不是"纯枚举"（带 fallback 变体，
        // 不能 `as u32`），所以只能按 `Debug` 名字比。
        assert!(decoded.entity_data.iter().any(|d| {
            format!("{:?}", d.data_item_id) == "NametagAlwaysShow"
                && matches!(&d.data_item_type, DataItemType::Byte(1))
        }));
    }

    /// 移动包：普通移动 `mode = Normal`，传送 `mode = Teleport(cause = 0)`；
    /// **y 必须是眼睛高度**（脚 + 1.62，Geyser `PlayerEntity#moveAbsolute`）。
    #[test]
    fn move_player_uses_normal_and_teleport_modes() {
        let info = sample();
        let normal = roundtrip(&move_player(9, &info, false));
        assert_eq!(normal.player_runtime_id.0, 9);
        assert!(matches!(normal.position_mode, PlayerPositionMode::Normal));
        assert!(normal.on_ground);
        // MCHPRS y = 192.0 → 脚 = 128.0 → 眼睛 = 129.62
        assert!((normal.position.1 - (128.0 + PLAYER_EYE_HEIGHT)).abs() < 1e-4);

        let teleported = roundtrip(&move_player(9, &info, true));
        match teleported.position_mode {
            PlayerPositionMode::Teleport {
                teleportation_cause,
                ..
            } => assert_eq!(teleportation_cause, 0),
            other => panic!("expected Teleport, got {other:?}"),
        }
    }

    /// `AddPlayer` 与 `MovePlayer` 的 y 差正好是眼睛高度 —— 这是真机上
    /// "互相看差 2 格"的直接回归测试（漏掉任何一边都会差 1.62）。
    #[test]
    fn add_player_uses_feet_while_move_player_uses_eye() {
        let info = sample();
        let add = roundtrip(&add_player(&info, 3));
        let mv = roundtrip(&move_player(3, &info, false));
        assert!((mv.position.1 - add.position.1 - PLAYER_EYE_HEIGHT).abs() < 1e-4);
    }

    #[test]
    fn remove_actor_targets_unique_id() {
        let decoded = roundtrip(&remove_actor(1234));
        assert_eq!(decoded.target_actor_id.0, 1234);
    }

    #[test]
    fn player_list_remove_targets_uuid() {
        let uuid = 0x0123_4567_89ab_cdef_0123_4567_89ab_cdef;
        let decoded = roundtrip(&player_list_remove(uuid));
        let PlayerListPacket::Remove {
            remove_player_list,
        } = decoded
        else {
            panic!("expected REMOVE");
        };
        assert_eq!(remove_player_list, vec![Uuid::from_u128(uuid)]);
    }
}

use bedrock::network::compression::Compression;
use bedrock::network::login::{LoginOptions, NetworkSettings, begin_encryption, check_protocol};
use bedrock::network::tokio::error::ConnectionError;
use bedrock::network::tokio::{Connection, Listener};
use bedrock::protocol::v662::enums::{PacketCompressionAlgorithm, PlayStatus};
use bedrock::protocol::v662::packets::{
    NetworkSettingsPacket, PlayStatusPacket, ServerToClientHandshakePacket,
};
use bedrock::protocol::v662::enums::ResourcePackResponse;
use bedrock::protocol::v662::types::{BaseGameVersion, Experiments};
use bedrock::protocol::v671::packets::ResourcePackStackPacket;
use bedrock::protocol::v766::packets::ResourcePacksInfoPacket;
use uuid::Uuid;
use bedrock::protocol::v662::enums::{
    ChatRestrictionLevel, Difficulty, EditorWorldType, EducationEditionOffer, GamePublishSetting,
    GameType, GeneratorType, PlayerPermissionLevel, ServerAuthMovementMode, SpawnBiomeType,
};
use bedrock::protocol::v662::packets::{
    ChunkRadiusUpdatedPacket, LevelChunkPacket, NetworkChunkPublisherUpdatePacket, RespawnPacket,
    SetTimePacket, StartGamePacket, UpdateBlockPacket,
};
use bedrock::protocol::v662::types::BlockPos;
use bedrock::protocol::v662::types::ItemData;
use bedrock::protocol::v662::enums::{DataItemType, PlayerRespawnState};
use bedrock::protocol::v685::enums::ActorDataIDs;
use bedrock::protocol::v662::packets::{
    SetActorDataPacket,
    AvailableActorIdentifiersPacket, BiomeDefinitionListPacket, CameraPresetsPacket,
    DimensionDataPacket, GameRulesChangedPacket, PlayerListPacket,
    UpdateAbilitiesPacket, UpdateAdventureSettingsPacket,
    AvailableCommandsPacket, CraftingDataPacket, CreativeContentPacket, ItemComponentPacket,
    PlayerHotbarPacket, SetDifficultyPacket, TrimDataPacket, UnlockedRecipesPacket,
};
use bedrock::protocol::v662::enums::ContainerID;
use bedrock::protocol::v662::packets::InteractAction;
use bedrock::protocol::v662::types::{
    AdventureSettings, DimensionDefinition, DimensionDefinitionGroup, DimensionDefinitionGroupType,
};
use bedrock::protocol::v662::types::CameraPresets;
use bedrock::protocol::v662::types::{DataItem, PropertySyncData};
use bedrock::protocol::v766::types::{AudioListener, CameraPreset};
use bedrock::protocol::v662::types::{GameRuleChanged, Type as GameRuleType};
use bedrock::protocol::v662::types::{
    ActorRuntimeID, ActorUniqueID, ChunkPos, EduSharedUriResource, GameRulesChangedPacketData,
    NetworkBlockPosition, NetworkPermissions, SpawnSettings, SyncedPlayerMovementSettings,
};
use bedrock::protocol::v685::types::LevelSettings;
use bedrock::protocol::{ProtoVersion, Unknown, V766};
use std::time::Instant;

use crate::bedrock::bridge::{BedrockCommand, BedrockEvent, BedrockHandle, PlotSnapshot};
use crate::bedrock::chunk as world;
use crate::bedrock::items_gen;
use std::sync::mpsc;

// Geyser 自带的 Bedrock 数据文件（MIT 项目资源），原样内嵌
const BIOME_DEFINITIONS_NBT: &[u8] = include_bytes!("data/biome_definitions.dat");
const ENTITY_IDENTIFIERS_NBT: &[u8] = include_bytes!("data/entity_identifiers.dat");

/// Bedrock 端点的世界数据来源。
///
/// **两个世界是分开的**（用户 2026-10-29 明确要求，见 BACKLOG.md BUG-032）：
/// `Demo` 只是协议层的石头试验台；`Mchprs` 读真实 plot。两者不共用任何世界数据路径。
#[derive(Clone)]
pub enum WorldSource {
    /// 示例用的石头测试世界（`examples/bedrock_server`）。
    Demo,
    /// MCHPRS 真实世界：每次有客户端登录就向服务端线程要一份**只读**快照。
    Mchprs {
        events: mpsc::Sender<BedrockEvent>,
        /// 视距（**区块**为单位），由 `crates/core` 从 `CONFIG.view_distance` 传进来。
        /// 网络层不读 core 的配置，所以它必须随源一起带过来：
        /// `ChunkRadiusUpdated`、publisher 半径、快照区块数**三处同源**（BUG-032 第 16 条）。
        chunk_radius: i32,
    },
}

/// 登录完成后交给上层的信息（玩家化用，见 BACKLOG.md BUG-032）。
pub struct BedrockSessionInfo {
    /// 客户端显示名；服务端据此推导与同名 Java 离线玩家一致的身份。
    pub username: String,
    /// 出生点，**已经是 MCHPRS（Java）坐标**：直接可以赋给 `Player::pos`。
    ///
    /// 换算只在这里做一次（`spawn_pos` 是 Bedrock 坐标，y 比 MCHPRS 少 64）。
    /// 之前把 Bedrock 坐标直接当 MCHPRS 坐标用，服务端会认为玩家在地底 64 格，
    /// 而且退出时的 `player.save()` 会把这个错误位置写进同名 Java 存档
    /// （BACKLOG.md BUG-032 复查第 12 条）。
    pub position: (f64, f64, f64),
}

fn parse_nbt_compound(bytes: &[u8]) -> std::collections::HashMap<String, nbtx::Value> {
    parse_nbt_compound_named(bytes, "NBT")
}

/// 与 [`parse_nbt_compound`] 相同，但失败日志里带上资源名。
///
/// 解析函数本身不知道解的是哪张表，线上只看到"失败了"没法定位，
/// 所以两个调用点各报一次名字（BUG-032 第 46 条质量门 §5.6）。
fn parse_nbt_compound_named(
    bytes: &[u8],
    name: &str,
) -> std::collections::HashMap<String, nbtx::Value> {
    // 走自写的 varint-NBT 读取器：不经过 nbtx/facet 反射，见 `nbt_net` 模块注释。
    match crate::bedrock::nbt_net::read_root_compound(bytes) {
        Ok(map) => map,
        Err(e) => {
            tracing::error!("Bedrock NBT 资源 {name} 解析失败（{} 字节）: {e}", bytes.len());
            std::collections::HashMap::new()
        }
    }
}

type Protocol = V766;
const PROTOCOL: i32 = Protocol::PROTOCOL_VERSION as i32;
const ADDR: &str = "0.0.0.0:19132";

fn camera_preset(
    name: &str,
    inherit_from: &str,
    listener: Option<AudioListener>,
    player_effects: Option<bool>,
) -> CameraPreset<Protocol> {
    CameraPreset::<Protocol> {
        name: name.to_string(),
        inherit_from: inherit_from.to_string(),
        pos_x: None,
        pos_y: None,
        pos_z: None,
        rot_x: None,
        rot_y: None,
        rot_speed: None,
        snap_to_target: None,
        horizontal_rot_limit: None,
        vertical_rot_limit: None,
        continue_targeting: None,
        block_listening_radius: None,
        view_offset: None,
        entity_offset: None,
        radius: None,
        listener,
        player_effects,
        align_target_and_camera_forwards: None,
        aim_assist_preset: None,
    }
}

/// ?Geyser CameraDefinitions.CAMERA_PRESETS 一一对应
fn camera_presets() -> Vec<CameraPreset<Protocol>> {
    vec![
        camera_preset("minecraft:first_person", "", None, None),
        camera_preset("minecraft:free", "", None, None),
        camera_preset("minecraft:third_person", "", None, None),
        camera_preset("minecraft:third_person_front", "", None, None),
        camera_preset(
            "geyser:free_audio",
            "minecraft:free",
            Some(AudioListener::Player),
            Some(false),
        ),
        camera_preset(
            "geyser:free_effects",
            "minecraft:free",
            Some(AudioListener::Camera),
            Some(true),
        ),
        camera_preset(
            "geyser:free_audio_effects",
            "minecraft:free",
            Some(AudioListener::Player),
            Some(true),
        ),
    ]
}

pub(crate) fn pname(p: &Protocol) -> &'static str {
    use V766::*;
    match p {
        RequestNetworkSettingsPacket(_) => "RequestNetworkSettings",
        NetworkSettingsPacket(_) => "NetworkSettings",
        LoginPacket(_) => "Login",
        ServerToClientHandshakePacket(_) => "ServerToClientHandshake",
        ClientToServerHandshakePacket(_) => "ClientToServerHandshake",
        PlayStatusPacket(_) => "PlayStatus",
        ResourcePacksInfoPacket(_) => "ResourcePacksInfo",
        ResourcePackStackPacket(_) => "ResourcePackStack",
        ResourcePackClientResponsePacket(_) => "ResourcePackClientResponse",
        ClientCacheStatusPacket(_) => "ClientCacheStatus",
        DisconnectPacket(_) => "Disconnect",
        RequestChunkRadiusPacket(_) => "RequestChunkRadius",
        ChunkRadiusUpdatedPacket(_) => "ChunkRadiusUpdated",
        PlayerAuthInputPacket(_) => "PlayerAuthInput",
        InteractPacket(_) => "Interact",
        // 交互链路的关键包（以前都落在 "Other" 里看不见）：
        // `PlayerAction` 带 START_ITEM_USE_ON/STOP_ITEM_USE_ON，`Animate` 是挥手动画。
        PlayerActionPacket(_) => "PlayerAction",
        AnimatePacket(_) => "Animate",
        CommandRequestPacket(_) => "CommandRequest",
        MovePlayerPacket(_) => "MovePlayer",
        SubChunkRequestPacket(_) => "SubChunkRequest",
        ClientCacheBlobStatusPacket(_) => "ClientCacheBlobStatus",
        SetLocalPlayerAsInitializedPacket(_) => "SetLocalPlayerAsInitialized",
        _ => "Other",
    }
}

/// 与 tracing 日志对齐用的墙上时间（UTC 秒，带小数）。
///
/// 排查"退出→重进卡住"这类**跨连接**时序问题必须用绝对时间：
/// 单看 `start.elapsed()` 只能说明连接内部过了多久，无法判断两条连接之间空了多长时间。
fn wall_clock() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn log(step: &str, start: Instant, dir: &str, msg: &str) {
    println!(
        "[{step}] t={:.3} {:>8.3} {dir} {msg}",
        wall_clock(),
        start.elapsed().as_secs_f64()
    );
}

fn play_status(status: PlayStatus) -> Protocol {
    Protocol::PlayStatusPacket(Box::new(PlayStatusPacket { status }))
}

// ---- 实体标志位 ----
// 位号不是从枚举序号数出来的，而是让 Geyser 自带的 v766 codec（Cloudburst）把它自己的
// EntityFlag 逐个序列化，再解出置位位置得到的，见 tools/ProbeFlagBits.java?
// FLAGS 用元数据 id 0，FLAGS_2 用 id 92，两者都是 Int64（var 编码）
const FLAG_CAN_SHOW_NAME: u64 = 1 << 15;
const FLAG_SILENT: u64 = 1 << 18;
const FLAG_CAN_CLIMB: u64 = 1 << 20;
const FLAG_BREATHING: u64 = 1 << 36;
const FLAG_HAS_COLLISION: u64 = 1 << 49;
const FLAG_HAS_GRAVITY: u64 = 1 << 50;
const FLAG2_HIDDEN_WHEN_INVISIBLE: u64 = 1 << 26;

/// Geyser `Entity.initializeMetadata()` + `SessionPlayerEntity.setAirSupply(getMaxAir())`
/// 落到的元数据集合。其中 BREATHING 是 1.21 起的硬性要求：
/// Geyser 源码注释原文——“Seemingly required to be sent as of Bedrock 1.21.
/// Otherwise, bubbles will appear as empty"。不发就是空氧气条。
fn player_metadata() -> Vec<DataItem<Protocol>> {
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

async fn recv_logged(
    conn: &mut Connection<Protocol>,
    step: &str,
    start: Instant,
) -> Result<Vec<Protocol>, ConnectionError> {
    let batch = conn.recv().await?;
    for p in &batch {
        let line = format!("{}", pname(p));
        log(step, start, "<<", &line);
    }
    Ok(batch)
}

fn build_level_settings() -> LevelSettings<Protocol> {
    LevelSettings {
        seed: 12345,
        spawn_settings: SpawnSettings {
            spawn_type: SpawnBiomeType::Default,
            user_defined_biome_name: String::from("plains"),
            dimension: 0,
        },
        generator_type: GeneratorType::Overworld,
        game_type: GameType::Creative,
        is_hardcore_enabled: false,
        game_difficulty: Difficulty::Easy,
        // 这里**不需要**写真实出生点：Geyser 也不往 `StartGame` 里写（保持 Cloudburst 默认值），
        // 真实出生点由登录后单独发的 `SetSpawnPositionPacket` 下发（BUG-032 第 23 条）。
        default_spawn_block_position: NetworkBlockPosition { x: 0, y: 0, z: 0 },
        // 协议文档：创造模式下 achievements_disabled=true 会让客户端崩溃
        achievements_disabled: true,
        editor_world_type: EditorWorldType::NonEditor,
        is_created_in_editor: false,
        is_exported_from_editor: false,
        day_cycle_stop_time: 0,
        education_edition_offer: EducationEditionOffer::None,
        education_features_enabled: false,
        education_product_id: String::new(),
        rain_level: 0.0,
        lightning_level: 0.0,
        has_confirmed_platform_locked_content: false,
        multiplayer_enabled: true,
        lan_broadcasting_enabled: false,
        xbox_live_broadcast_setting: GamePublishSetting::Public,
        platform_broadcast_setting: GamePublishSetting::Public,
        commands_enabled: true,
        texture_packs_required: false,
        rule_data: GameRulesChangedPacketData {
            rules_list: vec![GameRuleChanged {
                rule_name: String::from("showcoordinates"),
                can_be_modified_by_player: false,
                rule_type: GameRuleType::Bool(true),
            }],
        },
        experiments: Experiments {
            experiments: vec![],
            ever_toggled: false,
        },
        bonus_chest_enabled: false,
        starting_map_enabled: false,
        player_permissions: PlayerPermissionLevel::Member,
        server_chunk_tick_range: 4,
        locked_behaviour_pack: false,
        locked_resource_pack: false,
        from_locked_template: false,
        use_msa_gamer_tags: false,
        from_template: false,
        has_locked_template_settings: false,
        only_spawn_v1_villagers: false,
        persona_disabled: false,
        custom_skins_disabled: false,
        emote_chat_muted: false,
        base_game_version: BaseGameVersion(Protocol::GAME_VERSION.to_string()),
        limited_world_width: 0,
        limited_world_depth: 0,
        nether_type: false,
        edu_shared_uri_resource: EduSharedUriResource {
            button_name: String::new(),
            link_uri: String::new(),
        },
        // 766 规范里这是普通 bool（1 字节）；bedrock-rs 的类型是 Option<bool>，
        // Some(..) 会多出 1 字节导致后续字段整体错位，用 None 恰好是 1 字节 false。
        override_force_experimental_gameplay: None,
        chat_restriction_level: ChatRestrictionLevel::None,
        disable_player_interactions: false,
        server_id: String::new(),
        world_id: String::new(),
        scenario_id: String::new(),
    }
}

fn build_start_game(runtime_id: u64) -> StartGamePacket<Protocol> {
    StartGamePacket {
        target_actor_id: ActorUniqueID(runtime_id as i64),
        target_runtime_id: ActorRuntimeID(runtime_id),
        actor_game_type: GameType::Creative,
        position: (0.5, 82.0, 0.5),
        rotation: (0.0, 0.0),
        settings: build_level_settings(),
        level_id: String::from("mchprs-spike"),
        level_name: String::from("MCHPRS Spike"),
        template_content_identity: String::new(),
        is_trial: false,
        movement_settings: SyncedPlayerMovementSettings {
            // **必须是 ServerAuthoritative**：我们的入站翻译（`translate.rs`）只处理
            // `PlayerAuthInput` → Java 的 `SSetPlayerPositionAndRotation`。
            // 一旦改成 `ClientAuthoritative`，客户端改发 `MovePlayerPacket`，
            // 而 `MovePlayer` 分支只记日志、不更新服务端位置 —— 服务端眼里玩家永远不动，
            // 交互/放方块全部错位（2026-10-31 实测：进服即异常）。
            //
            // Geyser 同样声明服务端权威，并且会真的做位置纠正；我们不做纠正，
            // 所以"速度/视角漂移"的根因不在这里，别再用这个开关试。
            authority_mode: ServerAuthMovementMode::ServerAuthoritative,
            rewind_history_size: 0,
            server_authoritative_block_breaking: false,
        },
        current_level_time: 6000,
        enchantment_seed: 0,
        block_properties: vec![],
        // 协议要求：必须下发完整物品表，否则手机客户端会出问题
        item_list: items_gen::ITEMS
            .iter()
            .map(|(name, id, component_based)| ItemData {
                name: (*name).to_string(),
                id: *id,
                is_component_based: *component_based,
            })
            .collect(),
        multiplayer_correlation_id: String::from("00000000-0000-0000-0000-000000000001"),
        enable_item_stack_net_manager: false,
        server_version: Protocol::GAME_VERSION.to_string(),
        player_property_data: Default::default(),
        server_block_type_registry_checksum: 0,
        world_template_id: Uuid::nil(),
        server_enabled_client_side_generation: false,
        block_network_ids_are_hashes: false,
        network_permissions: NetworkPermissions {
            server_auth_sound_enabled: false,
        },
    }
}

/// B1 + B2: 协议确认 -> NetworkSettings -> 加密登录
///
/// `runtime_id` 是这条连接自己的实体运行时 id（BUG-032 第 22 条）：
/// 以前写死 1，多连接会撞号，服务端位置纠正也没法对准人。
async fn do_login(
    conn: &mut Connection<Protocol>,
    start: Instant,
    source: &WorldSource,
    runtime_id: u64,
) -> Result<BedrockSessionInfo, Box<dyn std::error::Error + Send + Sync>> {
    // ---------- B1: 协议号 ----------
    let batch = recv_logged(conn, "B1", start).await?;
    let version = batch.into_iter().find_map(|p| match p {
        Protocol::RequestNetworkSettingsPacket(r) => Some(r.client_network_version),
        _ => None,
    });
    let Some(client_version) = version else {
        log("B1", start, "!!", "没有收到 RequestNetworkSettings");
        return Err("expected RequestNetworkSettings".into());
    };
    log(
        "B1",
        start,
        "::",
        &format!("client_protocol={client_version} server_protocol={PROTOCOL}"),
    );
    if let Err(e) = check_protocol(client_version, PROTOCOL) {
        let status = if client_version < PROTOCOL {
            PlayStatus::LoginFailedClientOld
        } else {
            PlayStatus::LoginFailedServerOld
        };
        conn.send(&[play_status(status)]).await?;
        log("B1", start, "!!", "协议不匹配，已回 PlayStatus");
        return Err(e.into());
    }
    log("B1", start, "OK", "协议 766 匹配");

    // 恢复标准压缩（阈值 256）；StartGame 带物品表后约 52KB，必须压缩
    let settings = NetworkSettings::default();
    let algorithm = match settings.compression {
        Compression::Zlib { .. } => PacketCompressionAlgorithm::ZLib,
        Compression::Snappy { .. } => PacketCompressionAlgorithm::Snappy,
        Compression::None => PacketCompressionAlgorithm::None,
    };
    conn.send(&[Protocol::NetworkSettingsPacket(Box::new(
        NetworkSettingsPacket {
            compression_threshold: settings.threshold(),
            compression_algorithm: algorithm,
            client_throttle_enabled: false,
            client_throttle_threshold: 0,
            client_throttle_scalar: 0.0,
        },
    ))])
    .await?;
    log("B1", start, ">>", "NetworkSettings");
    conn.enable_compression(settings.compression);

    // ---------- B2: 登录 + 握手 + 加密 ----------
    let batch = recv_logged(conn, "B2", start).await?;
    let login_packet = batch.into_iter().find_map(|p| match p {
        Protocol::LoginPacket(p) => Some(p),
        _ => None,
    });
    let Some(login_packet) = login_packet else {
        return Err("expected Login".into());
    };
    check_protocol(login_packet.client_network_version, PROTOCOL)?;

    let options = LoginOptions::default().require_authentication(false);
    let login = options.verify(&login_packet.connection_request)?;
    // 玩家化要用：Bedrock 客户端的显示名（与同名 Java 离线玩家是同一身份）。
    let username = login.authentication.identity().display_name.clone();
    log(
        "B2",
        start,
        "::",
        &format!(
            "identity={} xuid={:?}",
            login.authentication.identity().display_name,
            login.authentication.identity().xuid
        ),
    );

    let (token, encryption) = begin_encryption(&login)?;
    conn.send(&[Protocol::ServerToClientHandshakePacket(Box::new(
        ServerToClientHandshakePacket {
            handshake_web_token: token,
        },
    ))])
    .await?;
    log("B2", start, ">>", "ServerToClientHandshake");
    conn.enable_encryption(encryption);

    recv_logged(conn, "B2", start).await?;
    log("B2", start, ">>", "PlayStatus(LoginSuccess)");
    conn.send(&[play_status(PlayStatus::LoginSuccess)]).await?;

    // ---------- B3: 资源包三段 ----------
    let info = ResourcePacksInfoPacket {
        resource_pack_required: false,
        has_addon_packs: false,
        has_scripts: false,
        world_template_uuid: Uuid::nil(),
        world_template_version: String::new(),
        resource_packs: vec![],
    };
    conn.send(&[Protocol::ResourcePacksInfoPacket(Box::new(info))])
        .await?;
    log("B3", start, ">>", "ResourcePacksInfo");

    // 等客户端 HaveAllPacks(3)
    loop {
        let batch = recv_logged(conn, "B3", start).await?;
        let mut done = false;
        for p in batch {
            if let Protocol::ResourcePackClientResponsePacket(r) = p {
                log("B3", start, "::", &format!("response={:?}", r.response));
                if matches!(
                    r.response,
                    ResourcePackResponse::DownloadingFinished
                ) {
                    done = true;
                }
            }
        }
        if done {
            break;
        }
    }

    conn.send(&[Protocol::ResourcePackStackPacket(Box::new(
        ResourcePackStackPacket {
            texture_pack_required: false,
            addon_list: vec![],
            texture_pack_list: vec![],
            base_game_version: BaseGameVersion(Protocol::GAME_VERSION.to_string()),
            experiments: Experiments {
                experiments: vec![],
                ever_toggled: false,
            },
            include_editor_packs: false,
        },
    ))])
    .await?;
    log("B3", start, ">>", "ResourcePackStack");

    loop {
        let batch = recv_logged(conn, "B3", start).await?;
        let mut done = false;
        for p in batch {
            if let Protocol::ResourcePackClientResponsePacket(r) = p {
                log("B3", start, "::", &format!("response={:?}", r.response));
                if matches!(
                    r.response,
                    ResourcePackResponse::ResourcePackStackFinished
                ) {
                    done = true;
                }
            }
        }
        if done {
            log("B3", start, "OK", "资源包阶段完成");
            break;
        }
    }

    // ---------- B4: StartGame ----------
    // 与 Geyser 抓包一致：先发 PlayerList(REMOVE, []) 和维度定义
    conn.send(&[Protocol::PlayerListPacket(Box::new(
        PlayerListPacket::Remove {
            remove_player_list: vec![],
        },
    ))])
    .await?;
    log("B4", start, ">>", "PlayerList(REMOVE, [])");

    conn.send(&[Protocol::DimensionDataPacket(Box::new(
        DimensionDataPacket {
            dimension_definition_group: DimensionDefinitionGroup {
                definitions: vec![DimensionDefinitionGroupType {
                    name: String::from("minecraft:overworld"),
                    dimension_definition: DimensionDefinition {
                        height_max: 384,
                        height_min: -64,
                        generator_type: 5,
                    },
                }],
            },
        },
    ))])
    .await?;
    log("B4", start, ">>", "DimensionData(overworld 384/-64)");

    let start_game = build_start_game(runtime_id);
    conn.send(&[Protocol::StartGamePacket(Box::new(start_game))])
        .await?;
    log("B4", start, ">>", "StartGame");

    // ?Geyser 一致：模式/能力包在 spawn 之后再发（见 C2?

    // ---------- C1: 区块半径握手 ----------
    // **声明的视距必须和实际下发的区块数一致**（BACKLOG.md BUG-032 第 16/26 条）：
    // 声明得比实际发的大，客户端会以为周围都已加载，远处是一片虚空。
    // 半径来源只有一个：core 从 `CONFIG.view_distance` 取来、随 `WorldSource` 传进来。
    // `Demo` 没有配置（示例程序），用它自己的固定值。
    const DEMO_RADIUS: i32 = 8;
    let chunk_radius: i32 = match &source {
        WorldSource::Mchprs { chunk_radius, .. } => (*chunk_radius).max(1),
        WorldSource::Demo => DEMO_RADIUS,
    };

    // 等待 `RequestChunkRadius` 必须有超时：否则客户端不发就永久挂住一个 task（第 20 条）。
    const CHUNK_RADIUS_WAIT: std::time::Duration = std::time::Duration::from_secs(15);
    loop {
        let batch = match tokio::time::timeout(CHUNK_RADIUS_WAIT, recv_logged(conn, "C1", start)).await
        {
            Ok(batch) => batch?,
            Err(_) => {
                return Err(format!(
                    "等待客户端 RequestChunkRadius 超时（{}s）",
                    CHUNK_RADIUS_WAIT.as_secs()
                )
                .into());
            }
        };
        let mut radius = None;
        for p in batch {
            match p {
                Protocol::RequestChunkRadiusPacket(r) => radius = Some(r.chunk_radius),
                Protocol::SetLocalPlayerAsInitializedPacket(_) => {
                    log("C1", start, "::", "客户端自报初始化完成")
                }
                _ => {}
            }
        }
        if let Some(r) = radius {
            log("C1", start, "::", &format!("客户端请求半径 {r}"));
            conn.send(&[Protocol::ChunkRadiusUpdatedPacket(Box::new(
                ChunkRadiusUpdatedPacket {
                    chunk_radius,
                },
            ))])
            .await?;
            log("C1", start, ">>", &format!("ChunkRadiusUpdated({chunk_radius})"));
            break;
        }
    }

    // ---------- C2: 区块数据 ----------
    // Demo：写死的石头地板；Mchprs：向服务端线程要一份真实 plot 的只读快照。
    // 参考 Geyser 的时序：先让客户端进入世界，等它自报初始化完成，再发区块。
    //
    // 注意：这里**不再**用 `chunk_radius`（= 配置视距）算 publisher 半径，
    // 而是用快照里真正发出去的半径 `initial_radius`（见下方），
    // 否则客户端会以为远处已发布而实际没数据（BUG-032 第 27 条）。

    // (chunk_x, chunk_z, 负载, sub_chunk_count) 列表 + 出生点 + **这批区块的半径**
    //
    // 半径必须跟着"实际下发的数据"走：MCHPRS 世界现在只同步发登录那一小圈
    // （`INITIAL_RADIUS_CHUNKS`），其余靠 `BedrockCommand::Chunks` 增量流送。
    // 如果这里还按 `view_distance` 报 publisher 半径，客户端会以为远处已发布而实际没数据
    // —— 正是 BUG-032 第 27 条的坑。
    let (chunk_payloads, spawn_pos, initial_radius): (
        Vec<(i32, i32, Vec<u8>, u32)>,
        (f32, f32, f32),
        i32,
    ) = match source {
        WorldSource::Demo => {
            // 1.21.50 的 block state id（来自 minecraft-data blockStates 索引）
            const STONE_STATE: i32 = 2403;
            const AIR_STATE: i32 = 11996;
            const PLAINS_BIOME: i32 = 1;
            const BLOCK_VERSION: i32 = 18163713; // 1.21.50 的方块调色板版本
            const MIN_SECTION: i8 = -4; // 主世界 -64
            const MAX_SECTION: i8 = 19; // 主世界 319
            const FLOOR_SECTION: i8 = 4; // y = 64..79
            let spec = world::ChunkSpec {
                min_section: MIN_SECTION,
                max_section: MAX_SECTION,
                highest_non_empty: MAX_SECTION, // 全部 24 段都显式发送（含空气）
                floor_section: FLOOR_SECTION,
                stone_state: STONE_STATE,
                air_state: AIR_STATE,
                biome_id: PLAINS_BIOME,
                block_version: BLOCK_VERSION,
                mode: world::Mode::Runtime,
            };
            let data = world::build_payload(&spec);
            let count = world::sub_chunk_count(MIN_SECTION, MAX_SECTION);
            let mut payloads = Vec::new();
            for cx in -chunk_radius..=chunk_radius {
                for cz in -chunk_radius..=chunk_radius {
                    payloads.push((cx, cz, data.clone(), count));
                }
            }
            (payloads, (0.5, 82.0, 0.5), chunk_radius)
        }
        WorldSource::Mchprs { events, .. } => {
            let snapshot_start = Instant::now();
            let (reply, rx) = mpsc::channel::<PlotSnapshot>();
            if events
                .send(BedrockEvent::SnapshotRequest {
                    reply,
                    chunk_radius,
                    // 带上登录身份：plot 会在**发区块之前**决定出生点（有存档就用存档位置），
                    // 避免登录后再补一条一百多格的传送（BUG-032 第 31 条）。
                    username: username.clone(),
                })
                .is_err()
            {
                return Err("MCHPRS 服务端已关闭，无法获取世界快照".into());
            }
            // 同步 recv 放阻塞线程池，避免卡住 tokio worker。
            let received = tokio::task::spawn_blocking(move || {
                rx.recv_timeout(std::time::Duration::from_secs(10))
            })
            .await?;
            let snapshot = received
                .map_err(|_| "等待 MCHPRS 世界快照超时（是否没有 plot 在运行？）")?;
            println!(
                "[C2] t={:.3} 快照已收到（等待 {:.3}s）",
                wall_clock(),
                snapshot_start.elapsed().as_secs_f64()
            );
            let count = world::BEDROCK_SECTION_COUNT as u32;
            let radius = snapshot.chunk_radius;
            let payloads = snapshot
                .chunks
                .into_iter()
                .map(|(cx, cz, data)| (cx, cz, data, count))
                .collect();
            (payloads, snapshot.spawn, radius)
        }
    };

    // publisher 半径（方块）必须**覆盖整块视距**，哪怕区块是分批到的。
    //
    // 这一点踩过一个坑：原来按"登录那一小圈（2 区块 → 45 方块）"报，结果客户端把
    // 落在 publisher 之外的 `LevelChunk` 直接丢掉（它只保留已发布范围内的区块），
    // 后面 publisher 长大了也不会补发那些被丢的区块 → 玩家走到哪里都是空洞。
    // Geyser/原版的做法就是**先发满半径的 publisher，再发区块**，这里保持一致。
    //
    // 半径取值仍是 `floor(r 区块 × 16 × √2)`（第 27 条）：方形被半径 r 的圆完全覆盖。
    let view_radius_blocks: u32 =
        (chunk_radius as f64 * 16.0 * std::f64::consts::SQRT_2).floor() as u32;
    conn.send(&[play_status(PlayStatus::PlayerSpawn)]).await?;
    log("C2", start, ">>", "PlayStatus(PlayerSpawn)");

    // Geyser 原文注释：客户端在收到属性包之前会以离谱的速度移动，必须下发移动速度。
    //
    // **`target_runtime_id` 必须用本条连接自己的 id**（BUG-032 第 31 条的真凶）：
    // `runtime_id` 是全局递增的（第 22 条改的），以前这里写死 1 —— 第一次连接恰好是 1
    // 所以一切正常；第二次连接是 2，属性包发给了不存在的实体 1，客户端收不到
    // `minecraft:movement`，于是"以离谱的速度移动"：实测重进后 30~39 m/s、视场角变宽。
    // 现在统一走 `protocol::movement_attributes()`，参数必须显式传入，防止再写死。
    conn.send(&[Protocol::UpdateAttributesPacket(Box::new(
        crate::bedrock::protocol::movement_attributes(runtime_id),
    ))])
    .await?;
    log("C2", start, ">>", "UpdateAttributes(minecraft:movement=0.1)");

    // 与 Geyser 抓包一致：spawn 之后发 UpdateAdventureSettings + UpdateAbilities
    conn.send(&[Protocol::UpdateAdventureSettingsPacket(Box::new(
        UpdateAdventureSettingsPacket {
            adventure_settings: AdventureSettings {
                no_pvm: false,
                no_mvp: false,
                immutable_world: false,
                show_name_tags: false,
                auto_jump: true,
            },
        },
    ))])
    .await?;
    log("C2", start, ">>", "UpdateAdventureSettings(autoJump=true)");

    conn.send(&[Protocol::UpdateAbilitiesPacket(Box::new(
        build_update_abilities(runtime_id),
    ))])
    .await?;
    log("C2", start, ">>", "UpdateAbilities");

    // 玩家实体元数据：等价于 Geyser 的 Entity.initializeMetadata() + SessionPlayerEntity.setAirSupply()
    conn.send(&[Protocol::SetActorDataPacket(Box::new(
        crate::bedrock::protocol::set_actor_data(runtime_id),
    ))])
    .await?;
    // Bedrock 的"玩家位置"包（`Respawn` / `MovePlayer`）用**眼睛高度**（脚 + 1.62），
    // 只有 `AddPlayer` 用脚 —— 见 `entity::PLAYER_EYE_HEIGHT` 里的 Geyser 证据。
    // 漏掉这一步就是真机上的"基岩看 Java 玩家低 2 格"。
    let spawn_eye = (
        spawn_pos.0,
        spawn_pos.1 + crate::bedrock::entity::PLAYER_EYE_HEIGHT,
        spawn_pos.2,
    );
    // Geyser 登录的第一个出生包：RespawnPacket(SERVER_READY, runtimeEntityId=0)。
    // 见下方三连的说明（BUG-032 第 31 条）。
    conn.send(&[Protocol::RespawnPacket(Box::new(RespawnPacket::<Protocol> {
        // Geyser 注释：Bedrock 原版服务端这里就是 0，不是玩家的 runtime id。
        player_runtime_id: ActorRuntimeID(0),
        position: spawn_eye,
        state: PlayerRespawnState::ReadyToSpawn,
    }))])
    .await?;
    log("C2", start, ">>", "Respawn(SERVER_READY)");

    conn.send(&[Protocol::SetActorDataPacket(Box::new(
        crate::bedrock::protocol::set_actor_data(runtime_id),
    ))])
    .await?;
    log("C2", start, ">>", "SetActorData(玩家初始元数据)");
    println!(
        "  [DBG] SetActorData = {:?}",
        crate::bedrock::protocol::set_actor_data(runtime_id)
    );
    // 出生握手：**顺序照 Geyser 抄**（`JavaPlayerPositionTranslator` 的未 spawned 分支）：
    //   RespawnPacket(SERVER_READY) → SetEntityData → MovePlayer(RESPAWN) → MovePlayer(TELEPORT)
    //
    // Geyser 在登录时发的是这个三连；我们原来只发最后一条 TELEPORT。缺了
    // RespawnPacket + MovePlayer(RESPAWN)，客户端不会完整"重新出生"一次，
    // 相机/视场角会沿用上一局的状态 —— "重进后视场角变大、移动变快"（BUG-032 第 31 条）。
    conn.send(&[
        Protocol::MovePlayerPacket(Box::new(crate::bedrock::protocol::respawn(
            runtime_id, spawn_eye, (0.0, 0.0),
        ))),
        Protocol::MovePlayerPacket(Box::new(crate::bedrock::protocol::teleport(
            runtime_id, spawn_eye, (0.0, 0.0),
        ))),
    ])
    .await?;
    log(
        "C2",
        start,
        ">>",
        &format!(
            "MovePlayer(Respawn+Teleport {:.1},{:.1},{:.1})",
            spawn_eye.0, spawn_eye.1, spawn_eye.2
        ),
    );

    // 世界出生点（BUG-032 第 23 条）：Geyser 的 `StartGame` 不写出生点，而是收到 Java 的
    // `SetDefaultSpawnPosition` 后单独发这个包；我们同理，等快照把出生点算出来再补发。
    // 坐标是 Bedrock 坐标（`spawn_pos` 已经做过 y-64），方块坐标向下取整。
    let spawn_block = (
        spawn_pos.0.floor() as i32,
        spawn_pos.1.floor() as i32,
        spawn_pos.2.floor() as i32,
    );
    conn.send(&[Protocol::SetSpawnPositionPacket(Box::new(
        crate::bedrock::protocol::set_spawn_position(spawn_block),
    ))])
    .await?;
    log(
        "C2",
        start,
        ">>",
        &format!(
            "SetSpawnPosition(WORLD_SPAWN {},{},{})",
            spawn_block.0, spawn_block.1, spawn_block.2
        ),
    );

    // ---- 与 Geyser connect() 同序的一批“状态声明”包 ----
    conn.send(&[Protocol::SetDifficultyPacket(Box::new(SetDifficultyPacket {
        difficulty: 1,
    }))])
    .await?;
    log("C2", start, ">>", "SetDifficulty(1)");

    conn.send(&[Protocol::PlayerHotbarPacket(Box::new(
        PlayerHotbarPacket::<Protocol> {
            selected_slot: 0,
            container_id: ContainerID::INVENTORY,
            should_select_slot: true,
        },
    ))])
    .await?;
    log("C2", start, ">>", "PlayerHotbar(0,0,true)");

    // 护甲纹样表：与 Geyser 一致地发空表（协议合法；只是没有纹样可选）
    conn.send(&[Protocol::TrimDataPacket(Box::new(TrimDataPacket {
        trim_pattern_list: vec![],
        trim_material_list: vec![],
    }))])
    .await?;
    log("C2", start, ">>", "TrimData(空)");

    // 合成表：空表 + clear_recipes=true，等价于“没有任何配方”
    conn.send(&[Protocol::CraftingDataPacket(Box::new(
        CraftingDataPacket::<Protocol> {
            crafting_entries: vec![],
            potion_mixes: vec![],
            container_mixes: vec![],
            material_reducers: vec![],
            clear_recipes: true,
        },
    ))])
    .await?;
    log("C2", start, ">>", "CraftingData(空, clean=true)");

    // 命令树（BUG-032 B3）：Bedrock 的补全/语法校验**完全靠这个包**（客户端不会像 Java
    // 那样发 `SCommandSuggestionsRequest`）。表由 `generators/gen_commands.py` 从 Java 侧
    // `DECLARE_COMMANDS` 生成，保证两侧同源；以前这里是空包，所以基岩端什么都没有。
    let commands = crate::bedrock::commands::login_commands();
    let command_count = commands.commands.len();
    conn.send(&[Protocol::AvailableCommandsPacket(Box::new(commands))])
        .await?;
    log(
        "C2",
        start,
        ">>",
        &format!("AvailableCommands({command_count} 个命令)"),
    );

    conn.send(&[Protocol::UnlockedRecipesPacket(Box::new(
        UnlockedRecipesPacket {
            packet_type: 1, // INITIALLY_UNLOCKED
            unlocked_recipes_list: vec![],
        },
    ))])
    .await?;
    log("C2", start, ">>", "UnlockedRecipes(INITIALLY_UNLOCKED, 空)");

    // ---- 以下顺序与 Geyser GeyserSession.connect() 一致 ----

    // 非 Bedrock 原生物品的组件表。Geyser 只在开启 add-non-bedrock-items 时才有内容；
    // 我们是纯原版方块世界，合法空表。
    conn.send(&[Protocol::ItemComponentPacket(Box::new(
        ItemComponentPacket { items: vec![] },
    ))])
    .await?;
    log("C2", start, ">>", "ItemComponent(空)");

    let biomes = parse_nbt_compound_named(BIOME_DEFINITIONS_NBT, "biome_definitions.dat");
    let biome_count = biomes.len();
    if biome_count > 0 {
        conn.send(&[Protocol::BiomeDefinitionListPacket(Box::new(
            BiomeDefinitionListPacket {
                biome_definition_data: crate::bedrock::nbt_net::NbtCompound::new(biomes),
            },
        ))])
        .await?;
        log(
            "C2",
            start,
            ">>",
            &format!("BiomeDefinitionList({biome_count} 个)"),
        );
    } else {
        log("C2", start, "!!", "生物群系表解析为空，跳过发送");
    }

    let entities = parse_nbt_compound_named(ENTITY_IDENTIFIERS_NBT, "entity_identifiers.dat");
    let entity_count = entities.len();
    if entity_count > 0 {
        conn.send(&[Protocol::AvailableActorIdentifiersPacket(Box::new(
            AvailableActorIdentifiersPacket {
                actor_info_list: crate::bedrock::nbt_net::NbtCompound::new(entities),
            },
        ))])
        .await?;
        log(
            "C2",
            start,
            ">>",
            &format!("AvailableActorIdentifiers({entity_count} 个)"),
        );
    } else {
        log("C2", start, "!!", "实体标识表解析为空，跳过发送");
    }

    // 相机预设：与 Geyser CameraDefinitions 完全一致
    let presets = camera_presets();
    conn.send(&[Protocol::CameraPresetsPacket(Box::new(
        CameraPresetsPacket {
            camera_presets: CameraPresets {
                presets: presets.clone(),
            },
        },
    ))])
    .await?;
    log(
        "C2",
        start,
        ">>",
        &format!("CameraPresets({} 个)", presets.len()),
    );

    // 创造模式物品列表（R1）：MCHPRS 支持的每种方块一条，表由 gen_creative.js 生成。
    // 位置与 Geyser 一致（紧跟在 CameraPresets 之后）。
    let creative = crate::bedrock::creative::creative_content();
    log(
        "C2",
        start,
        ">>",
        &format!("CreativeContent({} 条)", creative.write_entries.len()),
    );
    conn.send(&[Protocol::CreativeContentPacket(Box::new(creative))])
        .await?;

    conn.send(&[Protocol::GameRulesChangedPacket(Box::new(
        GameRulesChangedPacket {
            rules_data: GameRulesChangedPacketData {
                rules_list: vec![
                    GameRuleChanged {
                        rule_name: String::from("naturalregeneration"),
                        can_be_modified_by_player: false,
                        rule_type: GameRuleType::Bool(false),
                    },
                    GameRuleChanged {
                        rule_name: String::from("keepinventory"),
                        can_be_modified_by_player: false,
                        rule_type: GameRuleType::Bool(true),
                    },
                    GameRuleChanged {
                        rule_name: String::from("spawnradius"),
                        can_be_modified_by_player: false,
                        rule_type: GameRuleType::Int(0),
                    },
                    GameRuleChanged {
                        rule_name: String::from("recipesunlock"),
                        can_be_modified_by_player: false,
                        rule_type: GameRuleType::Bool(true),
                    },
                ],
            },
        },
    ))])
    .await?;
    log("C2", start, ">>", "GameRulesChanged");

    let mut initialized = false;
    for _ in 0..200 {
        let batch = match tokio::time::timeout(
            std::time::Duration::from_secs(3),
            recv_logged(conn, "C2", start),
        )
        .await
        {
            Ok(Ok(b)) => b,
            _ => break,
        };
        for p in &batch {
            if matches!(p, Protocol::SetLocalPlayerAsInitializedPacket(_)) {
                initialized = true;
            }
        }
        if initialized {
            break;
        }
    }
    log(
        "C2",
        start,
        "::",
        if initialized {
            "客户端初始化完成，开始发区块"
        } else {
            "未等到初始化包，仍然发区块"
        },
    );

    // publisher 中心用**真实出生点**的方块坐标（以前 `+8` 是拍的，和出生点语义不一致，
    // 见 BUG-032 第 27 条）。
    let publisher_center = BlockPos {
        x: spawn_pos.0 as i32,
        y: spawn_pos.1 as i32,
        z: spawn_pos.2 as i32,
    };
    conn.send(&[Protocol::NetworkChunkPublisherUpdatePacket(Box::new(
        NetworkChunkPublisherUpdatePacket {
            new_view_position: publisher_center.clone(),
            new_view_radius: view_radius_blocks,
            server_built_chunks: vec![],
        },
    ))])
    .await?;
    log(
        "C2",
        start,
        ">>",
        &format!(
            "NetworkChunkPublisherUpdate({},{},{} r={view_radius_blocks} blocks)",
            publisher_center.x, publisher_center.y, publisher_center.z
        ),
    );

    let mut sent = 0usize;
    let mut total_bytes = 0usize;
    for (cx, cz, data, count) in &chunk_payloads {
        conn.send(&[Protocol::LevelChunkPacket(Box::new(LevelChunkPacket {
            chunk_position: ChunkPos { x: *cx, z: *cz },
            dimension_id: 0,
            sub_chunk_count: *count,
            sub_chunk_limit: 0,
            cache_enabled: false,
            cache_blobs: vec![],
            serialized_chunk_data: data.clone(),
        }))])
        .await?;
        sent += 1;
        total_bytes += data.len();
    }
    log(
        "C2",
        start,
        "OK",
        &format!(
            "已发送 {} 个区块（合计 {} 字节，平均 {} 字节；登录快照半径 {} 区块，其余流送）",
            sent,
            total_bytes,
            total_bytes / sent.max(1),
            initial_radius
        ),
    );

    // 红石展示区只在 Demo 世界下发：MCHPRS 世界里不能凭空塞方块（本阶段只读）。
    if matches!(source, WorldSource::Demo) {
    const SHOWCASE: [u32; 8] = [
        5749,  // stone_button: face=floor facing=north powered=false
        5627,  // lever:       face=floor facing=north powered=false
        7418,  // redstone_lamp:  lit=false
        9223,  // redstone_block
        539,   // note_block:  instrument=harp note=0
        4138,  // redstone_wire: 无连接 power=0
        2017,  // piston:      extended=false facing=north
        12551, // observer:    facing=north powered=false
    ];
    let ring: [(i32, i32); 8] = [
        (2, 0),
        (4, 0),
        (6, 0),
        (2, 2),
        (4, 2),
        (6, 2),
        (2, 4),
        (4, 4),
    ];
    for (i, java_id) in SHOWCASE.iter().enumerate() {
        let (dx, dz) = ring[i];
        let runtime_id = crate::bedrock::bedrock_block_state(*java_id) as u32;
        conn.send(&[Protocol::UpdateBlockPacket(Box::new(UpdateBlockPacket {
            block_position: NetworkBlockPosition { x: dx, y: 80, z: dz },
            block_runtime_id: runtime_id,
            flags: 0,
            layer: 0,
        }))])
        .await?;
        log(
            "C2",
            start,
            "OK",
            &format!("展示方块 mchprs_id={java_id} -> bedrock={runtime_id} @({dx},80,{dz})"),
        );
    }
    log("C2", start, "OK", "红石展示区已下发（按钮/拉杆/红石灯/红石块/音符盒/红石线/活塞/侦测器）");
    }

    conn.send(&[Protocol::SetTimePacket(Box::new(SetTimePacket { time: 6000 }))])
        .await?;
    log("C2", start, ">>", "SetTime(6000)");

    Ok(BedrockSessionInfo {
        username,
        // `spawn_pos` 是 Bedrock 坐标（客户端已经传送到那里），交给 MCHPRS 侧前 +64
        // 换回 MCHPRS 坐标，否则服务端会认为玩家在地底 64 格（复查第 12 条）。
        position: (
            spawn_pos.0 as f64,
            spawn_pos.1 as f64 + world::Y_OFFSET as f64,
            spawn_pos.2 as f64,
        ),
    })
}

/// 与 Geyser sendAdventureSettings 等价的能力包（Layer: Base）。
///
/// **不要再在这里写第二份常量**：以前这里有一份独立的 `0x0FBF`，而逐字节回归测试
/// 只盖住 `protocol.rs` 那份 → 上线跑的那份漂了都没人发现（BUG-032「能力值双份定义」）。
/// 现在统一走 [`crate::bedrock::protocol::update_abilities`]，飞行位由它按参数决定。
fn build_update_abilities(runtime_id: u64) -> UpdateAbilitiesPacket<Protocol> {
    crate::bedrock::protocol::update_abilities_for(false, runtime_id, 0.05)
}

fn selftest(mode: &str) {
    use bedrock::protocol::ProtoCodec;

    if mode == "nbt" {
        let b = parse_nbt_compound(BIOME_DEFINITIONS_NBT);
        let e = parse_nbt_compound(ENTITY_IDENTIFIERS_NBT);
        println!("[SELFTEST] biomes={} entities={}", b.len(), e.len());
        for (k, _) in b.iter().take(3) {
            println!("  biome: {k}");
        }
        return;
    }

    if mode == "abilities" {
        let pkt = build_update_abilities(crate::bedrock::protocol::PLAYER_RUNTIME_ID);
        let mut buf = Vec::new();
        match pkt.serialize(&mut buf) {
            Ok(()) => {
                std::fs::create_dir_all("logs").ok();
                std::fs::write("logs/update_abilities.bin", &buf).ok();
                println!("[SELFTEST] abilities {} 字节", buf.len());
            }
            Err(e) => println!("[SELFTEST] ERR abilities: {e}"),
        }
        return;
    }

    if mode == "7" {
        println!("[SELFTEST] StartGame 往返");
        let pkt = build_start_game(crate::bedrock::protocol::PLAYER_RUNTIME_ID);
        let mut buf: Vec<u8> = Vec::new();
        pkt.serialize(&mut buf).expect("serialize");
        println!("[SELFTEST] 序列化 len={}", buf.len());
        let mut cur = std::io::Cursor::new(&buf[..]);
        match StartGamePacket::<Protocol>::deserialize(&mut cur) {
            Ok(back) => {
                let pos = cur.position() as usize;
                println!(
                    "[SELFTEST] 反序列化 OK，已消费 {}/{} 字节，剩余 {}",
                    pos,
                    buf.len(),
                    buf.len() - pos
                );
                let mut buf2: Vec<u8> = Vec::new();
                back.serialize(&mut buf2).expect("re-serialize");
                println!("[SELFTEST] 再序列化长度={} 一致={}", buf2.len(), buf2 == buf);
            }
            Err(e) => println!("[SELFTEST] 反序列化失败: {e}"),
        }
        return;
    }
    if mode == "9" {
        let pkt = build_start_game(crate::bedrock::protocol::PLAYER_RUNTIME_ID);
        let mut buf: Vec<u8> = Vec::new();
        pkt.serialize(&mut buf).expect("serialize");
        let path = std::env::args().nth(2).unwrap_or_else(|| String::from("/tmp/startgame.bin"));
        std::fs::write(&path, &buf).expect("write");
        println!("[SELFTEST] 已写出 {} 字节到 {}", buf.len(), path);
        return;
    }
    if mode == "meta" {
        let pkt = SetActorDataPacket::<Protocol> {
            target_runtime_id: ActorRuntimeID(1),
            actor_data: player_metadata(),
            synced_properties: PropertySyncData {
                int_entries_list: vec![],
                float_entries_list: vec![],
            },
            tick: 0,
        };
        let mut buf: Vec<u8> = Vec::new();
        pkt.serialize(&mut buf).expect("serialize");
        let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
        println!("[SELFTEST] SetActorData len={}", buf.len());
        println!("[HEX] {hex}");
        return;
    }
    if mode == "spawn" {
        // 与 refs/ProbeSpawnPackets.java 的输出逐条对拍（只比负载，不带包 id）
        macro_rules! dump {
            ($n:expr, $p:expr $(,)?) => {{
                let mut b: Vec<u8> = Vec::new();
                let _ = &$p;
                $p.serialize(&mut b).expect("serialize");
                let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
                println!("[SELFTEST] {} ({}B): {hex}", $n, b.len());
            }};
        }

        dump!(
            "SetDifficulty(1)",
            &SetDifficultyPacket { difficulty: 1 },
        );
        dump!(
            "PlayerHotbar(0,0,true)",
            &PlayerHotbarPacket::<Protocol> {
                selected_slot: 0,
                container_id: ContainerID::INVENTORY,
                should_select_slot: true,
            },
        );
        dump!(
            "TrimData(empty)",
            &TrimDataPacket {
                trim_pattern_list: vec![],
                trim_material_list: vec![],
            },
        );
        dump!(
            "UnlockedRecipes(INITIALLY_UNLOCKED, empty)",
            &UnlockedRecipesPacket {
                packet_type: 1,
                unlocked_recipes_list: vec![],
            },
        );
        dump!(
            "CraftingData(empty, clean=true)",
            &CraftingDataPacket::<Protocol> {
                crafting_entries: vec![],
                potion_mixes: vec![],
                container_mixes: vec![],
                material_reducers: vec![],
                clear_recipes: true,
            },
        );
        dump!(
            "ItemComponent(empty)",
            &ItemComponentPacket { items: vec![] },
        );
        dump!(
            "CreativeContent(empty)",
            &CreativeContentPacket::<Protocol> {
                write_entries: vec![],
            },
        );
        dump!(
            "AvailableCommands(empty)",
            &AvailableCommandsPacket::<Protocol> {
                enum_values: vec![],
                sub_command_values: vec![],
                post_fixes: vec![],
                enum_data: vec![],
                chained_sub_command_data: vec![],
                commands: vec![],
                soft_enums: vec![],
                constraints: vec![],
            },
        );
        return;
    }
    if mode == "1" {
        println!("[SELFTEST] 序列化 LevelSettings");
        let s = build_level_settings();
        let mut buf: Vec<u8> = Vec::new();
        match s.serialize(&mut buf) {
            Ok(()) => {
                println!("[SELFTEST] OK LevelSettings len={}", buf.len());
                let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
                println!("[HEX] {hex}");
            }
            Err(e) => println!("[SELFTEST] ERR LevelSettings: {e}"),
        }
        return;
    }
    println!("[SELFTEST] 1/3 构造 StartGame");
    let pkt = build_start_game(crate::bedrock::protocol::PLAYER_RUNTIME_ID);
    println!("[SELFTEST] 2/3 序列化 StartGame");
    let mut buf: Vec<u8> = Vec::new();
    match pkt.serialize(&mut buf) {
        Ok(()) => println!("[SELFTEST] OK StartGame len={}", buf.len()),
        Err(e) => println!("[SELFTEST] ERR StartGame: {e}"),
    }

    println!("[SELFTEST] 3/3 区块数据");
    let d = world::build_payload(&world::ChunkSpec {
        min_section: -4,
        max_section: 19,
        highest_non_empty: 4,
        floor_section: 4,
        stone_state: 2403,
        air_state: 11996,
        biome_id: 1,
        block_version: 18163713,
        mode: world::Mode::Runtime,
    });
    let sub_len = d.len();
    // 平台无关：写到当前工作目录下的 logs/，不再写死 Android 路径
    std::fs::create_dir_all("logs").ok();
    std::fs::write("logs/chunk_payload.bin", &d).ok();
    println!("[SELFTEST] 已写出区块负载 {} 字节", d.len());
    println!(
        "[SELFTEST] OK chunk sub={} total={} head={:02x?}",
        sub_len,
        d.len(),
        &d[..16.min(d.len())]
    );
    println!("[SELFTEST] 完成，没有崩溃");
}

/// 入站日志的汇总窗口：`PlayerAuthInput`/`MovePlayer` 每到此长度才打一条汇总。
///
/// 逐包打日志会把一次 1 分钟的会话刷成几 MB（实测），排查时反而找不到关键行。
const LOG_WINDOW: std::time::Duration = std::time::Duration::from_secs(1);

/// 分配实体运行时 id：全局递增，从 1 开始（与 Bedrock 客户端自己的实体计数习惯一致）。
///
/// 之前所有连接都用写死的 `ActorRuntimeID(1)`（BUG-032 第 22 条）：
/// 多连接撞号、服务端位置纠正也指不准人。
static NEXT_RUNTIME_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_runtime_id() -> u64 {
    NEXT_RUNTIME_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// 本连接上"别的玩家"的本地实体状态（BUG-032 第 22 条完整版）。
///
/// id 由本连接自己分配：Bedrock 的 runtime id 只是客户端本地句柄，
/// 不同客户端看到同一个玩家可以是不同数字，只要求会话内稳定。
struct RemotePlayerState {
    runtime_id: u64,
    /// unique id 与 runtime id 用同一个数（Geyser 的 `geyserId` 也是填两处）。
    unique_id: i64,
    /// 上次下发的 **Bedrock 坐标**与朝向，用来判断要不要再发 `MovePlayer`。
    position: (f32, f32, f32),
    rotation: (f32, f32),
    on_ground: bool,
    /// 上次下发的主手物品与姿态（A1 / A3）：只有变了才发 `MobEquipment` / `SetActorData`。
    held_item: Option<(u32, u8)>,
    sneaking: bool,
    sprinting: bool,
}

pub async fn serve(addr: String, source: WorldSource) -> Result<(), Box<dyn std::error::Error>> {
    let mut listener = Listener::builder()
        .raknet(addr.parse().unwrap())
        .name("MCHPRS Bedrock Spike")
        .sub_name("mchprs-io")
        .protocol::<Protocol>()
        .max_players(10)
        .player_count(0)
        .bind()
        .await?;

    println!(
        "[A3] LISTEN udp {addr} protocol={} version={}",
        Protocol::PROTOCOL_VERSION,
        Protocol::GAME_VERSION
    );

    loop {
        let conn = listener.accept::<Unknown>().await?;
        let source = source.clone();
        tokio::spawn(async move {
            let start = Instant::now();
            let mut conn = conn.into_ver::<Protocol>();
            // 每条连接一个自己的实体运行时 id（BUG-032 第 22 条）：以前全写死 1。
            let runtime_id = next_runtime_id();
            println!(
                "[A3] t={:.3} ACCEPT peer runtime_id={runtime_id} addr={}",
                wall_clock(),
                conn.get_socket_addr()
            );
            let session_info = match do_login(&mut conn, start, &source, runtime_id).await {
                Ok(info) => {
                    println!("[B4] 登录链路完成，等待后续阶段");
                    Some(info)
                }
                Err(e) => {
                    println!("[!!] 失败: {e}");
                    None
                }
            };

            // 玩家化：把这条 Bedrock 连接登记成 MCHPRS 的真玩家（BACKLOG.md BUG-032）。
            // 服务端会用这个句柄构造 `PlayerBackend::Bedrock` 并送进 spawn plot (0,0)。
            let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            // 指令方向必须是 tokio 通道：它要和 `conn.recv()` 一起进 `tokio::select!`，
            // 而 `std::sync::mpsc::Receiver::recv()` 是阻塞调用、不是 future（会编译报错）。
            // 入站（`inbound_tx/rx`）仍然用 std mpsc：plot 那边是同步 tick，直接 `try_recv` 排空。
            let (command_tx, mut command_rx) =
                tokio::sync::mpsc::unbounded_channel::<BedrockCommand>();
            // 入站方向：Bedrock 输入 → Java 入站包 → plot 复用现有逻辑。
            let (inbound_tx, inbound_rx) = mpsc::channel::<
                Box<dyn crate::packets::serverbound::ServerBoundPacket>,
            >();
            // 背包同步方向（BUG-032 R5）：Bedrock 客户端"手里拿的是什么"必须先于
            // "我要放这个方块"到达 plot，所以单独开一条通道（plot 每 tick 先排空它）。
            let (inventory_tx, inventory_rx) = mpsc::channel::<
                crate::bedrock::bridge::BedrockInventoryUpdate,
            >();
            // 客户端自报的最新位置（Bedrock 坐标）。打开背包时要用它当容器坐标。
            // 初值取登录阶段定下的出生点；`do_login` 给的是 MCHPRS 坐标，这里换回 Bedrock。
            let mut last_pos: (f32, f32, f32) = session_info
                .as_ref()
                .map(|info| {
                    (
                        info.position.0 as f32,
                        info.position.1 as f32 - world::Y_OFFSET as f32,
                        info.position.2 as f32,
                    )
                })
                .unwrap_or((0.0, 0.0, 0.0));
            if let (Some(info), WorldSource::Mchprs { events, .. }) = (session_info, &source) {
                let handle = BedrockHandle::new(command_tx, alive.clone());
                let link = crate::bedrock::bridge::BedrockPlayerLink {
                    handle,
                    packets: inbound_rx,
                    inventory: inventory_rx,
                };
                let spawn = (info.position.0, info.position.1, info.position.2);
                if events
                    .send(BedrockEvent::RegisterPlayer {
                        username: info.username,
                        position: spawn,
                        link,
                    })
                    .is_err()
                {
                    println!("[!!] 无法登记 Bedrock 玩家：服务端通道已关闭");
                }
            }

            // 保持连接，继续打印收包（B4 之前用于观察）
            // 用客户端自报坐标实测水平速度：客观判断“移动是否真的变慢”
            // 参考值：原版行走 4.317 m/s，潜行 1.31 m/s，游泳 2.2 m/s
            let mut last_move: Option<((f32, f32, f32), Instant)> = None;
            // 客户端最近一次上报的旋转 `(pitch, yaw)`：服务端强制定位时要原样带上，
            // 否则一传送就把视角拽回 0。
            let mut last_rotation: (f32, f32) = (0.0, 0.0);
            // 入站日志限流：`PlayerAuthInput` 每秒 20 条，逐条打会把日志刷成几 MB。
            // 每 `LOG_WINDOW` 汇总一次（条数 + 最后位置 + 最后 flags）。
            let mut input_count: u32 = 0;
            // 最后一次 PlayerAuthInput 采样（位置 + 时刻），用来算实测速度。
            let mut last_input_sample: Option<((f32, f32, f32), Instant)> = None;
            let mut input_window = Instant::now();
            let mut move_count: u32 = 0;
            let mut move_window = Instant::now();
            // 出站侧计分板的本地状态：用来判断"要不要重发 objective"和"要不要删掉多余行"。
            let mut sidebar_sent = false;
            let mut sidebar_title = String::new();
            let mut sidebar_len = 0usize;
            // 上一版每行的文本：用来判断"这一行是不是改过名字"（改过就必须先 REMOVE）。
            let mut sidebar_texts: Vec<String> = Vec::new();
            // 别的玩家在本客户端里的实体状态（BUG-032 第 22 条完整版）。
            // key = 玩家 uuid；runtime/unique id 由本连接自己分配，只要求会话内稳定。
            let mut remote_players: std::collections::HashMap<u128, RemotePlayerState> =
                std::collections::HashMap::new();
            loop {
                // 同时等两件事：客户端发包、服务端下达指令（玩家化后 MCHPRS 可能踢人/推送）。
                //
                // 指令分支**不能**在这里直接 `conn.send()`：`conn.recv()` 的 future 还活着，
                // `conn` 被它借着。所以指令只搬进局部变量，等 select 结束后统一处理。
                let mut sidebar_update: Option<crate::bedrock::bridge::BedrockSidebar> = None;
                let mut chat_lines: Vec<String> = Vec::new();
                // 方块更新/能力值：本轮 select 里攒起来，统一在 select 之后发（同一个借用原因）。
                let mut block_updates: Vec<(i32, i32, i32, u32)> = Vec::new();
                let mut abilities: Option<(bool, f32)> = None;
                let mut teleport_to: Option<(f64, f64, f64)> = None;
                let mut player_snapshot: Option<Vec<crate::bedrock::bridge::BedrockPlayerInfo>> =
                    None;
                // 增量区块（BUG-032 第 26 条）：本轮 select 里收到的一批，select 之后统一发。
                let mut chunk_batch: Option<crate::bedrock::bridge::BedrockChunkBatch> = None;
                // 软枚举（B4 动态补全）：值变了就整份替换。
                let mut soft_enum: Option<(String, Vec<String>)> = None;
                // 挥手事件（A3）：一次可能收到多条（多人同时挥）。
                let mut swings: Vec<u128> = Vec::new();
                // 游戏模式变化（A5）。
                let mut game_mode: Option<crate::bedrock::bridge::BedrockGameMode> = None;
                // 告示牌文字（A2）。
                let mut sign_updates: Vec<crate::bedrock::bridge::BedrockSign> = Vec::new();
                // 让客户端弹告示牌编辑框（BUG-032 第 43 条）：`(x, y, z, front)`，MCHPRS 坐标。
                let mut open_signs: Vec<(i32, i32, i32, bool)> = Vec::new();
                // 背包回传（BUG-032 第 39 条）：进 plot 时发一次。
                let mut inventory_snapshot: Option<
                    crate::bedrock::bridge::BedrockInventorySnapshot,
                > = None;
                let mut should_close = false;
                let received = tokio::select! {
                    received = conn.recv() => received,
                    command = command_rx.recv() => {
                        match command {
                            // `None` = 服务端把所有句柄都 drop 了（例如服务端正在退出）。
                            Some(BedrockCommand::Disconnect) | None => should_close = true,
                            Some(BedrockCommand::Sidebar(sidebar)) => sidebar_update = Some(sidebar),
                            Some(BedrockCommand::Chat { text }) => chat_lines.push(text),
                            Some(BedrockCommand::BlockUpdates(changes)) => {
                                block_updates.extend(changes)
                            }
                            Some(BedrockCommand::Abilities { flying, fly_speed }) => {
                                abilities = Some((flying, fly_speed))
                            }
                            Some(BedrockCommand::Teleport { x, y, z }) => {
                                teleport_to = Some((x, y, z))
                            }
                            Some(BedrockCommand::Players(players)) => {
                                player_snapshot = Some(players)
                            }
                            Some(BedrockCommand::Chunks(batch)) => chunk_batch = Some(batch),
                            Some(BedrockCommand::SoftEnum { name, values }) => {
                                soft_enum = Some((name, values))
                            }
                            Some(BedrockCommand::Swing { uuid }) => swings.push(uuid),
                            Some(BedrockCommand::GameMode(mode)) => game_mode = Some(mode),
                            Some(BedrockCommand::Sign(sign)) => sign_updates.push(sign),
                            Some(BedrockCommand::OpenSign { x, y, z, front }) => {
                                open_signs.push((x, y, z, front))
                            }
                            Some(BedrockCommand::Inventory(snapshot)) => {
                                inventory_snapshot = Some(snapshot)
                            }
                        }
                        // 返回一个空批次，让下面的收包逻辑什么都不做。
                        Ok(Vec::new())
                    }
                };
                if should_close {
                    println!("[C3] 服务端要求断开（或已释放句柄），断开 Bedrock 客户端");
                    break;
                }

                // 聊天/命令回执：一行一个 `TextPacket`（RAW，`§` 码由客户端解析）。
                if !chat_lines.is_empty() {
                    let packets: Vec<Protocol> = chat_lines
                        .iter()
                        .map(|text| {
                            Protocol::TextPacket(Box::new(crate::bedrock::text::raw_text(text)))
                        })
                        .collect();
                    println!("[R4] 下发聊天 {} 行", packets.len());
                    crate::bedrock::debug::dump_out(&packets);
                    if let Err(e) = conn.send(&packets).await {
                        println!("[!!] 下发聊天失败: {e}");
                        break;
                    }
                }

                // 计分板推送：先补 objective（首次/标题变了）、再删掉需要重画的行、最后整行覆写。
                //
                // **为什么改文本的行要先 REMOVE**：Bedrock 的侧边栏行文本就是 fake player 名，
                // 行由 scoreboard_id 标识；同一个 id 只改文本客户端不刷新（Mojang bug MCPE-143063）。
                // 不先删的话 `/redpiler compile` 之后第一行永远停在 `Stopped`（真机实测）。
                // Geyser 的做法见 `SidebarDisplaySlot#render0`（文本变了就 remove + set）。
                if let Some(sidebar) = sidebar_update {
                    let mut packets: Vec<Protocol> = Vec::new();
                    if !sidebar_sent || sidebar.title != sidebar_title {
                        packets.push(Protocol::SetDisplayObjectivePacket(Box::new(
                            crate::bedrock::scoreboard::set_display_objective(&sidebar.title),
                        )));
                        sidebar_sent = true;
                        sidebar_title = sidebar.title.clone();
                    }
                    // 1) 文本变了的行（含这次新增、下标越界的行）：先删。
                    let renamed: Vec<usize> = (0..sidebar.lines.len())
                        .filter(|index| {
                            sidebar_texts.get(*index) != Some(&sidebar.lines[*index].text)
                        })
                        .collect();
                    if !renamed.is_empty() {
                        packets.push(Protocol::SetScorePacket(Box::new(
                            crate::bedrock::scoreboard::remove_scores_at(&renamed),
                        )));
                    }
                    // 2) 行数变少：删掉尾巴。
                    if sidebar.lines.len() < sidebar_len {
                        packets.push(Protocol::SetScorePacket(Box::new(
                            crate::bedrock::scoreboard::remove_scores(
                                sidebar.lines.len(),
                                sidebar_len,
                            ),
                        )));
                    }
                    // 3) 整行覆写。
                    packets.push(Protocol::SetScorePacket(Box::new(
                        crate::bedrock::scoreboard::change_scores(&sidebar),
                    )));
                    sidebar_len = sidebar.lines.len();
                    sidebar_texts = sidebar.lines.iter().map(|l| l.text.clone()).collect();
                    crate::bedrock::debug::dump_out(&packets);
                    if let Err(e) = conn.send(&packets).await {
                        println!("[!!] 下发计分板失败: {e}");
                        break;
                    }
                    println!(
                        "[R3] 计分板已下发：{} 行，标题={:?}",
                        sidebar.lines.len(),
                        sidebar.title
                    );
                }
                // 方块更新（BUG-032 第 3 条）：MCHPRS 坐标/状态 → Bedrock 坐标/运行时 id。
                // 坐标换算与 id 映射都只在这里做一次。
                if !block_updates.is_empty() {
                    let packets: Vec<Protocol> = block_updates
                        .iter()
                        .map(|(x, y, z, java_state)| {
                            Protocol::UpdateBlockPacket(Box::new(
                                crate::bedrock::protocol::block_update(
                                    (*x, *y - world::Y_OFFSET, *z),
                                    crate::bedrock::bedrock_block_state(*java_state) as u32,
                                ),
                            ))
                        })
                        .collect();
                    crate::bedrock::debug::dump_out(&packets);
                    if let Err(e) = conn.send(&packets).await {
                        println!("[!!] 下发方块更新失败: {e}");
                        break;
                    }
                }

                // 能力值跟随（飞行状态 + 飞行速度）。
                if let Some((flying, fly_speed)) = abilities {
                    let packets = vec![Protocol::UpdateAbilitiesPacket(Box::new(
                        crate::bedrock::protocol::update_abilities_for(flying, runtime_id, fly_speed),
                    ))];
                    println!("[R2] 能力值跟随：flying={flying} fly_speed={fly_speed}");
                    crate::bedrock::debug::dump_out(&packets);
                    if let Err(e) = conn.send(&packets).await {
                        println!("[!!] 下发能力值失败: {e}");
                        break;
                    }
                }

                // 服务端强制定位：MCHPRS 坐标 → Bedrock 坐标（`y - 64`）。
                // 用本连接自己的 runtime id，别再写死 1。
                if let Some((x, y, z)) = teleport_to {
                    // `MovePlayer` 用**眼睛高度**（脚 + 1.62），见 `entity::PLAYER_EYE_HEIGHT`。
                    let bedrock_pos = (
                        x as f32,
                        (y - world::Y_OFFSET as f64 + crate::bedrock::entity::PLAYER_EYE_HEIGHT as f64)
                            as f32,
                        z as f32,
                    );
                    let packets = vec![Protocol::MovePlayerPacket(Box::new(
                        crate::bedrock::protocol::teleport(runtime_id, bedrock_pos, last_rotation),
                    ))];
                    crate::bedrock_verbose!("[R5] 服务端定位：({x:.2}, {y:.2}, {z:.2}) → Bedrock ({:.2}, {:.2}, {:.2})",
                        bedrock_pos.0, bedrock_pos.1, bedrock_pos.2
                    );
                    crate::bedrock::debug::dump_out(&packets);
                    if let Err(e) = conn.send(&packets).await {
                        println!("[!!] 下发定位失败: {e}");
                        break;
                    }
                    last_pos = bedrock_pos;
                }

                // 别的玩家同步（BUG-032 第 22 条完整版）：core 每 tick 推一份全量快照，
                // 这里做 diff —— 新来的"先玩家列表、再生成实体"，走了的"先销毁实体、
                // 再从玩家列表移除"，动了的发 `MovePlayer`。
                if let Some(players) = player_snapshot {
                    use crate::bedrock::entity;
                    let mut packets: Vec<Protocol> = Vec::new();

                    // 1) 已经不在列表里的人。
                    let alive: std::collections::HashSet<u128> =
                        players.iter().map(|p| p.uuid).collect();
                    let gone: Vec<u128> = remote_players
                        .keys()
                        .copied()
                        .filter(|uuid| !alive.contains(uuid))
                        .collect();
                    for uuid in gone {
                        if let Some(state) = remote_players.remove(&uuid) {
                            packets.push(Protocol::RemoveActorPacket(Box::new(
                                entity::remove_actor(state.unique_id),
                            )));
                            packets.push(Protocol::PlayerListPacket(Box::new(
                                entity::player_list_remove(uuid),
                            )));
                            println!("[R6] 移除玩家实体 uuid={uuid:x} unique={}", state.unique_id);
                        }
                    }

                    // 2) 新来的 / 动了的。
                    for info in &players {
                        match remote_players.get_mut(&info.uuid) {
                            None => {
                                let runtime = next_runtime_id();
                                let unique_id = runtime as i64;
                                packets.push(Protocol::PlayerListPacket(Box::new(
                                    entity::player_list_add(info, unique_id),
                                )));
                                packets.push(Protocol::AddPlayerPacket(Box::new(
                                    entity::add_player(info, runtime),
                                )));
                                // 记账用**脚**坐标（`AddPlayer` 发的就是脚）；`MovePlayer` 的 y
                                // 由 `entity::move_player` 自己 +1.62（见 PLAYER_EYE_HEIGHT）。
                                let position = entity::bedrock_feet(info.position);
                                println!(
                                    "[R6] 生成玩家 {} runtime={runtime} pos={position:?}",
                                    info.username
                                );
                                remote_players.insert(
                                    info.uuid,
                                    RemotePlayerState {
                                        runtime_id: runtime,
                                        unique_id,
                                        position,
                                        rotation: entity::bedrock_rotation(info.rotation),
                                        on_ground: info.on_ground,
                                        held_item: info.held_item,
                                        sneaking: info.sneaking,
                                        sprinting: info.sprinting,
                                    },
                                );
                            }
                            Some(state) => {
                                // A1 主手物品：变了就发 `MobEquipment`（和位置无关，
                                // 换物品时人可能站着不动）。
                                if state.held_item != info.held_item {
                                    packets.push(Protocol::MobEquipmentPacket(Box::new(
                                        entity::mob_equipment(state.runtime_id, info.held_item),
                                    )));
                                    crate::bedrock_verbose!("[R8] {} 主手物品 {:?} → {:?}",
                                        info.username, state.held_item, info.held_item
                                    );
                                    state.held_item = info.held_item;
                                }
                                // A3 姿态：潜行/疾跑位变了才发（`SetActorData` 只带 FLAGS 一条）。
                                if state.sneaking != info.sneaking
                                    || state.sprinting != info.sprinting
                                {
                                    packets.push(Protocol::SetActorDataPacket(Box::new(
                                        crate::bedrock::protocol::flags_actor_data(
                                            state.runtime_id,
                                            info.sneaking,
                                            info.sprinting,
                                        ),
                                    )));
                                    crate::bedrock_verbose!("[R8] {} 姿态 sneaking={} sprinting={} flags=0x{:x} bits={:?}",
                                        info.username,
                                        info.sneaking,
                                        info.sprinting,
                                        crate::bedrock::protocol::player_flags(
                                            info.sneaking,
                                            info.sprinting
                                        ),
                                        crate::bedrock::protocol::flag_bits()
                                    );
                                    state.sneaking = info.sneaking;
                                    state.sprinting = info.sprinting;
                                }
                                let position = entity::bedrock_feet(info.position);
                                let rotation = entity::bedrock_rotation(info.rotation);
                                let moved = (position.0 - state.position.0).abs() > 0.01
                                    || (position.1 - state.position.1).abs() > 0.01
                                    || (position.2 - state.position.2).abs() > 0.01;
                                let turned = (rotation.0 - state.rotation.0).abs() > 0.5
                                    || (rotation.1 - state.rotation.1).abs() > 0.5;
                                if !(moved || turned || info.on_ground != state.on_ground) {
                                    continue;
                                }
                                // 一步跨过 8 格只可能是传送（`/tp`、坠落纠正）：
                                // 用 `mode = Teleport` 让客户端直接落位，别演一段"跑过去"。
                                let teleported = (position.0 - state.position.0).abs() > 8.0
                                    || (position.1 - state.position.1).abs() > 8.0
                                    || (position.2 - state.position.2).abs() > 8.0;
                                packets.push(Protocol::MovePlayerPacket(Box::new(
                                    entity::move_player(state.runtime_id, info, teleported),
                                )));
                                state.position = position;
                                state.rotation = rotation;
                                state.on_ground = info.on_ground;
                            }
                        }
                    }

                    if !packets.is_empty() {
                        let count = packets.len();
                        crate::bedrock::debug::dump_out(&packets);
                        if let Err(e) = conn.send(&packets).await {
                            println!("[!!] 下发玩家同步失败: {e}");
                            break;
                        }
                        crate::bedrock_verbose!("[R6] 玩家同步：{count} 个包（线上共 {} 人）",
                            players.len()
                        );
                    }
                }

                // 增量区块（BUG-032 第 26 条 + 第 19 条）。
                //
                // plot 线程按每 tick 预算分批生成，这里只负责编码下发：
                // `NetworkChunkPublisherUpdate` 必须**先于**区块（客户端按 publisher 的范围
                // 决定保留哪些区块；顺序反了会导致区块发出去又被丢弃）。
                if let Some(batch) = chunk_batch {
                    let mut packets: Vec<Protocol> = Vec::new();
                    if let Some((bx, by, bz, radius)) = batch.publisher {
                        packets.push(Protocol::NetworkChunkPublisherUpdatePacket(Box::new(
                            NetworkChunkPublisherUpdatePacket {
                                new_view_position: BlockPos { x: bx, y: by, z: bz },
                                new_view_radius: radius,
                                server_built_chunks: vec![],
                            },
                        )));
                    }
                    let chunk_count = batch.chunks.len();
                    let bytes: usize = batch.chunks.iter().map(|(_, _, data, _)| data.len()).sum();
                    for (cx, cz, data, count) in batch.chunks {
                        packets.push(Protocol::LevelChunkPacket(Box::new(LevelChunkPacket {
                            chunk_position: ChunkPos { x: cx, z: cz },
                            dimension_id: 0,
                            sub_chunk_count: count,
                            sub_chunk_limit: 0,
                            cache_enabled: false,
                            cache_blobs: vec![],
                            serialized_chunk_data: data,
                        })));
                    }
                    crate::bedrock::debug::dump_out(&packets);
                    if let Err(e) = conn.send(&packets).await {
                        println!("[!!] 下发区块流送失败: {e}");
                        break;
                    }
                    println!(
                        "[C4] 区块流送 {chunk_count} 个（{bytes} 字节），publisher={:?}",
                        batch.publisher
                    );
                }

                // 软枚举（B4 动态补全）：整份替换。客户端拿到后，命令补全里就会列出这些值。
                if let Some((name, values)) = soft_enum {
                    let count = values.len();
                    let packet = vec![Protocol::UpdateSoftEnumPacket(Box::new(
                        crate::bedrock::commands::soft_enum_update(&name, values),
                    ))];
                    if let Err(e) = conn.send(&packet).await {
                        println!("[!!] 下发软枚举失败: {e}");
                        break;
                    }
                    println!("[B4] 软枚举 {name} = {count} 项");
                }

                // 挥手动效（A3）：一次事件包。目标不在本地实体表里就忽略
                // （可能是刚进/刚出 plot 的人，客户端根本没有这个实体）。
                if !swings.is_empty() {
                    let mut packets: Vec<Protocol> = Vec::new();
                    for uuid in &swings {
                        match remote_players.get(uuid) {
                            Some(state) => packets.push(Protocol::AnimatePacket(Box::new(
                                crate::bedrock::entity::animate_swing(state.runtime_id),
                            ))),
                            None => crate::bedrock_verbose!("[R8] 挥手目标不在实体表 uuid={uuid:x}，忽略"),
                        }
                    }
                    if !packets.is_empty() {
                        crate::bedrock_verbose!("[R8] 下发挥手 {} 次", packets.len());
                        crate::bedrock::debug::dump_out(&packets);
                        if let Err(e) = conn.send(&packets).await {
                            println!("[!!] 下发挥手失败: {e}");
                            break;
                        }
                    }
                }

                // 游戏模式（A5）：`/gamemode`、`/gmc`、`/gmsp` 会走到这里。
                if let Some(mode) = game_mode {
                    let packets = vec![Protocol::SetPlayerGameTypePacket(Box::new(
                        crate::bedrock::protocol::set_game_mode(mode),
                    ))];
                    println!("[R9] 游戏模式 → {mode:?}");
                    crate::bedrock::debug::dump_out(&packets);
                    if let Err(e) = conn.send(&packets).await {
                        println!("[!!] 下发游戏模式失败: {e}");
                        break;
                    }
                }

                // 打开告示牌编辑框（BUG-032 第 43 条）。
                //
                // 放在牌子的 `BlockActorData` **之前**：先让界面弹出来，字随后到，
                // 玩家一进来就能看到已有文字（Geyser 也是先 `OpenSign` 再由 Java 的
                // `BlockEntityData` 灌内容）。
                //
                // 少了这条，客户端根本不会弹编辑框 —— 真机"放下牌子后写不了字"就是这个。
                if !open_signs.is_empty() {
                    let packets: Vec<Protocol> = open_signs
                        .iter()
                        .map(|&(x, y, z, front)| {
                            Protocol::OpenSignPacket(Box::new(
                                crate::bedrock::protocol::open_sign(x, y, z, front),
                            ))
                        })
                        .collect();
                    crate::bedrock_verbose!("[A2] 下发 OpenSign {} 块：{:?}",
                        open_signs.len(),
                        open_signs
                    );
                    crate::bedrock::debug::dump_out(&packets);
                    if let Err(e) = conn.send(&packets).await {
                        println!("[!!] 下发 OpenSign 失败: {e}");
                        break;
                    }
                }

                // 告示牌文字（A2）：实时更新走 `BlockActorData`；进 plot 时的那一份
                // 随区块负载一起发（`chunk::build_chunk_from_world_with_signs`）。
                if !sign_updates.is_empty() {
                    let count = sign_updates.len();
                    let packets: Vec<Protocol> = sign_updates
                        .iter()
                        .map(|sign| {
                            Protocol::BlockActorDataPacket(Box::new(
                                crate::bedrock::block_entity::sign_actor_data(sign),
                            ))
                        })
                        .collect();
                    crate::bedrock_verbose!("[R7] 下发告示牌 {count} 块");
                    // 诊断：把第一块的实际字节打出来（真机"字不显示"时用来和
                    // `refs/ProbeSign.java` 的参考字节逐段对照）。
                    if let Some(Protocol::BlockActorDataPacket(first)) = packets.first() {
                        let mut buf = Vec::new();
                        {
                            use bedrock::protocol::ProtoCodec;
                            let _ = first.serialize(&mut buf);
                        }
                        let hex: String =
                            buf.iter().map(|b| format!("{b:02x}")).collect();
                        crate::bedrock_verbose!("[R7] 告示牌包 hex（{} 字节）= {}",
                            buf.len(),
                            &hex[..hex.len().min(240)]
                        );
                    }
                    crate::bedrock::debug::dump_out(&packets);
                    if let Err(e) = conn.send(&packets).await {
                        println!("[!!] 下发告示牌失败: {e}");
                        break;
                    }
                }

                // 背包回传（BUG-032 第 39 条）：把存档里的背包灌回客户端。
                //
                // 三份 `InventoryContent` 的顺序照 Geyser `PlayerInventoryTranslator#updateInventory`：
                // 主背包 36 → 护甲 4 → 副手 1；再发 `PlayerHotbar` 选中槽、`MobEquipment` 主手，
                // 保证"手里的东西"和快捷栏高亮都对。
                if let Some(snapshot) = inventory_snapshot {
                    use bedrock::protocol::v662::enums::ContainerID;
                    let selected_slot = snapshot.selected_slot.min(8);
                    let mut packets: Vec<Protocol> = vec![
                        Protocol::InventoryContentPacket(Box::new(
                            crate::bedrock::inventory::content_with(
                                ContainerID::INVENTORY,
                                &snapshot.main,
                            ),
                        )),
                        Protocol::InventoryContentPacket(Box::new(
                            crate::bedrock::inventory::content_with(
                                ContainerID::ARMOR,
                                &snapshot.armor,
                            ),
                        )),
                        Protocol::InventoryContentPacket(Box::new(
                            crate::bedrock::inventory::content_with(
                                ContainerID::OFFHAND,
                                &[snapshot.offhand],
                            ),
                        )),
                        Protocol::PlayerHotbarPacket(Box::new(PlayerHotbarPacket::<Protocol> {
                            selected_slot: selected_slot as u32,
                            container_id: ContainerID::INVENTORY,
                            // 客户端切到这一格（不是"服务端被动记录"）
                            should_select_slot: true,
                        })),
                        Protocol::MobEquipmentPacket(Box::new(
                            crate::bedrock::entity::mob_equipment_self(
                                runtime_id,
                                snapshot.main.get(selected_slot as usize).copied().flatten(),
                                selected_slot,
                            ),
                        )),
                    ];
                    crate::bedrock_verbose!("[R10] 背包回传：主背包 {} 格（非空 {}）、护甲 {}、副手 {}、选中槽 {}",
                        snapshot.main.len(),
                        snapshot.main.iter().filter(|s| s.is_some()).count(),
                        snapshot.armor.iter().filter(|s| s.is_some()).count(),
                        snapshot.offhand.is_some(),
                        selected_slot
                    );
                    crate::bedrock::debug::dump_out(&packets);
                    if let Err(e) = conn.send(&packets).await {
                        println!("[!!] 下发背包失败: {e}");
                        break;
                    }
                }

                // 背包相关：客户端的请求只置标志，统一在本轮收包结束后发包。
                let mut open_inventory = false;
                let mut close_inventory: Option<(ContainerID, bool)> = None;
                match received {
                    Ok(batch) => {
                        // 逐包调试：`debug::PACKETS = true` 时把这一批包名打出来。
                        crate::bedrock::debug::dump_in(&batch);
                        for p in &batch {
                            // 入站转义：Bedrock 输入 → Java 入站包 → plot 的现有逻辑。
                            // 只有 MCHPRS 世界（真玩家）才需要；Demo 世界没人读这条通道。
                            if matches!(source, WorldSource::Mchprs { .. }) {
                                for java_packet in crate::bedrock::translate::translate(p) {
                                    let _ = inbound_tx.send(java_packet);
                                }
                            }
                            match p {
                                Protocol::PlayerAuthInputPacket(pa) => {
                                    // S3：客户端的方块交互（点按/挖掘/放置）走 PerformBlockActions 位
                                    if let Some(actions) = &pa.player_block_actions {
                                        for a in actions {
                                            crate::bedrock_verbose!("[S3] << 方块交互 action={:?} pos={:?} facing={:?}",
                                                a.action_type, a.position, a.facing
                                            );
                                        }
                                    }
                                    last_pos = pa.player_position;
                                    last_rotation = pa.player_rotation;
                                    // 限流：每 `LOG_WINDOW` 只打一条汇总（原来是逐包打，日志被刷爆）。
                                    input_count += 1;
                                    if input_window.elapsed() >= LOG_WINDOW {
                                        // 顺带把**实测水平速度**打出来（BUG-032 第 31 条）：
                                        // 客户端自报的位置就是唯一客观的速度来源。原版参考值
                                        // 步行 4.317 m/s、疾跑 5.612 m/s、创造飞行约 10.9 m/s。
                                        let speed = match last_input_sample {
                                            Some((prev, at)) => {
                                                let dx = (pa.player_position.0 - prev.0) as f64;
                                                let dz = (pa.player_position.2 - prev.2) as f64;
                                                let dt = at.elapsed().as_secs_f64();
                                                if dt > 0.05 {
                                                    Some((dx * dx + dz * dz).sqrt() / dt)
                                                } else {
                                                    None
                                                }
                                            }
                                            None => None,
                                        };
                                        last_input_sample = Some((
                                            pa.player_position,
                                            Instant::now(),
                                        ));
                                        match speed {
                                            Some(v) => crate::bedrock_verbose!("[--] << PlayerAuthInput ×{} 最后 pos=({:.2}, {:.2}, {:.2}) 水平速度={:.2} m/s input=0x{:x}",
                                                input_count,
                                                pa.player_position.0,
                                                pa.player_position.1,
                                                pa.player_position.2,
                                                v,
                                                pa.input_data
                                            ),
                                            None => crate::bedrock_verbose!("[--] << PlayerAuthInput ×{} 最后 pos=({:.2}, {:.2}, {:.2}) input=0x{:x}",
                                                input_count,
                                                pa.player_position.0,
                                                pa.player_position.1,
                                                pa.player_position.2,
                                                pa.input_data
                                            ),
                                        }
                                        input_count = 0;
                                        input_window = Instant::now();
                                    }
                                }
                                Protocol::InteractPacket(ip) => {
                                    crate::bedrock_verbose!("[S3] << Interact action={:?} target={:?}",
                                        ip.action, ip.target_runtime_id
                                    );
                                    // 背包打不开的根因就在这里：Bedrock 客户端按 E 只发这个包，
                                    // 界面要等服务端回 ContainerOpen。不回 = 一直"打不开"。
                                    if matches!(ip.action, InteractAction::OpenInventory) {
                                        open_inventory = true;
                                    }
                                }
                                Protocol::MovePlayerPacket(mp) => {
                                    let now = Instant::now();
                                    let speed = match last_move {
                                        Some((prev, at)) => {
                                            let dx = (mp.position.0 - prev.0) as f64;
                                            let dz = (mp.position.2 - prev.2) as f64;
                                            let dt = now.duration_since(at).as_secs_f64();
                                            if dt > 0.0001 {
                                                Some((dx * dx + dz * dz).sqrt() / dt)
                                            } else {
                                                None
                                            }
                                        }
                                        None => None,
                                    };
                                    last_move = Some((mp.position, now));
                                    // 同样限流：每 `LOG_WINDOW` 一条，带累计条数与最后速度。
                                    move_count += 1;
                                    if move_window.elapsed() >= LOG_WINDOW {
                                        match speed {
                                            Some(v) => crate::bedrock_verbose!("[--] << MovePlayer ×{} 最后 pos=({:.2}, {:.2}, {:.2}) on_ground={} 水平速度={:.2} m/s",
                                                move_count,
                                                mp.position.0, mp.position.1, mp.position.2, mp.on_ground, v
                                            ),
                                            None => crate::bedrock_verbose!("[--] << MovePlayer ×{} 最后 pos=({:.2}, {:.2}, {:.2}) on_ground={}",
                                                move_count,
                                                mp.position.0, mp.position.1, mp.position.2, mp.on_ground
                                            ),
                                        }
                                        move_count = 0;
                                        move_window = Instant::now();
                                    }
                                    // 以前这里会把位置原样回显给客户端（"服务端权威移动"）。
                                    // 那既没用又和客户端预测打架：回显用的是写死的 `ActorRuntimeID(1)`，
                                    // 每个包都把客户端自己报的位置再设回去，是"移动发飘/变慢"的可疑来源
                                    // （BACKLOG.md BUG-032 复查第 17 条）。现在只记录，不纠正；
                                    // 真要纠正时必须走 `MovePlayer(Teleport)` 且用真实的 runtime id。
                                }
                                Protocol::PlayerHotbarPacket(h) => {
                                    // 快捷栏切槽：核心的 `selected_slot` 要跟上，
                                    // 放方块时才知道手里拿的是第几格。
                                    crate::bedrock_verbose!("[R5] << PlayerHotbar slot={}", h.selected_slot);
                                    let _ = inventory_tx.send(
                                        crate::bedrock::bridge::BedrockInventoryUpdate::SelectedSlot(
                                            h.selected_slot,
                                        ),
                                    );
                                }
                                Protocol::MobEquipmentPacket(me) => {
                                    // BUG-032 第 40 条：基岩客户端切快捷栏 / 换手持物品时发这个包，
                                    // **它带着那个槽里到底是什么**。以前我们只更新槽号、不更新物品，
                                    // 于是服务端判木斧用的是旧的 `inventory[槽]`：
                                    // 拿木斧时可能去破坏方块、空手时反而去选点。
                                    use bedrock::protocol::v662::enums::ContainerID;
                                    if me.container_id != ContainerID::INVENTORY {
                                        continue;
                                    }
                                    let hotbar = me.slot.clamp(0, 8) as u32;
                                    let bedrock_item = me.item.id;
                                    let java_item = if bedrock_item == 0 {
                                        None
                                    } else {
                                        crate::bedrock::mchprs_item_id(bedrock_item)
                                    };
                                    let count = me
                                        .item
                                        .stack_size
                                        .unwrap_or(1)
                                        .clamp(1, 255) as u8;
                                    crate::bedrock_verbose!("[R5] << MobEquipment slot={hotbar} bedrock_item={bedrock_item} → java={java_item:?}×{count}"
                                    );
                                    let _ = inventory_tx.send(
                                        crate::bedrock::bridge::BedrockInventoryUpdate::SelectedSlot(
                                            hotbar,
                                        ),
                                    );
                                    let _ = inventory_tx.send(
                                        crate::bedrock::bridge::BedrockInventoryUpdate::Slot {
                                            // Java 背包里快捷栏是 36..=44
                                            slot: 36 + hotbar,
                                            item: java_item.map(|id| (id, count)),
                                        },
                                    );
                                }
                                Protocol::InventoryTransactionPacket(tx) => {
                                    use bedrock::protocol::v662::enums::ItemUseInventoryTransactionType;
                                    use bedrock::protocol::v662::types::InventoryTransactionData;

                                    // BUG-032 第 44 条：丢物品（按 Q）。
                                    //
                                    // 形状照 Geyser `BedrockInventoryTransactionTranslator`：
                                    // `NORMAL` 事务 + 两条 action，`actions[0]` 来源是
                                    // `WORLD_INTERACTION`。我们**没有掉落物实体**（MCHPRS 世界
                                    // 里压根不放实体），所以唯一要做的是把服务端镜像的背包
                                    // 改成丢完的样子 —— 否则切栏 / 重进时 `InventoryContent`
                                    // 会把丢掉的东西"复活"。
                                    if matches!(&tx.data, InventoryTransactionData::Normal) {
                                        match crate::bedrock::inventory::drop_update(&tx.actions) {
                                            Some((slot, item)) => {
                                                crate::bedrock_verbose!("[R5] << 丢物品 slot={slot} 剩余={item:?}"
                                                );
                                                let _ = inventory_tx.send(
                                                    crate::bedrock::bridge::BedrockInventoryUpdate::Slot {
                                                        // Java 背包里快捷栏是 36..=44
                                                        slot: 36 + slot,
                                                        item,
                                                    },
                                                );
                                            }
                                            None => {
                                                // 没认出来也把形状打出来：真机上一看就知道
                                                // 客户端这一版到底发了什么（来源/槽位/前后数量）。
                                                crate::bedrock_verbose!("[R5] << InventoryTransaction(Normal) actions={} 未识别",
                                                    tx.actions.len()
                                                );
                                                for (i, action) in tx.actions.iter().enumerate() {
                                                    crate::bedrock_verbose!("[R5]    action[{i}] 来源={:?} 槽={} from=({}, {:?}) to=({}, {:?})",
                                                        action.source.source_type,
                                                        action.slot,
                                                        action.from_item_descriptor.id,
                                                        action.from_item_descriptor.stack_size,
                                                        action.to_item_descriptor.id,
                                                        action.to_item_descriptor.stack_size,
                                                    );
                                                }
                                            }
                                        }
                                        continue;
                                    }

                                    // 只关心「对着方块按右键/左键」这一种事务；其余（容器搬物）
                                    // 先不处理。
                                    let InventoryTransactionData::ItemUse(use_data) = &tx.data else {
                                        crate::bedrock_verbose!("[R5] << InventoryTransaction(其他类型)");
                                        continue;
                                    };
                                    let action = use_data.action_type.clone();
                                    crate::bedrock_verbose!("[R5] << InventoryTransaction ItemUse action={:?} pos={:?} face={} slot={} held_item={}",
                                        action,
                                        (
                                            use_data.block_position.x,
                                            use_data.block_position.y,
                                            use_data.block_position.z
                                        ),
                                        use_data.block_face,
                                        use_data.hotbar_slot,
                                        use_data.held_item.id,
                                    );
                                    // `Place`（枚举 0）就是「点方块」：放方块、拉杆、按钮、开门都走它。
                                    // Bedrock 的 Destroy 由 `PlayerAuthInput` 那条路径管，这里不重复转。
                                    if !matches!(action, ItemUseInventoryTransactionType::Place) {
                                        continue;
                                    }
                                    // Java 的 `BlockFace::from_id` 对越界 id 会 panic，先挡一下：
                                    // Bedrock 与 Java 的 0..=5 顺序一致（Down/Up/North/South/West/East）。
                                    if !(0..=5).contains(&use_data.block_face) {
                                        crate::bedrock_verbose!("[R5] !! 方块面非法 {}，丢弃",
                                            use_data.block_face
                                        );
                                        continue;
                                    }

                                    // ① 先把「手里是什么」同步给 plot：`handle_use_item_impl` 是从
                                    //    `Player::inventory` 取物品的，空手就什么都不会放。
                                    let hotbar_slot = use_data.hotbar_slot.clamp(0, 8) as u32;
                                    let java_item_id =
                                        crate::bedrock::mchprs_item_id(use_data.held_item.id);
                                    // 手里有东西、但 MCHPRS 不认识这个物品（创造面板是全量原版物品）：
                                    // 客户端已经乐观地在本地把方块放上去了，服务端必须把真实方块发回去，
                                    // 否则那个方块永远留在客户端（Java 侧靠 `send_block_corrections`）。
                                    // 空手（id = 0）不算：那是右键交互（拉杆/按钮/开门），照旧走 SUseItemOn。
                                    if use_data.held_item.id != 0 && java_item_id.is_none() {
                                        let _ = inventory_tx.send(
                                            crate::bedrock::bridge::BedrockInventoryUpdate::Correction {
                                                x: use_data.block_position.x,
                                                y: use_data.block_position.y as i32 + 64,
                                                z: use_data.block_position.z,
                                            },
                                        );
                                        crate::bedrock_verbose!("[R5] !! 未知物品放置（bedrock item id={}）pos=({}, {}, {}) → 请求修正",
                                            use_data.held_item.id,
                                            use_data.block_position.x,
                                            use_data.block_position.y as i32 + 64,
                                            use_data.block_position.z
                                        );
                                    }
                                    let count = use_data
                                        .held_item
                                        .stack_size
                                        .unwrap_or(1)
                                        .min(255) as u8;
                                    let _ = inventory_tx.send(
                                        crate::bedrock::bridge::BedrockInventoryUpdate::SelectedSlot(
                                            hotbar_slot,
                                        ),
                                    );
                                    let _ = inventory_tx.send(
                                        crate::bedrock::bridge::BedrockInventoryUpdate::Slot {
                                            // Java 背包里快捷栏是 36..=44
                                            slot: 36 + hotbar_slot,
                                            item: java_item_id.map(|id| (id, count)),
                                        },
                                    );

                                    // ② 再把这个动作转成 Java 的 `SUseItemOn`（plot 只认它）。
                                    let _ = inbound_tx.send(Box::new(
                                        crate::packets::serverbound::SUseItemOn {
                                            hand: 0,
                                            x: use_data.block_position.x,
                                            // Bedrock 的 y 比 MCHPRS 少 64
                                            y: use_data.block_position.y as i32 + 64,
                                            z: use_data.block_position.z,
                                            face: use_data.block_face,
                                            cursor_x: use_data.click_position.0,
                                            cursor_y: use_data.click_position.1,
                                            cursor_z: use_data.click_position.2,
                                            inside_block: false,
                                            sequence: 0,
                                        },
                                    ));
                                    crate::bedrock_verbose!("[R5] >> SUseItemOn pos=({}, {}, {}) face={} item={:?}",
                                        use_data.block_position.x,
                                        use_data.block_position.y as i32 + 64,
                                        use_data.block_position.z,
                                        use_data.block_face,
                                        java_item_id
                                    );
                                }
                                Protocol::ItemStackRequestPacket(req) => {
                                    // 拿物品：创造面板取物走这条。客户端在本服务端（我们发
                                    // `enable_item_stack_net_manager = false`）是乐观应用的，
                                    // 这里只做记录；真正让服务端"知道手里有东西"的是上面
                                    // InventoryTransaction 里带过来的 held_item。
                                    for request in &req.requests {
                                        crate::bedrock_verbose!("[R5] << ItemStackRequest id={} actions={}",
                                            request.client_request_id,
                                            request.actions.len()
                                        );
                                    }
                                }
                                Protocol::ContainerClosePacket(cc) => {
                                    crate::bedrock_verbose!("[R1] << ContainerClose id={} server_initiated={}",
                                        cc.container_id.0, cc.server_initiated_close
                                    );
                                    close_inventory =
                                        Some((cc.container_id, cc.server_initiated_close));
                                }
                                Protocol::SubChunkRequestPacket(_) => {
                                    crate::bedrock_verbose!("[--] << SubChunkRequest")
                                }
                                Protocol::BlockActorDataPacket(p) => {
                                    // BUG-032 第 43 条：客户端写/改告示牌。这里只打日志 ——
                                    // 真正的转义在 `translate()` 里（已经在这之前跑过），
                                    // 日志留证用：真机第一眼要确认客户端到底发了没有、带了哪些键。
                                    let mut keys: Vec<&str> = p
                                        .actor_data_tags
                                        .keys()
                                        .map(|k| k.as_str())
                                        .collect();
                                    keys.sort_unstable();
                                    crate::bedrock_verbose!("[A2] << BlockActorData pos=({}, {}, {}) 键={:?}",
                                        p.block_position.x,
                                        p.block_position.y,
                                        p.block_position.z,
                                        keys
                                    );
                                    // 诊断：把解析出来的四行也打出来 —— 和 `[R7]` 下发那份对照，
                                    // 就能判断是"没解析出来"还是"解析对了但客户端不吃"。
                                    if let Some((front, back)) =
                                        crate::bedrock::block_entity::sign_rows(&p.actor_data_tags)
                                    {
                                        crate::bedrock_verbose!("[A2]   解析后 正面={front:?} 反面={back:?}");
                                    }
                                }
                                Protocol::Unknown(u) => crate::bedrock_verbose!("[--] << Unknown id=0x{:x}", u.id),
                                other => crate::bedrock_verbose!("[--] << {}", pname(other)),
                            }
                        }
                    }
                    // 以前这里是裸 `Err(_) => break`：客户端一断开，服务端只留一行
                    // "left the game"，完全看不出是解码失败还是对面主动断的（BUG-032 第 29 条）。
                    Err(e) => {
                        println!("[!!] 收包失败，断开连接: {e}");
                        break;
                    }
                }

                // 打开背包（BUG-032 第 45 条）：**只发 `ContainerOpen`**。
                //
                // 以前这里跟着发三份 `InventoryContent`，而且是**全空**的
                // （`inventory::open_inventory_packets`）—— 客户端把服务端发的当成权威内容，
                // 于是一打开背包，东西全被清空（真机现象）。
                //
                // 我们的背包是**客户端权威**（`StartGame.enable_item_stack_net_manager = false`），
                // 服务端只知道"选中槽里是什么"，根本不知道其余槽位 → 任何 `InventoryContent`
                // 都可能把玩家真实的物品覆盖掉。所以开界面只报坐标，内容一律不碰。
                if open_inventory {
                    let packets = vec![Protocol::ContainerOpenPacket(Box::new(
                        crate::bedrock::inventory::container_open(last_pos),
                    ))];
                    crate::bedrock_verbose!("[R1] 打开背包：ContainerOpen({:.1},{:.1},{:.1})（不再下发 InventoryContent，避免清空背包）",
                        last_pos.0, last_pos.1, last_pos.2
                    );
                    crate::bedrock::debug::dump_out(&packets);
                    if let Err(e) = conn.send(&packets).await {
                        println!("[!!] 下发背包失败: {e}");
                        break;
                    }
                }

                // 关背包：客户端要一份回执（Geyser 原样回发），不回它下次就不肯再开。
                if let Some((container_id, server_initiated)) = close_inventory {
                    let packets = vec![Protocol::ContainerClosePacket(Box::new(
                        crate::bedrock::inventory::container_close(container_id, server_initiated),
                    ))];
                    crate::bedrock::debug::dump_out(&packets);
                    if let Err(e) = conn.send(&packets).await {
                        println!("[!!] 回执关背包失败: {e}");
                        break;
                    }
                }
            }
            // 会话结束：先把存活标记置 false，MCHPRS 侧下一 tick 就会把这个 Bedrock 玩家清掉。
            alive.store(false, std::sync::atomic::Ordering::Relaxed);
            // **必须走完整的 `close()` 并等它返回**（BUG-032 第 47 条）。
            //
            // 试过"300ms 超时就丢弃连接"，结果是灾难：`raknet-tokio/src/session/mod.rs:46`
            // 里会话任务写的是 `Some(msg) = msg_rx.recv() => {...}`，我们把 `RakSession`
            // drop 掉后 `recv()` 返回 `None`，这个模式匹配不上、tokio 只把分支禁用，
            // **不会**触发 `DisconnectNow`；于是 raknet 服务端的 `session_map` 里那条
            // 旧会话永远不删，同一个 addr（手机退出世界后复用同一 UDP 端口）的新连接
            // 一律被回 `AlreadyConnected`（`raknet/src/server/mod.rs:589`），
            // 客户端就永久卡在"正在连接服务器…"。
            //
            // 代价：这里会等 `CLOSE_TIMEOUT`（约 5~10 秒）才把会话清掉，这段窗口内
            // 立刻重进仍会被拒（基线行为）。根治要改 raknet 那个 match 分支，见 windows-2.txt。
            conn.close().await;
        });
    }
}

/// 在独立线程里起一个 tokio 运行时，跑 **MCHPRS 真实世界**的 Bedrock 端点。
///
/// 由 `crates/core` 的服务端线程在启动时调用一次；`events` 用来向服务端线程要世界快照。
/// `chunk_radius` 是视距（区块），由 core 从 `CONFIG.view_distance` 取来 —— 网络层不读
/// core 的配置，所以只能从这里传进来（BUG-032 第 16 条）。
/// 用独立线程是刻意的：MCHPRS 主循环是同步 tick，Bedrock 侧是 async，两者不要互相阻塞。
pub fn spawn_mchprs_listener(
    addr: String,
    events: mpsc::Sender<BedrockEvent>,
    chunk_radius: i32,
) {
    let spawn_result = std::thread::Builder::new()
        .name("mchprs-bedrock".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(err) => {
                    println!("[!!] Bedrock 监听线程无法创建 tokio 运行时: {err}");
                    return;
                }
            };
            runtime.block_on(async move {
                let source = WorldSource::Mchprs {
                    events,
                    chunk_radius,
                };
                if let Err(err) = serve(addr, source).await {
                    println!("[!!] Bedrock 监听退出: {err}");
                }
            });
        });
    if let Err(err) = spawn_result {
        println!("[!!] Bedrock 监听线程创建失败: {err}");
    }
}

#[cfg(test)]
mod nbt_resource_tests {
    use super::{BIOME_DEFINITIONS_NBT, ENTITY_IDENTIFIERS_NBT, parse_nbt_compound};

    /// BUG-032 第 46 条：两张 Geyser 资源都必须能被自写读取器**整字节**解析。
    ///
    /// 断言只用结构（不写 87/129 这类魔法数字，资源更新时会脆）：
    /// - 两张表都非空；
    /// - 生物群系表的值全是 `Compound`；
    /// - 实体标识表有 `idlist` 且是**非空列表**。
    #[test]
    fn bedrock_nbt_resources_parse() {
        let biomes = parse_nbt_compound(BIOME_DEFINITIONS_NBT);
        let entities = parse_nbt_compound(ENTITY_IDENTIFIERS_NBT);
        // `selftest("nbt")` 在基线里就没有调用点（只编译不执行），这里用同一条语句
        // 给出等价输出，方便 `--nocapture` 抄进 REPORT。
        println!("[SELFTEST] biomes={} entities={}", biomes.len(), entities.len());
        assert!(!biomes.is_empty(), "biome_definitions.dat 解析为空");
        assert!(!entities.is_empty(), "entity_identifiers.dat 解析为空");

        assert!(
            biomes.values().all(|v| matches!(v, nbtx::Value::Compound(_))),
            "生物群系表的值应全部是 Compound"
        );

        match entities.get("idlist") {
            Some(nbtx::Value::List(list)) => assert!(!list.is_empty(), "idlist 不应为空"),
            other => panic!("entity_identifiers.dat 的 idlist 缺失或类型不对: {other:?}"),
        }
    }
}

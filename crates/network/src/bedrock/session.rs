use bedrock::network::compression::Compression;
use bedrock::network::login::{LoginOptions, NetworkSettings, begin_encryption, check_protocol};
use bedrock::network::tokio::error::ConnectionError;
use bedrock::network::tokio::{Connection, Listener};
use bedrock::protocol::v662::enums::{PacketCompressionAlgorithm, PlayStatus};
use bedrock::protocol::v662::packets::{
    NetworkSettingsPacket, PlayStatusPacket, ServerToClientHandshakePacket,
};
use bedrock::protocol::v662::enums::ResourcePackResponse;
use bedrock::protocol::v662::types::{BaseGameVersion, Experiment, Experiments};
use bedrock::protocol::v671::packets::ResourcePackStackPacket;
use bedrock::protocol::v766::packets::ResourcePacksInfoPacket;
use uuid::Uuid;
use bedrock::protocol::v662::enums::{
    ChatRestrictionLevel, Difficulty, EditorWorldType, EducationEditionOffer, GamePublishSetting,
    GameType, GeneratorType, PlayerPermissionLevel, ServerAuthMovementMode, SpawnBiomeType,
};
use bedrock::protocol::v662::packets::{
    ChunkRadiusUpdatedPacket, LevelChunkPacket, NetworkChunkPublisherUpdatePacket, SetTimePacket,
    StartGamePacket, UpdateBlockPacket,
};
use bedrock::protocol::v662::types::BlockPos;
use bedrock::protocol::v662::types::ItemData;
use bedrock::protocol::v662::enums::{CommandPermissionLevel, DataItemType, PlayerPositionMode};
use bedrock::protocol::v685::enums::ActorDataIDs;
use bedrock::protocol::v662::packets::{
    SetActorDataPacket,
    AvailableActorIdentifiersPacket, BiomeDefinitionListPacket, CameraPresetsPacket,
    DimensionDataPacket, GameRulesChangedPacket, MovePlayerPacket, PlayerListPacket,
    SetPlayerGameTypePacket, UpdateAbilitiesPacket, UpdateAdventureSettingsPacket,
    AvailableCommandsPacket, CraftingDataPacket, CreativeContentPacket, ItemComponentPacket,
    PlayerHotbarPacket, SetDifficultyPacket, TrimDataPacket, UnlockedRecipesPacket,
};
use bedrock::protocol::v662::enums::ContainerID;
use bedrock::protocol::v662::types::{
    AdventureSettings, DimensionDefinition, DimensionDefinitionGroup, DimensionDefinitionGroupType,
};
use bedrock::protocol::v662::types::CameraPresets;
use bedrock::protocol::v662::types::{DataItem, PropertySyncData};
use bedrock::protocol::v766::types::{AudioListener, CameraPreset};
use bedrock::protocol::v729::packets::{AttributeData, UpdateAttributesPacket};
use bedrock::protocol::v662::types::{
    SerializedAbilitiesData, SerializedAbilitiesLayer, SerializedLayer,
};
use bedrock::protocol::v748::enums::AuthoritativeMovementMode;
use bedrock::protocol::v748::packets::SetMovementAuthorityPacket;
use bedrock::protocol::v662::types::{GameRuleChanged, Type as GameRuleType};
use bedrock::protocol::v662::types::{
    ActorRuntimeID, ActorUniqueID, ChunkPos, EduSharedUriResource, GameRulesChangedPacketData,
    NetworkBlockPosition, NetworkPermissions, SpawnSettings, SyncedPlayerMovementSettings,
};
use bedrock::protocol::v685::types::LevelSettings;
use bedrock::protocol::{ProtoVersion, Unknown, V766};
use std::time::Instant;

use crate::bedrock::chunk as world;
use crate::bedrock::items_gen;

// Geyser 自带的 Bedrock 数据文件（MIT 项目资源），原样内嵌
const BIOME_DEFINITIONS_NBT: &[u8] = include_bytes!("data/biome_definitions.dat");
const ENTITY_IDENTIFIERS_NBT: &[u8] = include_bytes!("data/entity_identifiers.dat");

fn parse_nbt_compound(bytes: &[u8]) -> std::collections::HashMap<String, nbtx::Value> {
    type Map = std::collections::HashMap<String, nbtx::Value>;
    // Geyser 的 .dat 资源是 Bedrock 网络变体（varint 字符串长度），先按它解析
    let mut s1 = bytes;
    if let Ok(v) = nbtx::from_bytes::<nbtx::VarintEndian, Map>(&mut s1) {
        return v;
    }
    let mut s2 = bytes;
    if let Ok(v) = nbtx::from_bytes::<byteorder::LittleEndian, Map>(&mut s2) {
        println!("[::] NBT ?LittleEndian 解析成功");
        return v;
    }
    let mut s3 = bytes;
    if let Ok(v) = nbtx::from_bytes::<byteorder::BigEndian, Map>(&mut s3) {
        println!("[::] NBT ?BigEndian 解析成功");
        return v;
    }
    println!("[!!] NBT 三种变体都解析失败");
    std::collections::HashMap::new()
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

fn pname(p: &Protocol) -> &'static str {
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
        CommandRequestPacket(_) => "CommandRequest",
        MovePlayerPacket(_) => "MovePlayer",
        SubChunkRequestPacket(_) => "SubChunkRequest",
        ClientCacheBlobStatusPacket(_) => "ClientCacheBlobStatus",
        SetLocalPlayerAsInitializedPacket(_) => "SetLocalPlayerAsInitialized",
        _ => "Other",
    }
}

fn log(step: &str, start: Instant, dir: &str, msg: &str) {
    println!(
        "[{step}] {:>8.3} {dir} {msg}",
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
        default_spawn_block_position: NetworkBlockPosition { x: 0, y: 79, z: 0 },
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

fn build_start_game() -> StartGamePacket<Protocol> {
    StartGamePacket {
        target_actor_id: ActorUniqueID(1),
        target_runtime_id: ActorRuntimeID(1),
        actor_game_type: GameType::Creative,
        position: (0.5, 82.0, 0.5),
        rotation: (0.0, 0.0),
        settings: build_level_settings(),
        level_id: String::from("mchprs-spike"),
        level_name: String::from("MCHPRS Spike"),
        template_content_identity: String::new(),
        is_trial: false,
        movement_settings: SyncedPlayerMovementSettings {
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
async fn do_login(
    conn: &mut Connection<Protocol>,
    start: Instant,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    let start_game = build_start_game();
    conn.send(&[Protocol::StartGamePacket(Box::new(start_game))])
        .await?;
    log("B4", start, ">>", "StartGame");

    // ?Geyser 一致：模式/能力包在 spawn 之后再发（见 C2?

    // ---------- C1: 区块半径握手 ----------
    loop {
        let batch = recv_logged(conn, "C1", start).await?;
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
                ChunkRadiusUpdatedPacket { chunk_radius: 8 },
            ))])
            .await?;
            log("C1", start, ">>", "ChunkRadiusUpdated(8)");
            break;
        }
    }

    // ---------- C2: 单区块 + 一块地板 ----------
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

    // 参考 Geyser 的时序：先让客户端进入世界，等它自报初始化完成，再发区块。
    // Geyser 的算法：publisher 半径以“方块”为单位，约等于 (视距+1)*sqrt(2)*16
    const RADIUS: i32 = 8;
    let view_radius_blocks: u32 = (((RADIUS + 1) as f64) * std::f64::consts::SQRT_2).ceil() as u32 * 16;
    conn.send(&[play_status(PlayStatus::PlayerSpawn)]).await?;
    log("C2", start, ">>", "PlayStatus(PlayerSpawn)");

    // Geyser 原文注释：客户端在收到属性包之前会以离谱的速度移动，必须下发移动速度
    conn.send(&[Protocol::UpdateAttributesPacket(Box::new(
        UpdateAttributesPacket::<Protocol> {
            target_runtime_id: ActorRuntimeID(1),
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
        },
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
        build_update_abilities(),
    ))])
    .await?;
    log("C2", start, ">>", "UpdateAbilities");

    // 玩家实体元数据：等价于 Geyser 的 Entity.initializeMetadata() + SessionPlayerEntity.setAirSupply()
    conn.send(&[Protocol::SetActorDataPacket(Box::new(
        SetActorDataPacket::<Protocol> {
            target_runtime_id: ActorRuntimeID(1),
            actor_data: player_metadata(),
            synced_properties: PropertySyncData {
                int_entries_list: vec![],
                float_entries_list: vec![],
            },
            tick: 0,
        },
    ))])
    .await?;
    log("C2", start, ">>", "SetActorData(玩家初始元数据)");
    // 强制传送到地面上方，避免出生在地形内部
    conn.send(&[Protocol::MovePlayerPacket(Box::new(
        MovePlayerPacket::<Protocol> {
            player_runtime_id: ActorRuntimeID(1),
            position: (0.5, 82.0, 0.5),
            rotation: (0.0, 0.0),
            y_head_rotation: 0.0,
            position_mode: PlayerPositionMode::Teleport {
                teleportation_cause: 0,
                source_actor_type: 0,
            },
            on_ground: false,
            riding_runtime_id: ActorRuntimeID(0),
            tick: 0,
        },
    ))])
    .await?;
    log("C2", start, ">>", "MovePlayer(Teleport 0.5,82,0.5)");

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

    conn.send(&[Protocol::AvailableCommandsPacket(Box::new(
        AvailableCommandsPacket::<Protocol> {
            enum_values: vec![],
            sub_command_values: vec![],
            post_fixes: vec![],
            enum_data: vec![],
            chained_sub_command_data: vec![],
            commands: vec![],
            soft_enums: vec![],
            constraints: vec![],
        },
    ))])
    .await?;
    log("C2", start, ">>", "AvailableCommands(空)");

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

    let biomes = parse_nbt_compound(BIOME_DEFINITIONS_NBT);
    let biome_count = biomes.len();
    if biome_count > 0 {
        conn.send(&[Protocol::BiomeDefinitionListPacket(Box::new(
            BiomeDefinitionListPacket {
                biome_definition_data: biomes,
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

    let entities = parse_nbt_compound(ENTITY_IDENTIFIERS_NBT);
    let entity_count = entities.len();
    if entity_count > 0 {
        conn.send(&[Protocol::AvailableActorIdentifiersPacket(Box::new(
            AvailableActorIdentifiersPacket {
                actor_info_list: entities,
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

    // 创造模式物品列表。我们暂时不提供（创造模式下的背包列表会为空），
    // 但空表是合法编码，且位置必须与 Geyser 一致（紧跟在 CameraPresets 之后）。
    conn.send(&[Protocol::CreativeContentPacket(Box::new(
        CreativeContentPacket::<Protocol> {
            write_entries: vec![],
        },
    ))])
    .await?;
    log("C2", start, ">>", "CreativeContent(空)");

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

    conn.send(&[Protocol::NetworkChunkPublisherUpdatePacket(Box::new(
        NetworkChunkPublisherUpdatePacket {
            new_view_position: BlockPos { x: 0, y: 90, z: 0 },
            new_view_radius: view_radius_blocks,
            server_built_chunks: vec![],
        },
    ))])
    .await?;
    log(
        "C2",
        start,
        ">>",
        &format!("NetworkChunkPublisherUpdate(0,90,0 r={view_radius_blocks} blocks)"),
    );

    let mut sent = 0;
    for cx in -RADIUS..=RADIUS {
        for cz in -RADIUS..=RADIUS {
            conn.send(&[Protocol::LevelChunkPacket(Box::new(LevelChunkPacket {
                chunk_position: ChunkPos { x: cx, z: cz },
                dimension_id: 0,
                sub_chunk_count: count,
                sub_chunk_limit: 0,
                cache_enabled: false,
                cache_blobs: vec![],
                serialized_chunk_data: data.clone(),
            }))])
            .await?;
            sent += 1;
        }
    }
    log(
        "C2",
        start,
        "OK",
        &format!(
            "已发送 {} 个区块（每个 {} 字节，sub_chunk_count={}）",
            sent,
            data.len(),
            count
        ),
    );

    // 判别实验：出生点周围一圈放 12 个不同 state id 的方块，看有没有任何一个被渲染出来
    const CANDIDATES: [u32; 12] = [
        2403, 2404, 2405, 2399, 2400, 1, 100, 1000, 5000, 5588, 9362, 11996,
    ];
    let ring: [(i32, i32); 12] = [
        (2, 0), (2, 2), (0, 2), (-2, 2), (-2, 0), (-2, -2),
        (0, -2), (2, -2), (4, 0), (0, 4), (-4, 0), (0, -4),
    ];
    for (i, id) in CANDIDATES.iter().enumerate() {
        let (dx, dz) = ring[i];
        conn.send(&[Protocol::UpdateBlockPacket(Box::new(UpdateBlockPacket {
            block_position: NetworkBlockPosition { x: dx, y: 80, z: dz },
            block_runtime_id: *id,
            flags: 0,
            layer: 0,
        }))])
        .await?;
    }
    log("C2", start, "OK", "已放置 12 个候选方块（y=80 一圈）");

    conn.send(&[Protocol::SetTimePacket(Box::new(SetTimePacket { time: 6000 }))])
        .await?;
    log("C2", start, ">>", "SetTime(6000)");

    Ok(())
}

/// 与 Geyser sendAdventureSettings 等价的能力包（Layer: Base）
fn build_update_abilities() -> UpdateAbilitiesPacket<Protocol> {
    const ABILITY_SET: u32 = 0x0007_FFFF; // 后端声明“支持全部能力”，与 Geyser 一致
    // 创造模式 + 允许飞行 + 正在飞行 + 无限资源。位号由 Cloudburst v766 codec 实测
    // （refs/ProbeCreativeAbilities.java）：BUILD0 MINE1 DOORS2 OPEN3 ATK_PLAYERS4 ATK_MOBS5
    // TELEPORT7 INVULNERABLE8 FLYING9 MAY_FLY10 INSTABUILD11
    const ABILITY_VALUES: u32 = 0x0FBF;
    UpdateAbilitiesPacket::<Protocol> {
        data: SerializedAbilitiesData {
            target_player_raw_id: 1,
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
        let pkt = build_update_abilities();
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

    if mode == "2" {
        println!("[SELFTEST] nbtx ?map");
        let m: std::collections::HashMap<String, nbtx::Value> = Default::default();
        let mut cur = std::io::Cursor::new(Vec::new());
        match nbtx::to_le_bytes_in(&mut cur, &m) {
            Ok(()) => println!("[SELFTEST] OK empty map bytes={}", cur.get_ref().len()),
            Err(e) => println!("[SELFTEST] ERR map: {e:?}"),
        }
        return;
    }
    if mode == "3" {
        println!("[SELFTEST] nbtx 单条 map");
        let mut m: std::collections::HashMap<String, nbtx::Value> = Default::default();
        m.insert(String::from("x"), nbtx::Value::Int(1));
        let mut cur = std::io::Cursor::new(Vec::new());
        match nbtx::to_le_bytes_in(&mut cur, &m) {
            Ok(()) => println!("[SELFTEST] OK 1-entry map bytes={}", cur.get_ref().len()),
            Err(e) => println!("[SELFTEST] ERR map: {e:?}"),
        }
        return;
    }
    if mode == "5" {
        println!("[SELFTEST] nbtx 基础类型 i32");
        let mut cur = std::io::Cursor::new(Vec::new());
        match nbtx::to_le_bytes_in(&mut cur, &7i32) {
            Ok(()) => println!("[SELFTEST] OK i32 bytes={}", cur.get_ref().len()),
            Err(e) => println!("[SELFTEST] ERR i32: {e:?}"),
        }
        return;
    }
    if mode == "6" {
        println!("[SELFTEST] nbtx String");
        let mut cur = std::io::Cursor::new(Vec::new());
        match nbtx::to_le_bytes_in(&mut cur, &String::from("hi")) {
            Ok(()) => println!("[SELFTEST] OK String bytes={}", cur.get_ref().len()),
            Err(e) => println!("[SELFTEST] ERR String: {e:?}"),
        }
        return;
    }
    if mode == "7" {
        println!("[SELFTEST] StartGame 往返");
        let pkt = build_start_game();
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
        let pkt = build_start_game();
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
    let pkt = build_start_game();
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

pub async fn serve(addr: String) -> Result<(), Box<dyn std::error::Error>> {
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
        tokio::spawn(async move {
            let start = Instant::now();
            let mut conn = conn.into_ver::<Protocol>();
            println!("[A3] ACCEPT peer");
            match do_login(&mut conn, start).await {
                Ok(()) => println!("[B4] 登录链路完成，等待后续阶段"),
                Err(e) => println!("[!!] 失败: {e}"),
            }
            // 保持连接，继续打印收包（B4 之前用于观察）
            // 用客户端自报坐标实测水平速度：客观判断“移动是否真的变慢”
            // 参考值：原版行走 4.317 m/s，潜行 1.31 m/s，游泳 2.2 m/s
            let mut last_move: Option<((f32, f32, f32), Instant)> = None;
            loop {
                match conn.recv().await {
                    Ok(batch) => {
                        for p in &batch {
                            match p {
                                Protocol::PlayerAuthInputPacket(pa) => {
                                    // S3：客户端的方块交互（点按/挖掘/放置）走 PerformBlockActions 位
                                    if let Some(actions) = &pa.player_block_actions {
                                        for a in actions {
                                            println!(
                                                "[S3] << 方块交互 action={:?} pos={:?} facing={:?}",
                                                a.action_type, a.position, a.facing
                                            );
                                        }
                                    }
                                    println!(
                                        "[--] << PlayerAuthInput pos=({:.2}, {:.2}, {:.2}) input=0x{:x}",
                                        pa.player_position.0,
                                        pa.player_position.1,
                                        pa.player_position.2,
                                        pa.input_data
                                    )
                                }
                                Protocol::InteractPacket(ip) => println!(
                                    "[S3] << Interact action={:?} target={:?}",
                                    ip.action, ip.target_runtime_id
                                ),
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
                                    match speed {
                                        Some(v) => println!(
                                            "[--] << MovePlayer pos=({:.2}, {:.2}, {:.2}) on_ground={} 水平速度={:.2} m/s",
                                            mp.position.0, mp.position.1, mp.position.2, mp.on_ground, v
                                        ),
                                        None => println!(
                                            "[--] << MovePlayer pos=({:.2}, {:.2}, {:.2}) on_ground={}",
                                            mp.position.0, mp.position.1, mp.position.2, mp.on_ground
                                        ),
                                    }
                                    // 服务端权威移动：把位置确认回给客户端
                                    let echo = MovePlayerPacket::<Protocol> {
                                        player_runtime_id: ActorRuntimeID(1),
                                        position: mp.position,
                                        rotation: mp.rotation,
                                        y_head_rotation: mp.y_head_rotation,
                                        position_mode: mp.position_mode.clone(),
                                        on_ground: mp.on_ground,
                                        riding_runtime_id: ActorRuntimeID(0),
                                        tick: mp.tick,
                                    };
                                    if let Err(e) = conn
                                        .send(&[Protocol::MovePlayerPacket(Box::new(echo))])
                                        .await
                                    {
                                        println!("[!!] 回显 MovePlayer 失败: {e}");
                                        break;
                                    }
                                }
                                Protocol::SubChunkRequestPacket(_) => {
                                    println!("[--] << SubChunkRequest")
                                }
                                Protocol::Unknown(u) => println!("[--] << Unknown id=0x{:x}", u.id),
                                other => println!("[--] << {}", pname(other)),
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            conn.close().await;
        });
    }
}

use crate::config::CONFIG;
use crate::permissions::{self, PlayerPermissionsCache};
use crate::plot::worldedit::WorldEditUndo;
use crate::plot::PLOT_SCALE;
use crate::utils::{self, HyphenatedUUID};
use byteorder::{BigEndian, ReadBytesExt};
use mchprs_blocks::block_entities::{ContainerType, InventoryEntry};
use mchprs_blocks::items::{Item, ItemStack};
use mchprs_blocks::{BlockDirection, BlockFacing, BlockPos};
use mchprs_network::packets::clientbound::*;
use mchprs_network::packets::{PacketEncoder, PlayerProperty, SlotData};
use mchprs_network::{PlayerConn, PlayerPacketSender};
use mchprs_proc_macros::protocol_id;
use mchprs_schematic::WorldEditClipboard;
use mchprs_text::{ColorCode, TextComponent, TextComponentBuilder};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Instant, SystemTime};
use tracing::{error, warn};

pub type EntityId = u32;
static ENTITY_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum Gamemode {
    Creative,
    Spectator,
}

impl Gamemode {
    pub fn get_id(self) -> i32 {
        match self {
            Gamemode::Creative => 1,
            Gamemode::Spectator => 3,
        }
    }
}

/// This structure represents how the player will be
/// serialized when saved to it's file.
#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerData {
    on_ground: bool,
    flying: bool,
    motion: [f64; 3],
    position: [f64; 3],
    rotation: [f32; 2],
    inventory: Vec<InventoryEntry>,
    selected_item_slot: i32,
    fly_speed: f32,
    walk_speed: f32,
    gamemode: Gamemode,
}

impl Default for PlayerData {
    fn default() -> PlayerData {
        PlayerData {
            on_ground: true,
            flying: false,
            motion: [0.0, 0.0, 0.0],
            position: [128.0, 128.0, 128.0],
            rotation: [0.0, 0.0],
            inventory: Vec::new(),
            selected_item_slot: 0,
            fly_speed: 1.0,
            walk_speed: 1.0,
            gamemode: Gamemode::Creative,
        }
    }
}

bitflags! {
    #[derive(Debug)]
    pub struct SkinParts: u32 {
        const CAPE = 0x01;
        const JACKET = 0x02;
        const LEFT_SLEEVE = 0x04;
        const RIGHT_SLEEVE = 0x08;
        const LEFT_PANTS_LEG = 0x10;
        const RIGHT_PANTS_LEG = 0x20;
        const HAT = 0x40;
    }
}

impl Default for SkinParts {
    fn default() -> Self {
        Self::from_bits(0b1111111).unwrap()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerPos {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl PlayerPos {
    pub fn new(x: f64, y: f64, z: f64) -> PlayerPos {
        PlayerPos { x, y, z }
    }

    pub fn block_pos(self) -> BlockPos {
        BlockPos {
            x: self.x.floor() as i32,
            y: self.y.floor() as i32,
            z: self.z.floor() as i32,
        }
    }

    pub fn chunk_pos(self) -> (i32, i32) {
        (self.x.floor() as i32 >> 4, self.z.floor() as i32 >> 4)
    }

    pub fn plot_pos(self) -> (i32, i32) {
        let (chunk_x, chunk_z) = self.chunk_pos();
        (chunk_x >> PLOT_SCALE, chunk_z >> PLOT_SCALE)
    }
}

impl std::fmt::Display for PlayerPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.z)
    }
}

/// 玩家连接的后端。
///
/// Java 后端就是原来那条 TCP 连接；Bedrock 后端是一条通往 Bedrock 会话线程的句柄。
/// 这个类型**实现了与 `PlayerConn` 同名的 `send_packet` / `alive` / `close_connection`**，
/// 因此 `plot` 里现有的 `player.client.send_packet(..)` 之类调用不需要改。
///
/// 见 BACKLOG.md BUG-032「玩家化」：Bedrock 客户端从现在起是 `plot.players` 里的真玩家，
/// 位置、权限、命令、计分板都复用已有逻辑；尚不支持的功能（Java 包）在 Bedrock 后端是 no-op。
pub enum PlayerBackend {
    Java(PlayerConn),
    Bedrock(mchprs_network::bedrock::bridge::BedrockPlayerLink),
}

impl PlayerBackend {
    /// Java 包对 Bedrock 客户端没有意义，所以 Bedrock 后端直接丢弃。
    pub fn send_packet(&self, data: &mchprs_network::packets::PacketEncoder) {
        if let Self::Java(conn) = self {
            conn.send_packet(data);
        }
    }

    pub fn alive(&self) -> bool {
        match self {
            Self::Java(conn) => conn.alive(),
            Self::Bedrock(link) => link.handle.alive(),
        }
    }

    pub fn close_connection(&mut self) {
        match self {
            Self::Java(conn) => conn.close_connection(),
            Self::Bedrock(link) => link.handle.close_connection(),
        }
    }

    /// 只有 Java 后端能加入 Java 的方块广播列表。
    pub fn as_java(&self) -> Option<&PlayerConn> {
        match self {
            Self::Java(conn) => Some(conn),
            Self::Bedrock(_) => None,
        }
    }

    /// 是否是 Bedrock 客户端（例如 keep-alive 超时逻辑要跳过它）。
    pub fn is_bedrock(&self) -> bool {
        matches!(self, Self::Bedrock(_))
    }

    /// 推 Bedrock 计分板（Java 后端不用：Java 走自己的 `CUpdateScore` 那一套）。
    ///
    /// 出站方向是**语义化**的：core 不构造 Bedrock 包，只把"要显示什么"交给会话线程
    /// （见 BACKLOG.md BUG-032「入站转义 / 出站语义化」）。
    pub fn send_sidebar(&self, sidebar: &mchprs_network::bedrock::bridge::BedrockSidebar) {
        if let Self::Bedrock(link) = self {
            link.handle
                .send(mchprs_network::bedrock::bridge::BedrockCommand::Sidebar(
                    sidebar.clone(),
                ));
        }
    }

    /// 往聊天框推一行文本（`text` 可带 `§` 颜色码）。Java 后端不需要走这里。
    pub fn send_chat(&self, text: &str) {
        if let Self::Bedrock(link) = self {
            link.handle
                .send(mchprs_network::bedrock::bridge::BedrockCommand::Chat {
                    text: text.to_string(),
                });
        }
    }

    /// 推"线上其它玩家"的快照（BUG-032 第 22 条完整版）。
    ///
    /// Java 后端不用：Java 客户端本来就是靠 `CPlayerInfoUpdate` + `CSpawnEntity` 那一套
    /// 由服务端自己处理的。Bedrock 侧没有等价的通用实现，所以由 `plot` 每 tick 推一份
    /// 全量列表，会话线程自己 diff（生成/移动/销毁）。
    pub fn send_players(
        &self,
        players: Vec<mchprs_network::bedrock::bridge::BedrockPlayerInfo>,
    ) {
        if let Self::Bedrock(link) = self {
            link.handle.send_players(players);
        }
    }

    /// 推一批区块（BUG-032 第 26 条增量区块 + 第 19 条）。
    ///
    /// Java 后端不用：区块由 `CChunkData` 那套自己发。Bedrock 侧由 `plot` 按每 tick
    /// 的预算生成，这里只把"发什么"投给会话线程。
    pub fn send_chunks(&self, batch: mchprs_network::bedrock::bridge::BedrockChunkBatch) {
        if let Self::Bedrock(link) = self {
            link.handle.send_chunks(batch);
        }
    }

    /// 推软枚举取值（BUG-032 B4 动态补全）。
    ///
    /// Bedrock 客户端不会向服务端要补全，候选值只能这样推过去；Java 后端不用。
    pub fn send_soft_enum(&self, name: &str, values: Vec<String>) {
        if let Self::Bedrock(link) = self {
            link.handle.send_soft_enum(name, values);
        }
    }

    /// 单个方块变化（BUG-032 第 3 条）。
    ///
    /// 以前这里只有 Java 的 `CBlockUpdate`，Bedrock 玩家不在广播列表里，
    /// 于是"挖掉方块后自己那边不消失"。现在两种后端都走这一个入口：
    /// Java 照旧发 `CBlockUpdate`，Bedrock 走语义化指令由会话线程转成 `UpdateBlock`。
    ///
    /// `java_state` 是 MCHPRS（Java 1.20.4）的方块状态 id，**不要**在这里换算成 Bedrock
    /// 运行时 id —— 那是网络层的事（`bedrock_block_state` + `y - 64`）。
    pub fn send_block_update(&self, pos: BlockPos, java_state: u32) {
        match self {
            Self::Java(conn) => {
                let packet = CBlockUpdate {
                    block_id: java_state as i32,
                    x: pos.x,
                    y: pos.y,
                    z: pos.z,
                }
                .encode();
                conn.send_packet(&packet);
            }
            Self::Bedrock(_) => self.send_block_updates(&[(pos.x, pos.y, pos.z, java_state)]),
        }
    }

    /// 一批方块变化（`(x, y, z, java_state)`，MCHPRS 坐标/状态）。
    ///
    /// Java 后端逐条发 `CBlockUpdate`（Java 侧真正的批量包是 `CMultiBlockChange`，
    /// 由 plot 自己处理，不走这个入口）；Bedrock 后端把整批交给会话线程。
    pub fn send_block_updates(&self, changes: &[(i32, i32, i32, u32)]) {
        if let Self::Bedrock(link) = self {
            link.handle.send_block_updates(changes);
        }
    }

    /// 能力值变化（飞行状态 + 飞行速度，跟随 Java 侧；BUG-032「能力值不跟随后端状态」）。
    ///
    /// Java 后端由调用方另外发 `CPlayerAbilities`；这里只负责 Bedrock 那一份，
    /// 避免把"Java 包"和"Bedrock 语义"混在同一个分支里。
    /// `fly_speed` 传**绝对值**（Java 侧的 `0.05 * 倍率`），不是倍率。
    pub fn send_abilities(&self, flying: bool, fly_speed: f32) {
        if let Self::Bedrock(link) = self {
            link.handle.send_abilities(flying, fly_speed);
        }
    }

    /// 服务端强制定位（BUG-032 第 8 条）：Java 发 `CSynchronizePlayerPosition`，
    /// Bedrock 发 `MovePlayer(Teleport)`。以前 Bedrock 分支是静默丢弃，
    /// `/tp`、服务端纠正位置对 Bedrock 玩家完全无效。
    pub fn send_teleport(&self, x: f64, y: f64, z: f64) {
        if let Self::Bedrock(link) = self {
            link.handle.send_teleport(x, y, z);
        }
    }

    /// 与 `PlayerConn::receive_packets` 同名。
    ///
    /// Bedrock 后端返回的是**会话线程转义好的 Java 入站包**
    /// （`bedrock::translate`），所以 `plot` 的 `handle_packets_for_player`
    /// 会用同一套逻辑处理 Bedrock 玩家，无需分支。
    pub fn receive_packets(
        &mut self,
    ) -> Vec<Box<dyn mchprs_network::packets::serverbound::ServerBoundPacket>> {
        match self {
            Self::Java(conn) => conn.receive_packets(),
            Self::Bedrock(link) => {
                let mut packets = Vec::new();
                while let Ok(packet) = link.packets.try_recv() {
                    packets.push(packet);
                }
                packets
            }
        }
    }
    /// 取走 Bedrock 会话线程同步过来的背包状态（Java 后端恒为空）。
    ///
    /// Bedrock 的背包在创造模式下是客户端权威，服务端不维护完整背包；会话线程只在
    /// "要读手里物品"（放方块/交互）之前把那一格同步过来，见
    /// `mchprs_network::bedrock::bridge::BedrockInventoryUpdate`。
    /// **顺序要求**：调用方必须先排空它、再处理 `receive_packets()` 的包。
    pub fn take_inventory_updates(
        &mut self,
    ) -> Vec<mchprs_network::bedrock::bridge::BedrockInventoryUpdate> {
        match self {
            Self::Java(_) => Vec::new(),
            Self::Bedrock(link) => {
                let mut updates = Vec::new();
                while let Ok(update) = link.inventory.try_recv() {
                    updates.push(update);
                }
                updates
            }
        }
    }
}

pub struct Player {
    pub uuid: u128,
    pub username: String,
    pub properties: Vec<PlayerProperty>,
    pub skin_parts: SkinParts,
    pub inventory: Vec<Option<ItemStack>>,
    /// The selected slot of the player's hotbar (1-9)
    pub selected_slot: u32,
    pub pos: PlayerPos,
    /// The last X chunk the player was in. This is used for updated view position.
    pub last_chunk_x: i32,
    /// The last Z chunk the player was in. This is used for updated view position.
    pub last_chunk_z: i32,
    /// The player's head yaw rotation.
    pub yaw: f32,
    /// The player's head pitch rotation.
    pub pitch: f32,
    pub flying: bool,
    pub sprinting: bool,
    pub crouching: bool,
    pub on_ground: bool,
    pub fly_speed: f32,
    pub walk_speed: f32,
    pub gamemode: Gamemode,
    pub entity_id: EntityId,
    pub client: PlayerBackend,
    /// The last time the keep alive packet was received.
    pub last_keep_alive_received: Instant,
    /// The last time the keep alive packet was sent.
    last_keep_alive_sent: Instant,
    /// The worldedit first position.
    pub first_position: Option<BlockPos>,
    /// The worldedit second position.
    pub second_position: Option<BlockPos>,
    /// The worldedit current clipboard.
    pub worldedit_clipboard: Option<WorldEditClipboard>,
    /// The saved sections used for worldedit //undo
    /// Each entry stores the plot coords and the clipboard
    pub worldedit_undo: Vec<WorldEditUndo>,
    pub worldedit_redo: Vec<WorldEditUndo>,
    /// Commands are stored so they can be handled after packets
    pub command_queue: Vec<String>,
    permissions_cache: Option<PlayerPermissionsCache>,
}

impl fmt::Debug for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Player")
            .field("username", &self.username)
            .field("uuid", &HyphenatedUUID(self.uuid).to_string())
            .finish()
    }
}

impl Player {
    pub fn generate_offline_uuid(username: &str) -> u128 {
        Cursor::new(md5::compute(format!("OfflinePlayer:{}", username)).0)
            .read_u128::<BigEndian>()
            .unwrap()
            // Encode version and varient into uuid
            & (!(0xC << 60) & !(0xF << 76))
            | ((0x8 << 60) | (0x3 << 76))
    }

    fn from_data(
        player_data: PlayerData,
        uuid: u128,
        username: String,
        properties: Vec<PlayerProperty>,
        client: PlayerBackend,
    ) -> Player {
        // Load inventory
        let mut inventory: Vec<Option<ItemStack>> = vec![None; 46];
        for entry in player_data.inventory {
            let nbt = entry
                .nbt
                .map(|data| nbt::Blob::from_reader(&mut Cursor::new(data)).unwrap());
            inventory[entry.slot as usize] = Some(ItemStack {
                item_type: Item::from_id(entry.id),
                count: entry.count as u8,
                nbt,
            });
        }
        let permissions_cache = CONFIG
            .luckperms
            .is_some()
            .then(|| permissions::load_player_cache(uuid).unwrap());
        Player {
            uuid,
            username,
            properties,
            skin_parts: Default::default(),
            inventory,
            selected_slot: player_data.selected_item_slot as u32,
            pos: PlayerPos {
                x: player_data.position[0],
                y: player_data.position[1],
                z: player_data.position[2],
            },
            pitch: player_data.rotation[0],
            yaw: player_data.rotation[1],
            last_chunk_x: 0,
            last_chunk_z: 0,
            entity_id: ENTITY_ID_COUNTER.fetch_add(1, Ordering::Relaxed),
            client,
            flying: player_data.flying,
            sprinting: false,
            crouching: false,
            gamemode: player_data.gamemode,
            on_ground: player_data.on_ground,
            walk_speed: player_data.walk_speed,
            fly_speed: player_data.fly_speed,
            last_keep_alive_received: Instant::now(),
            last_keep_alive_sent: Instant::now(),
            first_position: None,
            second_position: None,
            worldedit_clipboard: None,
            worldedit_undo: Vec::new(),
            worldedit_redo: Vec::new(),
            command_queue: Vec::new(),
            permissions_cache,
        }
    }

    /// This will load the player from the file. If the file does not exist,
    /// It will be created.
    pub fn load_player(
        uuid: u128,
        username: String,
        properties: Vec<PlayerProperty>,
        client: PlayerBackend,
    ) -> Player {
        let filename = format!("./world/players/{:032x}", uuid);
        if let Ok(data) = fs::read(&filename) {
            let player_data: PlayerData = match bincode::deserialize(&data) {
                Ok(data) => data,
                Err(_) => {
                    warn!("There was an error loading the player data for {}, player data will be backed up and reset.", username);
                    if let Err(err) = fs::rename(&filename, filename.clone() + ".bak") {
                        error!("Failed to back up player data: {}", err);
                    }
                    return Player::from_data(
                        Default::default(),
                        uuid,
                        username,
                        properties,
                        client,
                    );
                }
            };

            Player::from_data(player_data, uuid, username, properties, client)
        } else {
            Player::from_data(Default::default(), uuid, username, properties, client)
        }
    }

    /// 这个 UUID 是否已经有存档文件。
    ///
    /// Bedrock 玩家进服时要判断"该用存档位置还是默认出生点"（BUG-032 第 32 条）：
    /// 以前无条件用 Bedrock 会话带的默认出生点覆盖，导致每次进服都从天上掉下来。
    pub fn save_exists(uuid: u128) -> bool {
        std::path::Path::new(&format!("./world/players/{:032x}", uuid)).exists()
    }

    /// 只读存档里的坐标（不构造完整 `Player`）。返回值是 **MCHPRS（Java）坐标**。
    ///
    /// Bedrock 登录在**发区块之前**就要知道出生点：有存档就直接出生在存档位置，
    /// 否则客户端先出生在默认点、再被传送一百多格，相机/视场角状态就乱了
    /// （BUG-032 第 31 条）。
    pub fn saved_position(uuid: u128) -> Option<PlayerPos> {
        let bytes = std::fs::read(format!("./world/players/{:032x}", uuid)).ok()?;
        let data: PlayerData = bincode::deserialize(&bytes).ok()?;
        Some(PlayerPos {
            x: data.position[0],
            y: data.position[1],
            z: data.position[2],
        })
    }

    /// Saves the player to `./world/players/{uuid}`. This will create
    /// the file if it does not already exist.
    pub fn save(&self) {
        // `truncate(true)` 不能少（BUG-032 第 24 条）：bincode 的结构大小随内容变化，
        // 不截断时"新数据比旧数据短"会在文件尾部留下上次的残字节，下次
        // `bincode::deserialize` 读到多余字节会失败，玩家数据就被备份并重置成默认值。
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(format!("./world/players/{:032x}", self.uuid))
            .unwrap();
        let mut inventory: Vec<InventoryEntry> = Vec::new();
        for (slot, item_option) in self.inventory.iter().enumerate() {
            if let Some(item) = item_option {
                let nbt = item.nbt.clone().map(|blob| {
                    let mut data = Vec::new();
                    blob.to_writer(&mut data).unwrap();
                    data
                });
                inventory.push(InventoryEntry {
                    count: item.count as i8,
                    id: item.item_type.get_id(),
                    slot: slot as i8,
                    nbt,
                });
            }
        }
        let data = bincode::serialize(&PlayerData {
            fly_speed: self.fly_speed,
            flying: self.flying,
            gamemode: self.gamemode,
            inventory,
            motion: [0f64, 0f64, 0f64],
            on_ground: self.on_ground,
            position: [self.pos.x, self.pos.y, self.pos.z],
            rotation: [self.pitch, self.yaw],
            selected_item_slot: self.selected_slot as i32,
            walk_speed: self.walk_speed,
        })
        .unwrap();
        file.write_all(&data).unwrap();
    }

    /// Manages keep alives and packet reading. Return true if the view position should be updated.
    pub fn update(&mut self) -> bool {
        // Bedrock 后端没有 Java 的 keep-alive 往返，超时逻辑必须跳过，
        // 否则 Bedrock 玩家会在 30 秒后被当成超时踢掉（BACKLOG.md BUG-032「玩家化」）。
        if !self.client.is_bedrock() {
            if self.last_keep_alive_received.elapsed().as_secs() > 30 {
                self.kick("Timed out.".into());
            }
            if self.last_keep_alive_sent.elapsed().as_secs() > 10 {
                self.send_keep_alive();
            }
        }

        // Prevent from locking player position at Infinity or NaN
        if !self.pos.x.is_finite() || !self.pos.y.is_finite() || !self.pos.z.is_finite() {
            self.pos.x = 128.0;
            self.pos.y = 128.0;
            self.pos.z = 128.0;
        }

        let (chunk_x, chunk_z) = self.pos.chunk_pos();
        chunk_x != self.last_chunk_x || chunk_z != self.last_chunk_z
    }

    /// Sends the keep alive packet to the client and updates `last_keep_alive_sent`
    pub fn send_keep_alive(&mut self) {
        let keep_alive = CKeepAlive {
            id: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        }
        .encode();
        self.client.send_packet(&keep_alive);
        self.last_keep_alive_sent = Instant::now();
    }

    pub fn get_direction(&self) -> BlockDirection {
        match ((self.yaw / 90.0 + 0.5).floor() as i32 & 3).unsigned_abs() {
            0 => BlockDirection::South,
            1 => BlockDirection::West,
            2 => BlockDirection::North,
            3 => BlockDirection::East,
            _ => BlockDirection::South,
        }
    }

    pub fn get_facing(&self) -> BlockFacing {
        let yaw = self.yaw.rem_euclid(360.0);
        let pitch = self.pitch;
        if pitch <= -70.0 {
            BlockFacing::Up
        } else if pitch >= 70.0 {
            BlockFacing::Down
        } else if (45.0..=135.0).contains(&yaw) {
            BlockFacing::West
        } else if (135.0..=225.0).contains(&yaw) {
            BlockFacing::North
        } else if (225.0..=315.0).contains(&yaw) {
            BlockFacing::East
        } else {
            BlockFacing::South
        }
    }

    pub fn teleport(&mut self, pos: PlayerPos) {
        // Prevent from teleporting to Infinity or NaN
        if !pos.x.is_finite() || !pos.y.is_finite() || !pos.z.is_finite() {
            self.send_error_message("We just saved you from a game crash, don't try it again!");
            return;
        }

        let player_position_and_look = CSynchronizePlayerPosition {
            x: pos.x,
            y: pos.y,
            z: pos.z,
            yaw: 0f32,
            pitch: 0f32,
            flags: 0x08 | 0x10, // pitch and yaw are relative
            teleport_id: 0,
        }
        .encode();
        self.pos = pos;
        self.client.send_packet(&player_position_and_look);
        // Bedrock 客户端不认 Java 包，以前这一发等于丢弃（BUG-032 第 8 条）。
        // 语义化交给会话线程：它换成 `MovePlayer(Teleport)`，并用**本连接自己的** runtime id。
        self.client.send_teleport(pos.x, pos.y, pos.z);
    }

    /// Sends the `ChatMessage` packet containing the raw text component
    /// Position 0: chat (chat box)
    pub fn send_raw_chat(&self, message: TextComponent) {
        // 走统一的入口，Bedrock 后端会在这里被转成 `TextPacket`（R4 回执/聊天）。
        self.send_raw_system_message(message);
    }

    /// Sends a raw chat message to the player
    pub fn send_chat_message(&self, message: &[TextComponent]) {
        self.send_raw_chat(TextComponent {
            extra: message.to_vec(),
            ..Default::default()
        });
    }

    pub fn send_no_permission_message(&self) {
        self.send_error_message("You do not have permission to perform this action.");
    }

    /// Sends the player a light purple system message (`message` is not in json format)
    pub fn send_worldedit_message(&self, message: &str) {
        self.send_raw_system_message(
            TextComponentBuilder::new(message.to_string())
                .color_code(ColorCode::LightPurple)
                .finish(),
        );
    }

    pub fn worldedit_set_first_position(&mut self, pos: BlockPos) {
        self.send_worldedit_message(&format!(
            "First position set to ({}, {}, {})",
            pos.x, pos.y, pos.z
        ));
        self.first_position = Some(pos);
        self.worldedit_send_cui(&format!("p|0|{}|{}|{}|0", pos.x, pos.y, pos.z));
    }

    pub fn worldedit_set_second_position(&mut self, pos: BlockPos) {
        self.send_worldedit_message(&format!(
            "Second position set to ({}, {}, {})",
            pos.x, pos.y, pos.z
        ));
        self.second_position = Some(pos);
        self.worldedit_send_cui(&format!("p|1|{}|{}|{}|0", pos.x, pos.y, pos.z));
    }

    pub fn worldedit_send_cui(&self, message: &str) {
        let cui_plugin_message = CPlayPluginMessage {
            channel: String::from("worldedit:cui"),
            data: Vec::from(message.as_bytes()),
        }
        .encode();
        self.client.send_packet(&cui_plugin_message);
    }

    /// Sends the player the disconnect packet, it is still up to the player to end the network
    /// stream.
    ///
    /// 必须是 `&mut self`：Bedrock 后端断开时要动 `PlayerConn::alive`（`close_connection(&mut self)`）。
    pub fn kick(&mut self, reason: TextComponent) {
        // Bedrock 客户端收不到 Java 的 Disconnect 包：让它自己断开，否则会一直挂着。
        if self.client.is_bedrock() {
            self.client.close_connection();
            return;
        }
        let disconnect = CDisconnect { reason }.encode();
        self.client.send_packet(&disconnect);
    }

    pub fn update_player_abilities(&self) {
        let player_abilities = CPlayerAbilities {
            flags: 0x0D | ((self.flying as u8) << 1),
            fly_speed: 0.05 * self.fly_speed,
            fov_modifier: 0.1,
        }
        .encode();
        self.client.send_packet(&player_abilities);
        // Bedrock 侧也得跟着变：以前这里只发 Java 包，Bedrock 客户端永远停在
        // "开局那一份能力值"（BUG-032「能力值不跟随后端状态」）。
        // `fly_speed` 要传绝对值，和上面 Java 包里的算法保持一致。
        self.client.send_abilities(self.flying, 0.05 * self.fly_speed);
    }

    pub fn set_gamemode(&mut self, gamemode: Gamemode) {
        self.gamemode = gamemode;
        let change_game_state = CGameEvent {
            reason: CGameEventType::ChangeGamemode,
            value: self.gamemode.get_id() as f32,
        }
        .encode();
        self.client.send_packet(&change_game_state);
    }

    pub fn has_permission(&self, node: &str) -> bool {
        if let Some(cache) = &self.permissions_cache {
            if let Some(val) = cache.get_node_val(node) {
                val > 0
            } else {
                // Node is not in database
                false
            }
        } else {
            // Permissions is not enabled
            true
        }
    }

    pub fn open_container(&self, inventory: &[InventoryEntry], container_type: ContainerType) {
        let mut slots: Vec<Option<SlotData>> =
            (0..container_type.num_slots()).map(|_| None).collect();
        for entry in inventory {
            let item_stack = utils::inventory_entry_to_stack(entry);
            slots[entry.slot as usize] = Some(utils::encode_slot_data(&item_stack));
        }

        let open_window = COpenScreen {
            window_id: 1,
            window_type: container_type.window_type(),
            window_title: "Container".into(),
        }
        .encode();
        self.client.send_packet(&open_window);

        let window_items = CSetContainerContent {
            window_id: 1,
            state_id: 0,
            slot_data: slots,
            carried_item: None,
        }
        .encode();
        self.client.send_packet(&window_items);
    }

    pub fn set_inventory_slot(&mut self, slot: u32, item: Option<ItemStack>) {
        let set_slot = CSetContainerSlot {
            window_id: 0,
            state_id: 0,
            slot: slot as i16,
            slot_data: item.as_ref().map(utils::encode_slot_data),
        }
        .encode();
        self.client.send_packet(&set_slot);

        self.inventory[slot as usize] = item;
    }

    pub fn spawn_packet(&self) -> CSpawnEntity {
        CSpawnEntity {
            entity_id: self.entity_id as i32,
            entity_uuid: self.uuid,
            entity_type: protocol_id!("minecraft:entity_type", "minecraft:player"),
            pitch: self.pitch,
            yaw: self.yaw,
            // TODO: probably not the same
            head_yaw: self.yaw,
            data: 0, // unused
            x: self.pos.x,
            y: self.pos.y,
            z: self.pos.z,
            velocity_x: 0,
            velocity_y: 0,
            velocity_z: 0,
        }
    }

    pub fn metadata_packet(&self) -> CSetEntityMetadata {
        CSetEntityMetadata {
            entity_id: self.entity_id as i32,
            metadata: vec![
                CSetEntityMetadataEntry {
                    index: 0,
                    metadata_type: 0,
                    value: vec![{
                        let mut bitfield = 0;
                        if self.crouching {
                            bitfield |= 0x02;
                        };
                        if self.sprinting {
                            bitfield |= 0x08;
                        };
                        bitfield
                    }],
                },
                CSetEntityMetadataEntry {
                    index: 6,
                    metadata_type: 20,
                    value: vec![if self.crouching { 5 } else { 0 }],
                },
                CSetEntityMetadataEntry {
                    index: 17,
                    metadata_type: 0,
                    value: vec![self.skin_parts.bits() as u8],
                },
            ],
        }
    }

    pub fn equippment_packet(&self) -> Option<CSetEquipment> {
        self.inventory[self.selected_slot as usize + 36]
            .as_ref()
            .map(|item| {
                CSetEquipment {
                    entity_id: self.entity_id as i32,
                    equipment: vec![CSetEquipmentEquipment {
                        slot: 0, // Main hand
                        item: Some(utils::encode_slot_data(item)),
                    }],
                }
            })
    }
}

pub trait PacketSender {
    fn send_packet(&self, data: &PacketEncoder);

    /// 推一份 Bedrock 侧边栏（R3）。默认空实现：Java 后端走自己的计分包。
    fn send_bedrock_sidebar(&self, _sidebar: &mchprs_network::bedrock::bridge::BedrockSidebar) {}

    /// Sends the `ChatMessage` packet containing the raw json data.
    /// Position 1: system message (chat box)
    fn send_raw_system_message(&self, message: TextComponent) {
        let chat_message = CSystemChatMessage {
            content: message,
            overlay: false,
        }
        .encode();
        self.send_packet(&chat_message);
    }

    /// Sends the player a red system message (`message` is not in json format)
    fn send_error_message(&self, message: &str) {
        self.send_raw_system_message(
            TextComponentBuilder::new(message.to_string())
                .color_code(ColorCode::Red)
                .finish(),
        );
    }

    /// Sends the player a yellow system message (`message` is not in json format)
    fn send_system_message(&self, message: &str) {
        self.send_raw_system_message(
            TextComponentBuilder::new(message.to_string())
                .color_code(ColorCode::Yellow)
                .finish(),
        )
    }
}

impl PacketSender for PlayerPacketSender {
    fn send_packet(&self, data: &PacketEncoder) {
        self.send_packet(data);
    }
}

impl PacketSender for Player {
    fn send_packet(&self, data: &PacketEncoder) {
        self.client.send_packet(data);
    }

    /// Bedrock 玩家收不到 `CSystemChatMessage`：转成 `TextPacket`（RAW）推过去；
    /// Java 玩家仍走原来的 JSON 组件包。见 BACKLOG.md BUG-032 R4「回执/聊天」。
    fn send_raw_system_message(&self, message: TextComponent) {
        if self.client.is_bedrock() {
            self.client.send_chat(&message.to_legacy_text());
            return;
        }
        let chat_message = CSystemChatMessage {
            content: message,
            overlay: false,
        }
        .encode();
        self.send_packet(&chat_message);
    }

    /// Bedrock 玩家的右侧计分板走语义化推送；Java 玩家继续用 `CUpdateScore`。
    fn send_bedrock_sidebar(&self, sidebar: &mchprs_network::bedrock::bridge::BedrockSidebar) {
        self.client.send_sidebar(sidebar);
    }
}

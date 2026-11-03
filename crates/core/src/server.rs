use crate::automation::{AutomationAction, AutomationRequest, AutomationResponse, AutomationServer};
use crate::config::CONFIG;
use crate::player::{Gamemode, PacketSender, Player, PlayerPos};
use crate::plot::commands::DECLARE_COMMANDS;
use crate::plot::{self, database, Plot, PLOT_BLOCK_HEIGHT, PLOT_SCALE};
use crate::{permissions, utils};
use backtrace::Backtrace;
use bus::Bus;
use hmac::{Hmac, Mac};
use mchprs_network::packets::clientbound::{
    CConfigurationPluginMessage, CDisconnectLogin, CFinishConfiguration, CGameEvent,
    CGameEventType, CLogin, CLoginPluginRequest, CLoginSuccess, CPlayerInfoActions,
    CPlayerInfoAddPlayer, CPlayerInfoUpdate, CPlayerInfoUpdatePlayer, CPong, CRegistryBiome,
    CRegistryBiomeEffects, CRegistryData, CRegistryDataCodec, CRegistryDimensionType, CResponse,
    CSetCompression, CSetContainerContent, CSetHeldItem, CSynchronizePlayerPosition,
    ClientBoundPacket, UpdateTime,
};
use mchprs_network::packets::serverbound::{
    SAcknowledgeFinishConfiguration, SHandshake, SLoginAcknowledged, SLoginPluginResponse,
    SLoginStart, SPing, SRequest, ServerBoundPacketHandler, VelocityResponseData,
};
use mchprs_network::packets::{PacketEncoderExt, PlayerProperty, SlotData, COMPRESSION_THRESHOLD};
use mchprs_blocks::BlockPos;
use mchprs_network::{NetworkServer, NetworkState};
use mchprs_text::TextComponent;
use mchprs_utils::map;
use mchprs_world::{MC_VERSION, PROTOCOL_VERSION};
use rustc_hash::FxHashMap;
use serde_json::json;
use sha2::Sha256;
use std::fs;
use std::io::Cursor;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// `Message` gets send from a plot thread to the server thread.
#[derive(Debug)]
pub enum Message {
    /// This message is sent to the server thread when a player sends a chat message,
    /// It contains the uuid and name of the player and the raw message the player sent.
    ChatInfo(u128, String, String),
    /// This message is sent to the server thread when a player joins the server.
    PlayerJoined(Player),
    /// This message is sent to the server thread when a player leaves the server.
    PlayerLeft(u128),
    /// This message is sent to the server thread when a player goes outside of their plot.
    PlayerLeavePlot(Player),
    /// This message is sent to the server thread when a player runs /tp <name>.
    PlayerTeleportOther(Player, String),
    /// This message is sent to the server thread when a player changes their gamemode.
    PlayerUpdateGamemode(u128, Gamemode),
    /// This message is sent to the server thread when a plot unloads itself.
    PlotUnload(i32, i32),
    /// This message is sent to the server thread when a player runs /stop.
    Shutdown,
    /// This message is sent to the server thread when a plot thread answers
    /// an automation request. The u64 is the automation connection id.
    IoResponse(u64, AutomationResponse),
    /// This message is sent to the server thread when a plot samples a group
    /// that an automation connection bound with `POUT`. The server pushes the
    /// `OUTPUT <name> <tick> <hex>` line to that connection.
    IoDataPortOutput {
        conn_id: u64,
        name: String,
        tick: u32,
        hex: String,
    },
    /// The plot entered reservation mode (`/tick seq`) or left it. On entry the
    /// automation bridge drops every connection and lets the next one become
    /// the exclusive script; leaving only unlocks (BEDROCK.md).
    SeqModeChanged { on: bool },
    /// A line typed into the server terminal (BUG-031); it is forwarded to the
    /// plot thread, which runs it like an operator command.
    ConsoleCommand(String),
}

/// `BroadcastMessage` gets broadcasted from the server thread to all the plot threads.
/// This happens when there is a chat message, a player joins or leaves, or the server
/// shuts down.
#[derive(Debug, Clone)]
pub enum BroadcastMessage {
    /// This message is broadcasted for chat messages. It contains the uuid of the player and
    /// the raw json data to send to the clients.
    Chat(u128, Vec<TextComponent>),
    /// This message is broadcasted when a player joins the server. It is used to update
    /// the tab-list on all connected clients.
    PlayerJoinedInfo(PlayerJoinInfo),
    /// This message is broadcasted when a player leaves the server. It is used to update
    /// the tab-list on all connected clients.
    PlayerLeft(u128),
    /// This message is broadcasted when a player changes their gamemode,
    PlayerUpdateGamemode(u128, Gamemode),
    /// This message is broadcasted when the server is stopping, either through the stop
    /// command or through the ctrl+c handler.
    Shutdown,
}

/// `PrivMessage` gets send from the server thread directly to a plot thread.
/// This only happens when a player is getting transfered to a plot.
#[derive(Debug)]
pub enum PrivMessage {
    PlayerEnterPlot(Player),
    PlayerTeleportOther(Player, String),
    AutomationRequest {
        conn_id: u64,
        request: AutomationRequest,
    },
    /// An automation connection closed; the plot must drop its data ports.
    AutomationDisconnect { conn_id: u64 },
    /// A `/` command typed into the server terminal (BUG-031).
    ConsoleCommand(String),
    /// A Bedrock client just logged in and needs a read-only world snapshot
    /// (BEDROCK.md「最小只读通路」). The plot builds it on its own thread
    /// and answers through `reply`.
    ///
    /// `chunk_radius` 来自 `CONFIG.view_distance`：plot 按它决定快照覆盖多大，
    /// 必须和发给客户端的 `ChunkRadiusUpdated` 一致（BUG-032 第 16/26 条）。
    BedrockSnapshot {
        reply: Sender<mchprs_network::bedrock::bridge::PlotSnapshot>,
        chunk_radius: i32,
        /// 出生点（**MCHPRS/Java 坐标**）：有存档就用存档位置，否则 None（用默认出生点）。
        /// 登录时直接出生在这里，避免"先出生再传送"（BUG-032 第 31 条）。
        spawn: Option<(f64, f64, f64)>,
    },
}

/// This is the data that gets sent in the `PlayerJoinedInfo` broadcast message.
/// It contains imformation such as the player's username, uuid, and skin.
#[derive(Debug, Clone)]
pub struct PlayerJoinInfo {
    pub username: String,
    pub uuid: u128,
    pub gamemode: Gamemode,
    pub properties: Vec<PlayerProperty>,
}

#[derive(Debug, Clone)]
struct PlayerListEntry {
    plot_x: i32,
    plot_z: i32,
    username: String,
    properties: Vec<PlayerProperty>,
    gamemode: Gamemode,
}

struct PlotListEntry {
    plot_x: i32,
    plot_z: i32,
    priv_message_sender: mpsc::Sender<PrivMessage>,
}

/// This represents a minecraft server
pub struct MinecraftServer {
    network: NetworkServer,
    broadcaster: Bus<BroadcastMessage>,
    receiver: Receiver<Message>,
    plot_sender: Sender<Message>,
    online_players: FxHashMap<u128, PlayerListEntry>,
    running_plots: Vec<PlotListEntry>,
    automation: Option<AutomationServer>,
    /// Events coming from the Bedrock listener thread (BEDROCK.md).
    bedrock_events: Receiver<mchprs_network::bedrock::bridge::BedrockEvent>,
    /// Bedrock 玩家身份表：uuid → 会话句柄。
    ///
    /// 用途：拒绝"同名 Java + Bedrock 同时在线"（两者 UUID 相同，会互相打架），
    /// 以及在需要时（超时/重复登录）主动断开对应的 Bedrock 会话。
    bedrock_players: FxHashMap<u128, mchprs_network::bedrock::bridge::BedrockHandle>,
}

impl MinecraftServer {
    /// Start the server
    pub fn run() {
        std::panic::set_hook(Box::new(|panic_info| {
            let backtrace = Backtrace::new();
            error!("plot {}\n{:?}", panic_info, backtrace);
        }));

        info!("Starting server...");
        let start_time = Instant::now();

        // Create world folders if they don't exist yet
        fs::create_dir_all("./world/players").unwrap();
        fs::create_dir_all("./world/plots").unwrap();
        fs::create_dir_all("./schems").unwrap();
        // `memory/` with its hidden templates and an `io.txt` skeleton
        // (BEDROCK.md).
        crate::memory::ensure_dir_and_templates();
        crate::io::ensure_file();
        // The `betterio` flag is read by the uncompiled simulator too, so it has
        // to be set at startup, not only before a compile (BEDROCK.md).
        mchprs_redstone::set_betterio(CONFIG.betterio);

        plot::database::init();

        let bind_addr = CONFIG.bind_address.clone();

        // Create thread messaging structs
        let (plot_tx, server_rx) = mpsc::channel();
        let bus = Bus::new(100);
        let ctrl_handler_sender = plot_tx.clone();

        ctrlc::set_handler(move || {
            ctrl_handler_sender.send(Message::Shutdown).unwrap();
        })
        .expect("There was an error setting the ctrlc handler");

        // Terminal console (BUG-031): every line that starts with `/` is
        // forwarded to the plot thread as an operator command.
        let console_sender = plot_tx.clone();
        crate::console::spawn(move |line| {
            let _ = console_sender.send(Message::ConsoleCommand(line));
        });

        if let Some(permissions_config) = &CONFIG.luckperms {
            permissions::init(permissions_config.clone()).unwrap();
        }

        // Create server struct
        // Bedrock listener (BEDROCK.md): `bedrock = 0` disables it.
        let (bedrock_tx, bedrock_rx) = mpsc::channel();
        let mut server = MinecraftServer {
            network: NetworkServer::new(bind_addr),
            broadcaster: bus,
            receiver: server_rx,
            plot_sender: plot_tx,
            online_players: FxHashMap::default(),
            running_plots: Vec::new(),
            automation: match AutomationServer::bind(CONFIG.io_port) {
                Ok(server) => Some(server),
                Err(err) => {
                    warn!(
                        "Failed to bind the automation bridge on 127.0.0.1:{}: {}. \
                         The MCHPRS_IO automation protocol is DISABLED; the game \
                         server keeps running.",
                        CONFIG.io_port, err
                    );
                    None
                }
            },
            bedrock_events: bedrock_rx,
            bedrock_players: FxHashMap::default(),
        };

        // 基岩版监听：`bedrock = 0` 表示关闭（BEDROCK.md）。
        // 视距用 `CONFIG.view_distance`，与 Java 侧 `CLogin` 同源：Bedrock 的世界大小
        // 不能和 Java 玩家看到的不一样（第 16/26 条）。
        if CONFIG.bedrock > 0 {
            let chunk_radius = CONFIG.view_distance.clamp(1, 32) as i32;
            let bedrock_addr = bedrock_bind_addr(&CONFIG.bind_address, CONFIG.bedrock);
            mchprs_network::bedrock::session::spawn_mchprs_listener(
                bedrock_addr.clone(),
                bedrock_tx,
                chunk_radius,
            );
            debug!("Bedrock listener on udp {bedrock_addr} (view distance {chunk_radius} chunks)");
        }

        // Load the spawn area plot on server start
        // This plot should be always active
        let (spawn_tx, spawn_rx) = mpsc::channel();
        Plot::load_and_run(
            0,
            0,
            server.broadcaster.add_rx(),
            server.plot_sender.clone(),
            spawn_rx,
            true,
            None,
        );
        server.running_plots.push(PlotListEntry {
            plot_x: 0,
            plot_z: 0,
            priv_message_sender: spawn_tx,
        });

        info!("Done! Start took {:?}", start_time.elapsed());

        loop {
            server.update();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Updates the player's location on the `online_players` list
    fn update_player_entry(&mut self, uuid: u128, plot_x: i32, plot_z: i32) {
        let player = self.online_players.get_mut(&uuid);
        if let Some(player) = player {
            player.plot_x = plot_x;
            player.plot_z = plot_z;
        }
    }

    /// Removes the plot entry from the `running_plots` list
    fn handle_plot_unload(&mut self, plot_x: i32, plot_z: i32) {
        let index = self
            .running_plots
            .iter()
            .position(|p| p.plot_x == plot_x && p.plot_z == plot_z);
        if let Some(index) = index {
            self.running_plots.remove(index);
        }
    }

    fn graceful_shutdown(&mut self) {
        info!("Commencing graceful shutdown...");
        self.broadcaster.broadcast(BroadcastMessage::Shutdown);
        // Wait for all plots to save and unload
        while !self.running_plots.is_empty() {
            while let Ok(message) = self.receiver.try_recv() {
                if let Message::PlotUnload(plot_x, plot_z) = message {
                    self.handle_plot_unload(plot_x, plot_z);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }

        std::process::exit(0);
    }

    fn send_player_to_plot(&mut self, mut player: Player, new_entry: bool) {
        // Single-plot server (docs/file-formats.md): p0,0 is the only plot that
        // ever exists. A player whose saved position lies outside it is moved
        // to the plot center instead of causing another plot to be loaded.
        let (mut plot_x, mut plot_z) = player.pos.plot_pos();
        if plot_x != 0 || plot_z != 0 {
            let (center_x, center_z) = Plot::get_center(0, 0);
            player.teleport(PlayerPos::new(center_x, 64.0, center_z));
            plot_x = 0;
            plot_z = 0;
        }

        if new_entry {
            let player_list_entry = PlayerListEntry {
                plot_x,
                plot_z,
                username: player.username.clone(),
                properties: player.properties.clone(),
                gamemode: player.gamemode,
            };
            self.online_players.insert(player.uuid, player_list_entry);
        } else {
            self.update_player_entry(player.uuid, plot_x, plot_z);
        }

        // The spawn plot is loaded unconditionally at startup, so it must be
        // present. Never fall back to loading a plot here: that would defeat
        // the single-plot rule and resurrect plot loading for other plots.
        match self
            .running_plots
            .iter()
            .find(|p| p.plot_x == plot_x && p.plot_z == plot_z)
        {
            Some(plot_list_entry) => {
                let _ = plot_list_entry
                    .priv_message_sender
                    .send(PrivMessage::PlayerEnterPlot(player));
            }
            None => {
                warn!(
                    "Cannot place player {} on plot {},{}: that plot is not loaded",
                    player.username, plot_x, plot_z
                );
            }
        }
    }

    /// Routes an automation request to the plot containing `pos`.
    fn route_automation_request(
        &mut self,
        conn_id: u64,
        pos: BlockPos,
        request: AutomationRequest,
    ) {
        let plot_x = pos.x >> 4 >> PLOT_SCALE;
        let plot_z = pos.z >> 4 >> PLOT_SCALE;
        let plot_loaded = self
            .running_plots
            .iter()
            .any(|p| p.plot_x == plot_x && p.plot_z == plot_z);
        if plot_loaded {
            let plot_list_entry = self
                .running_plots
                .iter()
                .find(|p| p.plot_x == plot_x && p.plot_z == plot_z)
                .unwrap();
            let _ = plot_list_entry
                .priv_message_sender
                .send(PrivMessage::AutomationRequest { conn_id, request });
        } else if let Some(automation) = &mut self.automation {
            // The plot is not loaded, so no circuit can be running there.
            automation.handle_reply(conn_id, AutomationResponse::Inactive(pos));
        }
    }

    /// 把一个 Bedrock 客户端登记成**真正的玩家**并送进 spawn plot (0,0)。
    ///
    /// 复用 Java 玩家的加载路径（背包、权限缓存、存档格式完全一致），只把连接后端换成
    /// `PlayerBackend::Bedrock`。见 BEDROCK.md「玩家化」。
    fn handle_bedrock_register(
        &mut self,
        username: String,
        position: (f64, f64, f64),
        link: mchprs_network::bedrock::bridge::BedrockPlayerLink,
    ) {
        // 与同名 Java 离线玩家共用身份：权限、存档都一致（符合"与 Java 版一致"）。
        let uuid = Player::generate_offline_uuid(&username);

        // 同名 Java + Bedrock 同时在线会撞同一个 UUID，直接拒绝后连的这个。
        if self.online_players.contains_key(&uuid) || self.bedrock_players.contains_key(&uuid) {
            warn!(
                "rejected Bedrock player {username}: a player with the same name is already online (UUID {:032x})",
                uuid
            );
            link.handle.close_connection();
            return;
        }

        // 先确认 spawn plot 在跑，再登记句柄：否则句柄会永远留在身份表里，
        // 这个用户名以后再也连不上（复盘第 14 条）。p0,0 是常驻 plot，正常不会走到这里。
        if !self
            .running_plots
            .iter()
            .any(|plot| plot.plot_x == 0 && plot.plot_z == 0)
        {
            warn!("Bedrock player {username} wants to enter plot (0,0), but that plot is not running; disconnecting");
            link.handle.close_connection();
            return;
        }

        // 先把句柄登记进身份表（用于后续断开/拒绝重复登录），再把 link 交给 Player。
        self.bedrock_players.insert(uuid, link.handle.clone());

        let mut player = Player::load_player(
            uuid,
            username,
            Vec::new(),
            crate::player::PlayerBackend::Bedrock(link),
        );
        // 位置：**有存档就用存档里的**，只有第一次进服才用 Bedrock 会话给的出生点
        // （BUG-032 第 32 条）。以前这里无条件覆盖，于是每次进服都被扔到默认出生点
        // (128,128,128) 再从天上掉下来 —— 存档其实一直是好的。
        if Player::save_exists(uuid) {
            // `do_login` 已经按默认出生点把客户端传送过一次，这里纠正到存档位置。
            player.teleport(player.pos.clone());
            debug!(
                "Bedrock player {} entered at the saved position ({:.2}, {:.2}, {:.2}); saved state flying={} fly_speed={:.2} walk_speed={:.2} gamemode={:?} on_ground={}",
                player.username,
                player.pos.x,
                player.pos.y,
                player.pos.z,
                player.flying,
                player.fly_speed,
                player.walk_speed,
                player.gamemode,
                player.on_ground
            );
        } else {
            player.pos = PlayerPos {
                x: position.0,
                y: position.1,
                z: position.2,
            };
            debug!(
                "Bedrock player {} joined for the first time, using the default spawn point ({:.2}, {:.2}, {:.2})",
                player.username, player.pos.x, player.pos.y, player.pos.z
            );
        }

        // **走和 Java 完全相同的入服流程**（复查第 13 条）：`Message::PlayerJoined` 那一支
        // 会做 `database::ensure_user`、广播 `PlayerJoinedInfo`（Java 玩家 tab 里可见）、
        // 登记 `online_players`，然后 `send_player_to_plot` 把玩家送进 p0,0。
        // 之前手写这一段漏掉了上面三件事，导致 Bedrock 玩家对 Java 侧"不存在"。
        // 消息投进 `plot_sender` 后由下一轮 `update()` 的 `handle_message` 处理。
        if self
            .plot_sender
            .send(Message::PlayerJoined(player))
            .is_err()
        {
            // 走到这里说明服务端正在退出（接收端已 drop）：把身份表清掉，别留下永久失效的名字。
            warn!("Bedrock player could not deliver PlayerJoined (server channel closed); rolled back identity table");
            self.bedrock_players.remove(&uuid);
        }
    }

    fn handle_player_enter_play(&mut self, client_idx: usize) {
        let client = self.network.handshaking_clients.remove(client_idx);

        let uuid = client.uuid.unwrap();
        let username = client.username.clone().unwrap();
        let properties = client.properties.clone();

        // 同名 Bedrock 玩家已在线时，拒绝这次 Java 登录（同一个 UUID 不能同时在线）。
        if self.bedrock_players.contains_key(&uuid) {
            warn!("rejected Java player {username}: a Bedrock player with the same name is online");
            client.close_connection();
            return;
        }

        let player = Player::load_player(
            uuid,
            username,
            properties,
            crate::player::PlayerBackend::Java(client.into()),
        );

        let join_game = CLogin {
            entity_id: player.entity_id as i32,
            is_hardcore: false,
            dimension_names: vec!["minecraft:overworld".to_owned()],
            max_players: CONFIG.max_players as i32,
            view_distance: CONFIG.view_distance as i32,
            simulation_distance: CONFIG.view_distance as i32,
            reduced_debug_info: false,
            enable_respawn_screen: false,
            do_limited_crafting: false,
            dimension_type: "minecraft:overworld".to_owned(),
            dimension_name: "minecraft:overworld".to_owned(),
            hashed_seed: 0,
            gamemode: player.gamemode.get_id() as u8,
            previous_gamemode: 1,
            is_debug: false,
            is_flat: true,
            death_location: None,
            portal_cooldown: 0,
        }
        .encode();
        player.client.send_packet(&join_game);

        let entity_metadata = player.metadata_packet().encode();
        player.client.send_packet(&entity_metadata);

        // Send the player's position and rotation.
        let player_pos_and_look = CSynchronizePlayerPosition {
            x: player.pos.x,
            y: player.pos.y,
            z: player.pos.z,
            yaw: player.yaw,
            pitch: player.pitch,
            flags: 0,
            teleport_id: 0,
        }
        .encode();
        player.client.send_packet(&player_pos_and_look);

        // Send the player list to the newly connected player.
        // (This is the list you see when you press tab in-game)
        let mut add_player_list = Vec::new();
        for (&uuid, player) in &self.online_players {
            add_player_list.push(CPlayerInfoUpdatePlayer {
                uuid,
                actions: CPlayerInfoActions {
                    add_player: Some(CPlayerInfoAddPlayer {
                        name: player.username.clone(),
                        properties: player.properties.clone(),
                    }),
                    update_gamemode: Some(player.gamemode.get_id()),
                    update_listed: Some(true),
                    ..Default::default()
                },
            });
        }
        add_player_list.push(CPlayerInfoUpdatePlayer {
            uuid: player.uuid,
            actions: CPlayerInfoActions {
                add_player: Some(CPlayerInfoAddPlayer {
                    name: player.username.clone(),
                    properties: player.properties.clone(),
                }),
                update_gamemode: Some(player.gamemode.get_id()),
                update_listed: Some(true),
                ..Default::default()
            },
        });

        let player_info = CPlayerInfoUpdate {
            players: add_player_list,
        }
        .encode();
        player.client.send_packet(&player_info);

        // Send the player's inventory
        let slot_data: Vec<Option<SlotData>> = player
            .inventory
            .iter()
            .map(|op| op.as_ref().map(utils::encode_slot_data))
            .collect();
        let window_items = CSetContainerContent {
            window_id: 0,
            state_id: 0,
            slot_data,
            carried_item: None,
        }
        .encode();
        player.client.send_packet(&window_items);

        // Send the player's selected item slot
        let held_item_change = CSetHeldItem {
            slot: player.selected_slot as i8,
        }
        .encode();
        player.client.send_packet(&held_item_change);

        player.client.send_packet(&DECLARE_COMMANDS);

        let time_update = UpdateTime {
            world_age: 0,
            // Noon
            time_of_day: -6000,
        }
        .encode();
        player.client.send_packet(&time_update);

        player.update_player_abilities();

        let game_event = CGameEvent {
            reason: CGameEventType::WaitForChunks,
            value: 0.0,
        }
        .encode();
        player.client.send_packet(&game_event);

        self.plot_sender
            .send(Message::PlayerJoined(player))
            .unwrap();
    }

    fn handle_player_login_start(&mut self, client_idx: usize, login_start: SLoginStart) {
        let clients = &mut self.network.handshaking_clients;
        let username = login_start.name;
        clients[client_idx].username = Some(username.clone());

        if let Some(velocity_config) = &CONFIG.velocity
            && velocity_config.enabled
        {
            let message_id = rand::random();
            clients[client_idx].forwarding_message_id = Some(message_id);
            let plugin_message = CLoginPluginRequest {
                channel: "velocity:player_info".to_string(),
                message_id,
                data: vec![1], // MODERN_DEFAULT
            }
            .encode();
            clients[client_idx].send_packet(&plugin_message);
            return;
        }

        self.complete_player_login(client_idx);
    }

    fn complete_player_login(&mut self, client_idx: usize) {
        let clients = &mut self.network.handshaking_clients;
        let username = clients[client_idx].username.clone().unwrap();

        let set_compression = CSetCompression {
            threshold: COMPRESSION_THRESHOLD as i32,
        }
        .encode();
        clients[client_idx].send_packet(&set_compression);
        clients[client_idx].set_compressed(true);

        // Set generate uuid if it doesn't exist yet
        let uuid = match clients[client_idx].uuid {
            Some(uuid) => uuid,
            None => {
                let uuid = Player::generate_offline_uuid(&username);
                clients[client_idx].uuid = Some(uuid);
                uuid
            }
        };

        let login_success = CLoginSuccess {
            uuid,
            username,
            properties: clients[client_idx].properties.clone(),
        }
        .encode();
        clients[client_idx].send_packet(&login_success);
    }

    fn handle_message(&mut self, message: Message) {
        match message {
            Message::PlayerJoined(player) => {
                info!("{} joined the game", player.username);
                // Send player info to plots
                let player_join_info = PlayerJoinInfo {
                    username: player.username.clone(),
                    uuid: player.uuid,
                    gamemode: player.gamemode,
                    properties: player.properties.clone(),
                };
                database::ensure_user(&format!("{:032x}", player.uuid), &player.username);
                self.broadcaster
                    .broadcast(BroadcastMessage::PlayerJoinedInfo(player_join_info));
                self.send_player_to_plot(player, true);
            }
            Message::PlayerLeft(uuid) => {
                if let Some((_, player)) = self.online_players.remove_entry(&uuid) {
                    info!("{} left the game", player.username);
                }
                // Bedrock 玩家离开时把身份表里的句柄一起清掉，否则同名玩家再也连不上。
                self.bedrock_players.remove(&uuid);
                self.broadcaster
                    .broadcast(BroadcastMessage::PlayerLeft(uuid));
            }
            Message::PlotUnload(plot_x, plot_z) => self.handle_plot_unload(plot_x, plot_z),
            Message::ConsoleCommand(line) => {
                // Single-plot server: console commands always target p0,0.
                match self
                    .running_plots
                    .iter()
                    .find(|plot| plot.plot_x == 0 && plot.plot_z == 0)
                {
                    Some(plot) => {
                        let _ = plot
                            .priv_message_sender
                            .send(PrivMessage::ConsoleCommand(line));
                    }
                    None => crate::console::error("No plot is loaded (p0,0)."),
                }
            }
            Message::ChatInfo(uuid, username, message) => {
                info!("<{}> {}", username, message);
                self.broadcaster.broadcast(BroadcastMessage::Chat(
                    uuid,
                    TextComponent::from_legacy_text(
                        &CONFIG
                            .chat_format
                            .replace("{username}", &username)
                            .replace("{message}", &message),
                    ),
                ));
            }
            Message::PlayerLeavePlot(player) => {
                self.send_player_to_plot(player, false);
            }
            Message::Shutdown => {
                self.graceful_shutdown();
            }
            Message::PlayerTeleportOther(player, other_username) => {
                let username_lower = other_username.to_lowercase();
                if let Some((_, other_player)) = self
                    .online_players
                    .iter()
                    .find(|(_, p)| p.username.to_lowercase().starts_with(&username_lower))
                {
                    let plot_x = other_player.plot_x;
                    let plot_z = other_player.plot_z;

                    let plot_loaded = self
                        .running_plots
                        .iter()
                        .any(|p| p.plot_x == plot_x && p.plot_z == plot_z);
                    if !plot_loaded {
                        player
                            .send_system_message("Their plot wasn't loaded. How did this happen??");
                        self.send_player_to_plot(player, false);
                    } else {
                        self.update_player_entry(player.uuid, plot_x, plot_z);
                        let plot_list_entry = self
                            .running_plots
                            .iter()
                            .find(|p| p.plot_x == plot_x && p.plot_z == plot_z)
                            .unwrap();
                        let _ = plot_list_entry
                            .priv_message_sender
                            .send(PrivMessage::PlayerTeleportOther(player, other_username));
                    }
                } else {
                    player.send_system_message("Player not found!");
                    self.send_player_to_plot(player, false);
                }
            }
            Message::PlayerUpdateGamemode(uuid, gamemode) => {
                if let Some(player) = self.online_players.get_mut(&uuid) {
                    player.gamemode = gamemode;
                }
                self.broadcaster
                    .broadcast(BroadcastMessage::PlayerUpdateGamemode(uuid, gamemode));
            }
            Message::IoResponse(conn_id, response) => {
                if let Some(automation) = &mut self.automation {
                    automation.handle_reply(conn_id, response);
                }
            }
            Message::IoDataPortOutput {
                conn_id,
                name,
                tick,
                hex,
            } => {
                if let Some(automation) = &mut self.automation {
                    automation.push_output(conn_id, &name, tick, &hex);
                }
            }
            Message::SeqModeChanged { on } => {
                if let Some(automation) = &mut self.automation {
                    automation.set_seq_mode(on);
                }
            }
        }
    }

    fn update(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            self.handle_message(message);
        }

        // Bedrock listener 的请求：转给常驻的 spawn plot（0,0）去打包只读快照。
        // 见 BEDROCK.md「最小只读通路」。
        while let Ok(event) = self.bedrock_events.try_recv() {
            match event {
                mchprs_network::bedrock::bridge::BedrockEvent::SnapshotRequest {
                    reply,
                    chunk_radius,
                    username,
                } => {
                    let spawn = {
                        let uuid = Player::generate_offline_uuid(&username);
                        Player::saved_position(uuid).map(|p| (p.x, p.y, p.z))
                    };
                    match self
                        .running_plots
                        .iter()
                        .find(|plot| plot.plot_x == 0 && plot.plot_z == 0)
                    {
                        Some(entry) => {
                            let _ = entry
                                .priv_message_sender
                                .send(PrivMessage::BedrockSnapshot {
                                    reply,
                                    chunk_radius,
                                    spawn,
                                });
                        }
                        None => {
                            warn!("Bedrock client requested a world snapshot, but spawn plot (0,0) is not running");
                        }
                    }
                }
                mchprs_network::bedrock::bridge::BedrockEvent::RegisterPlayer {
                    username,
                    position,
                    link,
                } => self.handle_bedrock_register(username, position, link),
            }
        }

        self.network.update();

        // Poll the automation bridge and route requests to their plots.
        let automation_actions = match &mut self.automation {
            Some(automation) => automation.poll(),
            None => Vec::new(),
        };
        for action in automation_actions {
            match action {
                AutomationAction::Route {
                    conn_id,
                    pos,
                    request,
                } => self.route_automation_request(conn_id, pos, request),
                AutomationAction::Disconnect {
                    conn_id,
                    plot_x,
                    plot_z,
                } => {
                    if let Some(plot_list_entry) = self
                        .running_plots
                        .iter()
                        .find(|p| p.plot_x == plot_x && p.plot_z == plot_z)
                    {
                        let _ = plot_list_entry
                            .priv_message_sender
                            .send(PrivMessage::AutomationDisconnect { conn_id });
                    }
                }
            }
        }
        if let Some(automation) = &mut self.automation {
            automation.flush();
        }

        let mut client_idx = 0;
        let mut clients_len = self.network.handshaking_clients.len();
        loop {
            if client_idx >= clients_len {
                break;
            }

            let packets = self.network.handshaking_clients[client_idx].receive_packets();
            for packet in packets {
                packet.handle(self, client_idx);
            }

            let new_len = self.network.handshaking_clients.len();

            if clients_len == new_len {
                client_idx += 1;
            }
            clients_len = new_len;
        }
    }
}

impl ServerBoundPacketHandler for MinecraftServer {
    fn handle_handshake(&mut self, handshake: SHandshake, client_idx: usize) {
        let clients = &mut self.network.handshaking_clients;
        let client = &mut clients[client_idx];
        let next_state = match handshake.next_state {
            1 => NetworkState::Status,
            2 => NetworkState::Login,
            // TODO: Handle invalid next state
            _ => return,
        };
        if next_state == NetworkState::Login && handshake.protocol_version != PROTOCOL_VERSION {
            warn!("A player tried to connect using the wrong version");
            let disconnect = CDisconnectLogin {
                reason: json!({ "text": format!("Version mismatch, I'm on {}!", MC_VERSION) })
                    .to_string(),
            }
            .encode();
            client.send_packet(&disconnect);
            client.close_connection();
        }
    }

    fn handle_request(&mut self, _request: SRequest, client_idk: usize) {
        let client = &mut self.network.handshaking_clients[client_idk];
        let response = CResponse {
            json_response: json!({
                "version": {
                    "name": MC_VERSION,
                    "protocol": PROTOCOL_VERSION
                },
                "players": {
                    "max": CONFIG.max_players,
                    "online": self.online_players.len(),
                    "sample": []
                },
                "description": {
                    "text": CONFIG.motd
                }
            })
            .to_string(),
        }
        .encode();
        client.send_packet(&response);
    }

    fn handle_ping(&mut self, ping: SPing, client_idx: usize) {
        let client = &mut self.network.handshaking_clients[client_idx];
        let pong = CPong {
            payload: ping.payload,
        }
        .encode();
        client.send_packet(&pong);
    }

    fn handle_login_start(&mut self, login_start: SLoginStart, client_idx: usize) {
        self.handle_player_login_start(client_idx, login_start);
    }

    fn handle_login_acknowledged(
        &mut self,
        _login_acknowledged: SLoginAcknowledged,
        client_idx: usize,
    ) {
        // The client has been switched to the Configuration state
        let client = &mut self.network.handshaking_clients[client_idx];

        // Sends the custom brand name to the player
        // (This can be seen in the f3 debug menu in-game)
        let brand = CConfigurationPluginMessage {
            channel: String::from("minecraft:brand"),
            data: {
                let mut data = Vec::new();
                data.write_string(32767, "Minecraft High Performance Redstone");
                data
            },
        }
        .encode();
        client.send_packet(&brand);

        let dimension = CRegistryDimensionType {
            fixed_time: Some(6000),
            has_skylight: true,
            has_ceiling: false,
            ultrawarm: false,
            natural: true,
            coordinate_scale: 1.0,
            bed_works: false,
            respawn_anchor_works: false,
            min_y: 0,
            height: PLOT_BLOCK_HEIGHT,
            logical_height: PLOT_BLOCK_HEIGHT,
            infiniburn: "#minecraft:infiniburn_overworld".to_owned(),
            effects: "#minecraft:overworld".to_owned(),
            ambient_light: 1.0,
            piglin_safe: false,
            has_raids: false,
            monster_spawn_block_light_limit: 0,
            monster_spawn_light_level: 0,
        };

        let codec = CRegistryDataCodec {
            dimension_types: map! {
                "minecraft:overworld" => dimension.clone()
            },
            biomes: map! {
                "mchprs:plot" => CRegistryBiome {
                    has_precipitation: false,
                    temperature: 0.5,
                    downfall: 0.5,
                    effects: CRegistryBiomeEffects {
                        sky_color: 0x7BA4FF,
                        water_fog_color: 0x050533,
                        fog_color: 0xC0D8FF,
                        water_color: 0x3F76E4,
                    },
                },
                // Apparently the client NEEDS this to exist (MC-267103)
                "minecraft:plains" => CRegistryBiome {
                    has_precipitation: false,
                    temperature: 0.8,
                    downfall: 0.4,
                    effects: CRegistryBiomeEffects {
                        sky_color: 7907327,
                        water_fog_color: 329011,
                        fog_color: 12638463,
                        water_color: 4159204,
                    },
                }
            },
        };
        let registry_data = CRegistryData {
            registry_codec: codec,
        };
        client.send_packet(&registry_data.encode());

        client.send_packet(&CFinishConfiguration.encode());
    }

    fn handle_acknowledge_finish_configuration(
        &mut self,
        _ackowledge_finish_configuration: SAcknowledgeFinishConfiguration,
        client_idx: usize,
    ) {
        self.handle_player_enter_play(client_idx);
    }

    fn handle_login_plugin_response(&mut self, packet: SLoginPluginResponse, client_idx: usize) {
        let clients = &mut self.network.handshaking_clients;

        if Some(packet.message_id) != clients[client_idx].forwarding_message_id {
            error!(
                "Unknown login plugin response with message id: {}",
                packet.message_id
            );
            return;
        }

        if !packet.successful {
            error!("Velocity forwarding channel not understood by client");
            return;
        }

        let velocity_response = VelocityResponseData::decode(&mut Cursor::new(&packet.data));

        let velocity_response = match velocity_response {
            Ok(velocity_response) => velocity_response,
            Err(err) => {
                error!("Could not decode velocity reponse data: {:?}", err);
                return;
            }
        };

        let secret = CONFIG.velocity.as_ref().unwrap().secret.as_bytes();
        let mut mac = <Hmac<Sha256>>::new_from_slice(secret).unwrap();
        mac.update(&packet.data[32..]);
        if mac.verify_slice(&packet.data[..32]).is_err() {
            error!("Failed to verify velocity secret!");
            return;
        };

        clients[client_idx].uuid = Some(velocity_response.uuid);
        clients[client_idx].properties = velocity_response.properties;
        self.complete_player_login(client_idx);
    }
}

pub fn get_version_string() -> String {
    format!(
        "{} v{}\n{}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("GIT_HASH")
    )
}

/// 基岩监听地址（BUG-032 第 54 条，方案 B）。
///
/// IP 取自 Java 的 `bind_address`，端口仍用 `CONFIG.bedrock`：用户把 `bind_address`
/// 配成 `127.0.0.1:25565` 想让服务只开本机时，基岩 UDP 也不再对整个局域网开放。
/// `bind_address` 里拿不到 IP（没有 `:` 或 IP 为空）时回落到 `0.0.0.0`，与旧行为一致。
fn bedrock_bind_addr(bind_address: &str, port: i64) -> String {
    let host = match bind_address.rsplit_once(':') {
        Some((host, _)) if !host.is_empty() => host,
        _ => "0.0.0.0",
    };
    if host.contains(':') {
        // IPv6 字面量要加方括号，`[::1]:19132`
        format!("[{}]:{}", host.trim_start_matches('[').trim_end_matches(']'), port)
    } else {
        format!("{host}:{port}")
    }
}

#[cfg(test)]
mod bedrock_bind_tests {
    use super::bedrock_bind_addr;

    #[test]
    fn ipv4_takes_ip_from_bind_address() {
        assert_eq!(bedrock_bind_addr("0.0.0.0:25565", 19132), "0.0.0.0:19132");
        assert_eq!(bedrock_bind_addr("127.0.0.1:25565", 19132), "127.0.0.1:19132");
        assert_eq!(
            bedrock_bind_addr("192.168.0.103:25565", 19132),
            "192.168.0.103:19132"
        );
    }

    #[test]
    fn ipv6_is_bracketed() {
        assert_eq!(bedrock_bind_addr("[::1]:25565", 19132), "[::1]:19132");
        assert_eq!(
            bedrock_bind_addr("[2001:db8::1]:25565", 19132),
            "[2001:db8::1]:19132"
        );
        // 不带方括号的写法也能救回来
        assert_eq!(bedrock_bind_addr("::1:25565", 19132), "[::1]:19132");
    }

    #[test]
    fn missing_ip_falls_back_to_wildcard() {
        assert_eq!(bedrock_bind_addr("25565", 19132), "0.0.0.0:19132");
        assert_eq!(bedrock_bind_addr(":25565", 19132), "0.0.0.0:19132");
    }
}

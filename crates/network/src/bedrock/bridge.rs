//! Bedrock 端点与 MCHPRS 服务端之间的**最小只读桥**。
//!
//! 线程拓扑（见 `BACKLOG.md` BUG-032「最小只读通路方案」）：
//!
//! ```text
//! Bedrock 监听线程  --BedrockEvent-->  服务端线程  --PrivMessage-->  plot 线程
//! Bedrock 监听线程  <--PlotSnapshot--  服务端线程  <--回传通道------  plot 线程
//! ```
//!
//! 这一版**只传世界快照**：不下发方块更新、不接收方块修改、不执行命令。
//! 后续的方块更新广播 / 交互回灌会在这条桥上加消息，但不会改它的方向语义。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use tokio::sync::mpsc as tokio_mpsc;

/// 服务端线程 / plot 线程发给某个 Bedrock 会话的指令。
///
/// 出站方向**按语义传**（不传 Java 包，也不传 Bedrock 包）：`crates/core` 不依赖
/// `bedrock` 协议 crate，所以这里只放"想表达什么"，具体包由会话线程自己编码。
/// 见 BACKLOG.md BUG-032「入站转义 / 出站语义化」。
#[derive(Debug)]
pub enum BedrockCommand {
    /// 断开这个客户端（客户端会收到 Java 侧的 kick 文案，下一版再补 Bedrock Disconnect 包）。
    Disconnect,
    /// 更新右侧计分板（Java 的 sidebar 等价物，R3）。
    Sidebar(BedrockSidebar),
    /// 往聊天框推一行文本（命令回执、系统提示、玩家聊天）。`text` 可带 `§` 颜色码。
    Chat { text: String },
    /// 一批方块变化（BUG-032 第 3 条）：`(x, y, z, java_state)`，
    /// `(x, y, z)` 是 **MCHPRS（Java）坐标**，`java_state` 是 MCHPRS 的方块状态 id ——
    /// 两者都由会话线程换算成 Bedrock 坐标（`y - 64`）与运行时 id。
    ///
    /// 用"一批"而不是"一条"：红石 tick 一次能改上千个方块，逐条发会把指令通道刷爆。
    BlockUpdates(Vec<(i32, i32, i32, u32)>),
    /// 能力值（飞行状态 + 飞行速度）：Bedrock 的 `UpdateAbilities`。
    ///
    /// `fly_speed` 是**绝对值**（Java 侧 `0.05 * 倍率`），不是倍率 —— 换算在 core 做完。
    Abilities { flying: bool, fly_speed: f32 },
    /// 服务端强制定位（BUG-032 第 8 条）。
    ///
    /// `(x, y, z)` 是 **MCHPRS（Java）坐标**，会话线程自己减 64 换回 Bedrock 坐标。
    /// 对应 Java 的 `CSynchronizePlayerPosition`：`Player::teleport()` 现在两种后端都走这里。
    Teleport { x: f64, y: f64, z: f64 },
    /// 线上其它玩家的完整快照（BUG-032 第 22 条完整版）。
    ///
    /// core 每 tick 推一份"现在有谁、在哪"的全量列表（**不含自己**），会话线程自己做 diff：
    /// 新来的发 PlayerList+AddPlayer，走了的发 RemoveActor+PlayerList，动了的发 MovePlayer。
    ///
    /// 用全量而不是增量：玩家进出、传送、坠落纠正都要改状态，
    /// 全量只多占几个字节，却省掉一套容易出错的增量协议。
    Players(Vec<BedrockPlayerInfo>),
    /// 增量区块流送（BUG-032 第 26 条 + 第 19 条）。
    ///
    /// plot 线程按每 tick 的预算分批生成、分批下发，不再登录时一次性生成 289 个区块
    /// （那样要卡 tick 200~260ms）。`publisher` 只在"玩家中心区块变了"或
    /// "已发方形变大"时带一个，避免客户端把没数据的区域当成已发布（第 27 条）。
    Chunks(BedrockChunkBatch),
    /// 动态补全：替换一个**软枚举**的全部取值（BUG-032 B4）。
    ///
    /// Bedrock 客户端不会发 `SCommandSuggestionsRequest`，所以候选值变化只能靠
    /// `UpdateSoftEnumPacket(REPLACE)` 推给客户端。`name` 必须和命令树里
    /// `Param::SoftEnum(name)` 用的一致（`schem_files` / `coord_files` /
    /// `coord_types`）。
    SoftEnum { name: String, values: Vec<String> },
    /// 某个玩家**挥了一下手臂**（BUG-032 A3 挥手动画）。
    ///
    /// 挥手是一次性事件、不是状态，所以不能塞进每 tick 的玩家快照；
    /// 会话线程按 uuid 找到那个人的 runtime id，发 `Animate(Swing)`。
    Swing { uuid: u128 },
    /// 游戏模式变化（BUG-032 A5）：Bedrock 的 `SetPlayerGameType`。
    GameMode(BedrockGameMode),
    /// 告示牌文字（BUG-032 A2）：Bedrock 的 `BlockActorData`。
    ///
    /// 坐标是 **MCHPRS（Java）坐标**，会话线程减 64 换算；
    /// `front` / `back` 是**已经降级成 legacy 文本**（`§` 码）的四行，
    /// JSON 文本组件到 legacy 的降级在 core 做（网络层不依赖文本组件）。
    Sign(BedrockSign),
    /// 让客户端弹出告示牌编辑框（BUG-032 第 43 条）：Bedrock 的 `OpenSign`。
    ///
    /// 出处：Geyser `JavaOpenSignEditorTranslator`（Java `ClientboundOpenSignEditorPacket`
    /// → Bedrock `OpenSignPacket{position, frontSide}`）。
    /// **没有这个包客户端永远不会弹编辑框** —— 它不会自己开，真机现象就是"根本不能写牌子"。
    ///
    /// `(x, y, z)` 是 **MCHPRS（Java）坐标**，会话线程自己 `- 64`。
    OpenSign { x: i32, y: i32, z: i32, front: bool },
    /// 把存档里的背包发给客户端（BUG-036）：进 plot 时一次，退出重进后物品就回来了。
    Inventory(BedrockInventorySnapshot),
}

/// 一份要发回给 Bedrock 客户端的背包（**MCHPRS/Java 物品 id**，由会话线程换成 Bedrock 物品 id）。
#[derive(Clone, Debug, Default)]
pub struct BedrockInventorySnapshot {
    /// 主背包 36 格，**Bedrock 顺序**：`0..=8` 是快捷栏、`9..=35` 是 27 格背包。
    pub main: Vec<Option<(u32, u8)>>,
    /// 护甲 4 格，Bedrock 顺序：头盔、胸甲、护腿、靴子。
    pub armor: Vec<Option<(u32, u8)>>,
    /// 副手。
    pub offhand: Option<(u32, u8)>,
    /// 当前快捷栏选中槽（0..=8）。
    pub selected_slot: u8,
}

/// Bedrock 的游戏模式（MCHPRS 只有这两种）。数值见 `GameType` 枚举：
/// Creative = 1、Spectator = 6（`Survival=0`、`Adventure=2`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BedrockGameMode {
    Creative,
    Spectator,
}

/// 一块告示牌的四行文字（**已经降级成 legacy 文本**）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BedrockSign {
    /// MCHPRS（Java）坐标。
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// 正面四行（行内不换行；Bedrock 用 `\n` 拼接）。
    pub front: [String; 4],
    /// 反面四行。
    pub back: [String; 4],
}

/// 一批要下发给某个 Bedrock 客户端的区块。
#[derive(Debug)]
pub struct BedrockChunkBatch {
    /// `(chunk_x, chunk_z, LevelChunk 负载, sub_chunk_count)`，`chunk_x/z` 是 Bedrock 区块坐标。
    pub chunks: Vec<(i32, i32, Vec<u8>, u32)>,
    /// `Some((方块 x, 方块 y, 方块 z, 半径方块))` = 先发 `NetworkChunkPublisherUpdate`。
    pub publisher: Option<(i32, i32, i32, u32)>,
}

/// 一个要在 Bedrock 客户端里显示的玩家（**MCHPRS/Java 坐标**，会话线程负责 `y - 64`）。
#[derive(Clone, Debug)]
pub struct BedrockPlayerInfo {
    pub uuid: u128,
    pub username: String,
    /// MCHPRS 坐标（不是 Bedrock 坐标）。
    pub position: (f64, f64, f64),
    /// **Java 顺序** `(yaw, pitch)`；换算成 Bedrock 的 `(pitch, yaw)` 在 `entity` 模块里做。
    pub rotation: (f32, f32),
    pub on_ground: bool,
    /// 主手物品：`(MCHPRS 物品 id, 数量)`，`None` = 空手。
    ///
    /// 网络层用它查 `item_map` 换成 Bedrock 的物品网络 id（A1 `MobEquipment`）。
    /// 查不到 = MCHPRS 认识但 Bedrock 表里没有的怪物品 → 当空手发（绝不瞎编 id）。
    pub held_item: Option<(u32, u8)>,
    /// 潜行 / 疾跑（A3：决定 `FLAGS` 元数据里的 SNEAKING / SPRINTING 位）。
    pub sneaking: bool,
    pub sprinting: bool,
}

impl BedrockPlayerInfo {
    /// 只带"看得见"的最小字段（测试与旧调用点用）。
    pub fn new(
        uuid: u128,
        username: impl Into<String>,
        position: (f64, f64, f64),
        rotation: (f32, f32),
        on_ground: bool,
    ) -> Self {
        Self {
            uuid,
            username: username.into(),
            position,
            rotation,
            on_ground,
            held_item: None,
            sneaking: false,
            sprinting: false,
        }
    }
}


/// 计分板一行：`text` 是**已经带 `§` 颜色码**的显示文本，`score` 决定排序（越大越靠上）。
#[derive(Clone, Debug)]
pub struct BedrockSidebarLine {
    /// Java 侧的 fake-player 名（如 `redpiler_state`、`flag_o`），Bedrock 用不上，
    /// 保留它是为了同一行在多次更新间可追溯、便于日志。
    pub name: String,
    pub score: i32,
    /// 显示文本（`§` legacy 码，见 `TextComponent::to_legacy_text`）。
    pub text: String,
}

/// 整个 sidebar 的当前状态（每次全量下发，会话线程自己做 diff）。
#[derive(Clone, Debug)]
pub struct BedrockSidebar {
    /// 标题（`§` legacy 码）。
    pub title: String,
    pub lines: Vec<BedrockSidebarLine>,
}

/// 从服务端侧操作一个 Bedrock 会话的句柄，可跨线程克隆。
///
/// 有了它，Bedrock 客户端就能作为 `PlayerBackend::Bedrock` 进入 MCHPRS 的玩家体系，
/// 复用位置、权限、命令、计分板等已有逻辑（见 BACKLOG.md BUG-032「玩家化」）。
///
/// 指令通道用 **tokio 的 unbounded** 而不是 `std::sync::mpsc`：会话线程要把它和
/// `conn.recv()` 一起 `tokio::select!`，而 `std::sync::mpsc::Receiver::recv()` 是阻塞调用、
/// 不是 future。`UnboundedSender::send` 仍然是同步的，所以 core（同步 tick 循环）这边不受影响。
#[derive(Clone)]
pub struct BedrockHandle {
    commands: tokio_mpsc::UnboundedSender<BedrockCommand>,
    alive: Arc<AtomicBool>,
}

impl BedrockHandle {
    pub fn new(commands: tokio_mpsc::UnboundedSender<BedrockCommand>, alive: Arc<AtomicBool>) -> Self {
        Self { commands, alive }
    }

    /// 会话是否还活着。会话线程断开时会把标记置为 `false`。
    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    /// 给这个会话投递一条指令（目前只有断开）。
    pub fn send(&self, command: BedrockCommand) {
        let _ = self.commands.send(command);
    }

    /// 与 Java 的 `PlayerConn::close_connection` 语义对齐。
    pub fn close_connection(&self) {
        self.alive.store(false, Ordering::Relaxed);
        let _ = self.commands.send(BedrockCommand::Disconnect);
    }

    /// 投递一批方块变化（BUG-032 第 3 条）。空批次直接丢掉，不发无意义的消息。
    pub fn send_block_updates(&self, changes: &[(i32, i32, i32, u32)]) {
        if changes.is_empty() {
            return;
        }
        let _ = self
            .commands
            .send(BedrockCommand::BlockUpdates(changes.to_vec()));
    }

    /// 投递能力值（飞行状态 + 飞行速度）变化。
    pub fn send_abilities(&self, flying: bool, fly_speed: f32) {
        let _ = self
            .commands
            .send(BedrockCommand::Abilities { flying, fly_speed });
    }

    /// 投递服务端强制定位（MCHPRS 坐标）。
    pub fn send_teleport(&self, x: f64, y: f64, z: f64) {
        let _ = self.commands.send(BedrockCommand::Teleport { x, y, z });
    }

    /// 投递"线上其它玩家"的最新快照（BUG-032 第 22 条完整版）。
    ///
    /// 空列表也要投递：它表达的是"现在别人都不在了"，会话线程据此销毁残留实体。
    pub fn send_players(&self, players: Vec<BedrockPlayerInfo>) {
        let _ = self.commands.send(BedrockCommand::Players(players));
    }

    /// 投递一批区块（BUG-032 第 26 条增量区块 + 第 19 条）。
    pub fn send_chunks(&self, batch: BedrockChunkBatch) {
        if batch.chunks.is_empty() && batch.publisher.is_none() {
            return;
        }
        let _ = self.commands.send(BedrockCommand::Chunks(batch));
    }

    /// 投递软枚举取值（BUG-032 B4 动态补全）。
    pub fn send_soft_enum(&self, name: &str, values: Vec<String>) {
        let _ = self.commands.send(BedrockCommand::SoftEnum {
            name: name.to_string(),
            values,
        });
    }

    /// 通知"某个玩家挥了手"（BUG-032 A3）。
    pub fn send_swing(&self, uuid: u128) {
        let _ = self.commands.send(BedrockCommand::Swing { uuid });
    }

    /// 投递游戏模式变化（BUG-032 A5）。
    pub fn send_game_mode(&self, mode: BedrockGameMode) {
        let _ = self.commands.send(BedrockCommand::GameMode(mode));
    }

    /// 投递一块告示牌的文字（BUG-032 A2）。
    pub fn send_sign(&self, sign: BedrockSign) {
        let _ = self.commands.send(BedrockCommand::Sign(sign));
    }

    /// 让客户端弹出告示牌编辑框（BUG-032 第 43 条）。`(x, y, z)` 是 MCHPRS 坐标。
    pub fn send_open_sign(&self, x: i32, y: i32, z: i32, front: bool) {
        let _ = self
            .commands
            .send(BedrockCommand::OpenSign { x, y, z, front });
    }

    /// 投递一份背包（BUG-036）。
    pub fn send_inventory(&self, snapshot: BedrockInventorySnapshot) {
        let _ = self.commands.send(BedrockCommand::Inventory(snapshot));
    }
}

/// 一个 Bedrock 玩家的双向链接。
///
/// - `handle`：出站方向，服务端/plot 用来对这个会话下指令（断开、以后的计分板/聊天）。
/// - `packets`：入站方向，Bedrock 会话把客户端输入**转义成 Java 的 `ServerBoundPacket`**
///   塞进来；`plot` 的 `handle_packets_for_player` 会像处理 Java 客户端一样处理它们，
///   于是命令、方块交互、移动等逻辑**完全复用**，不需要为 Bedrock 再写一套。
///
/// 见 BACKLOG.md BUG-032「玩家化 · 入站转义」。
pub struct BedrockPlayerLink {
    pub handle: BedrockHandle,
    pub packets: mpsc::Receiver<Box<dyn crate::packets::serverbound::ServerBoundPacket>>,
    /// 背包状态同步（见 [`BedrockInventoryUpdate`]）。和 `packets` 分开是**顺序要求**：
    /// plot 每 tick 先排空这一条、再处理 `packets`，这样"手里是什么"一定先于
    /// "我要放这个方块"到达 `handle_use_item_impl`。
    pub inventory: mpsc::Receiver<BedrockInventoryUpdate>,
}

/// 会话线程 → plot 的**背包状态同步**（BUG-032 R5：拿物品 / 放方块）。
///
/// Bedrock 的背包在创造模式下是客户端权威（我们在 `StartGame` 里发的是
/// `enable_item_stack_net_manager = false`），所以服务端不维护完整背包；
/// 只在**需要读它的时候**（放方块、右键交互）把"手里是什么"同步过去 ——
/// `Plot::handle_use_item_impl` 是从 `Player::inventory` 取手里物品的。
#[derive(Debug)]
pub enum BedrockInventoryUpdate {
    /// 快捷栏选中的槽位（0..=8）。
    SelectedSlot(u32),
    /// 写入一个背包槽位。`slot` 是 **Java 背包槽位号**（快捷栏是 36..=44）；
    /// `item` 是 `(MCHPRS 物品 id, 数量)`，`None` 表示空手。
    Slot { slot: u32, item: Option<(u32, u8)> },
    /// 客户端在某个位置放了东西，但**服务端不认识那个物品**，要求把该位置的
    /// 真实方块发回去（BUG-032 第 35 条）。
    ///
    /// 背景：Bedrock 的创造面板给的是**全量原版物品**（R1），大部分 MCHPRS 没有。
    /// 客户端是乐观预测——它先在本地把方块放上去，服务端不回话就永远留着。
    /// Java 侧由 `Plot::send_block_corrections`（`CBlockUpdate`）纠正；Bedrock 必须
    /// 显式补一条 `UpdateBlock`。坐标是 **MCHPRS（Java）坐标**。
    Correction { x: i32, y: i32, z: i32 },
}

/// plot 线程打包给 Bedrock 客户端的一份只读世界快照。
pub struct PlotSnapshot {
    /// Bedrock 世界的出生点（**已经做过 y-64 换算**）。
    pub spawn: (f32, f32, f32),
    /// `(chunk_x, chunk_z, LevelChunk 负载)`。
    /// 负载由 [`crate::bedrock::chunk::build_chunk_from_world`] 生成，`chunk_x/z` 是 Bedrock 区块坐标。
    /// 这是**登录时的那一批**（半径 [`crate::bedrock::chunk::INITIAL_RADIUS_CHUNKS`]），
    /// 其余区块由 `BedrockCommand::Chunks` 每 tick 增量流送。
    pub chunks: Vec<(i32, i32, Vec<u8>)>,
    /// 这批区块覆盖的半径（区块数）。会话线程据此算登录时 `NetworkChunkPublisherUpdate`
    /// 的半径，必须**只覆盖真的发了数据的范围**（BUG-032 第 27 条）。
    pub chunk_radius: i32,
}

/// Bedrock 监听线程发给服务端线程的事件。
pub enum BedrockEvent {
    /// 有新客户端登录，请给一份世界快照。
    ///
    /// `chunk_radius` 是**以区块为单位的半径**，由 core 从 `CONFIG.view_distance` 取来：
    /// 网络层不该读 core 的配置，所以值从这条消息传进来，plot 按它决定快照覆盖多大。
    /// 半径必须与实际下发的区块数一致，否则远处一片虚空（BUG-032 第 16/26 条）。
    SnapshotRequest {
        reply: mpsc::Sender<PlotSnapshot>,
        chunk_radius: i32,
        /// 登录身份。用来在**登录前**就决定出生点：有存档就按存档位置生成，
        /// 不再"先出生在默认点、再传送一百多格"（那个二次传送会让客户端的
        /// 相机/视场角状态错乱，见 BUG-032 第 31 条）。
        username: String,
    },
    /// 一个 Bedrock 客户端已经进入世界，请把它登记成**真正的玩家**
    /// （BACKLOG.md BUG-032「玩家化」）。服务端线程会用 `handle` 构造
    /// `PlayerBackend::Bedrock`，并把它送进 spawn plot。
    ///
    /// 这里只带 `username`：UUID 由**服务端**用
    /// `Player::generate_offline_uuid` 推导，这样与同名的 Java 离线玩家是同一个身份
    /// （权限、存档都一致，符合"与 Java 版一致"的要求）。
    RegisterPlayer {
        username: String,
        /// 出生点，**MCHPRS（Java）坐标** —— 已经做过 `y + 64` 换算，可以直接赋给 `Player::pos`。
        /// 换算在 `session` 侧只做一次，桥这里不重复换算（见 BUG-032 复查第 12 条）。
        position: (f64, f64, f64),
        link: BedrockPlayerLink,
    },
}

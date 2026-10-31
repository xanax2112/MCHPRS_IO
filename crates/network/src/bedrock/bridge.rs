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
}

/// plot 线程打包给 Bedrock 客户端的一份只读世界快照。
pub struct PlotSnapshot {
    /// Bedrock 世界的出生点（**已经做过 y-64 换算**）。
    pub spawn: (f32, f32, f32),
    /// `(chunk_x, chunk_z, LevelChunk 负载)`。
    /// 负载由 [`crate::bedrock::chunk::build_chunk_from_world`] 生成，`chunk_x/z` 是 Bedrock 区块坐标。
    pub chunks: Vec<(i32, i32, Vec<u8>)>,
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

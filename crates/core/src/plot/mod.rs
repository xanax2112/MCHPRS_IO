pub mod commands;
mod coord;
mod data;
pub mod database;
mod monitor;
mod packet_handlers;
mod scoreboard;
pub mod worldedit;

use crate::automation::{
    AutomationRequest, AutomationResponse, RequestKind, decode_hex, encode_hex,
};
use crate::config::{CONFIG, IoMode};
use crate::interaction;
use crate::interaction::UseOnBlockContext;
use crate::io::{self, IO_FILE};
use crate::memory::{self, MemoryState};
use crate::player::{EntityId, Gamemode, PacketSender, Player, PlayerPos};
use crate::server::{BroadcastMessage, Message, PrivMessage};
use crate::utils::HyphenatedUUID;
use anyhow::Error;
use bus::BusReader;
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::blocks::Block;
use mchprs_blocks::items::Item;
use mchprs_blocks::{BlockFace, BlockPos};
use mchprs_network::packets::clientbound::*;
use mchprs_network::packets::serverbound::SUseItemOn;
use mchprs_network::PlayerPacketSender;
use mchprs_redpiler::{CompileError, Compiler, CompilerOptions, TaskMonitor};
use mchprs_redstone::comparator::OverrideError;
use mchprs_save_data::plot_data::{ChunkData, PlotData, Tps, WorldSendRate};
use mchprs_text::TextComponent;
use mchprs_world::storage::Chunk;
use mchprs_world::{TickEntry, TickPriority, World};
use monitor::TimingsMonitor;
use rustc_hash::{FxHashMap, FxHashSet};
use scoreboard::RedpilerState;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Write;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tracing::{debug, error, info, warn};

use self::data::sleep_time_for_tps;
use self::scoreboard::Scoreboard;

/// The width of a plot (2^n)
pub const PLOT_SCALE: u32 = 5;

/// The width of a plot counted in chunks
pub const PLOT_WIDTH: i32 = 2i32.pow(PLOT_SCALE);
/// The plot width in blocks
pub const PLOT_BLOCK_WIDTH: i32 = PLOT_WIDTH * 16;
pub const NUM_CHUNKS: usize = PLOT_WIDTH.pow(2) as usize;

/// The height of the world in sections (Default: 24, Max: 127)
pub const PLOT_SECTIONS: usize = 24;
/// The plot height in blocks
pub const PLOT_BLOCK_HEIGHT: i32 = PLOT_SECTIONS as i32 * 16;

const ERROR_IO_ONLY: &str = "This plot cannot be interacted with while redpiler is active with `--io-only`. To stop redpiler, run `/redpiler reset`.";

const MAX_BATCH_SIZE: u32 = 50_000;

/// Maximum number of pending `INPUT` reservations. The queue is global because
/// reservation mode only ever admits one script (BACKLOG.md §3.3 / BUG-004).
const MAX_RESERVATIONS: usize = 255;

pub struct Plot {
    pub world: PlotWorld,
    pub players: Vec<Player>,
    pub redpiler: Compiler,

    /// File-backed read-only memories (BACKLOG.md BUG-015), loaded from
    /// `memory/*.rom.txt` and re-validated after every successful compile.
    memories: Vec<MemoryState>,

    // Thread communication
    message_receiver: BusReader<BroadcastMessage>,
    message_sender: Sender<Message>,
    priv_message_receiver: Receiver<PrivMessage>,

    locked_players: HashSet<EntityId>,

    // Timings
    tps: Tps,
    /// rtps to restore on `/tick unfreeze` (BACKLOG.md BUG-023).
    frozen_rtps: Option<Tps>,
    /// `/coord` context, undo and redo stacks (BACKLOG.md BUG-022).
    coord: coord::CoordState,
    /// Set when `/coord` touched a file, so the next compile re-reads it.
    coord_dirty: bool,
    last_update_time: Instant,
    lag_time: Duration,
    last_nspt: Option<Duration>,
    timings: TimingsMonitor,
    /// The last time a player was in this plot
    last_player_time: Instant,
    /// The last time the world changes were sent to the player
    last_world_send_time: Instant,
    /// The duration we should sleep for after every update
    sleep_time: Duration,
    /// When this is false, the update loop will end and the thread will stop.
    /// This will be set to false if no players are on the plot for a certain amount of time.
    running: bool,
    /// If true, the plot will remain running even if no players are on for a long time.
    always_running: bool,

    owner: Option<u128>,
    async_rt: Runtime,
    scoreboard: Scoreboard,

    /// Every group declared in `io.txt`, re-validated after each compile.
    io: io::IoTable,
    /// Completed tick counter of the running circuit: `0` the instant a compile
    /// finishes and `+1` per completed tick, wrapping at 32 bits
    /// (BACKLOG.md §7.2).
    tick: u32,
    /// Groups each automation connection is bound to with `POUT`. Any compile
    /// or reset empties this; scripts have to send `POUT` again
    /// (BACKLOG.md §7.3).
    pout_bindings: FxHashMap<u64, FxHashSet<String>>,
    /// Pending `INPUT` reservations, locked to absolute ticks. At most
    /// [`MAX_RESERVATIONS`] entries; a later request for the same tick and
    /// group overwrites the earlier one (BACKLOG.md §7.2).
    reservations: Vec<Reservation>,
    /// `/tick seq` is active: the plot is in reservation mode and the single
    /// exclusive script may reserve ticks (BACKLOG.md BUG-004).
    seq_mode: bool,
    /// The connection currently holding the reservation lock, mirrored from the
    /// automation bridge. Only that connection may use the
    /// `INPUT <name> <offset> <hex>` form (BACKLOG.md BUG-004).
    exclusive: Option<u64>,

    /// Bedrock 区块流送状态（BUG-032 第 26 条 + 第 19 条），key = 玩家 uuid。
    ///
    /// 以前是"登录时一次性生成 289 个区块"：实测 205~260ms 全卡在 plot 线程上，
    /// 而且走远就彻底没数据。现在改成每 tick 按预算推一圈。
    bedrock_streams: HashMap<u128, BedrockChunkStream>,
    /// 已生成的 Bedrock 区块负载缓存（key = Bedrock `(chunk_x, chunk_z)`）。
    ///
    /// 实测每个区块约 790 字节（空 plot），整张地皮 32×32 也就 ~800KB，
    /// 所以不做淘汰，全量常驻。
    bedrock_chunk_cache: HashMap<(i32, i32), Vec<u8>>,
    /// 本 tick 变过方块、需要重建缓存的区块（Bedrock 区块坐标）。
    bedrock_chunk_dirty: HashSet<(i32, i32)>,
}

/// 一个 Bedrock 玩家的区块流送进度（BUG-032 第 26/19 条）。
struct BedrockChunkStream {
    /// 上一次的**中心区块**（Bedrock 区块坐标）。变了才重算窗口。
    center: (i32, i32),
    /// 这个客户端当前"手里有"的区块。
    sent: HashSet<(i32, i32)>,
    /// 待发队列（由近到远）。
    pending: VecDeque<(i32, i32)>,
    /// 已经向客户端声明过的 publisher 半径（区块数）。
    ///
    /// **恒等于视距半径**：客户端会把 publisher 范围之外的 `LevelChunk` 直接丢掉，
    /// 所以 publisher 必须在**第一次登录时就报满**（Geyser/原版也是先 publisher 再区块），
    /// 绝不能用"随发随长"的策略 —— 那样先发的区块会被丢，之后也不会补发（真机踩过）。
    published_radius: i32,
    /// 这个窗口是什么时候开始铺的（用于打"铺满用了多久"的日志）。
    started: Instant,
}

/// Bedrock 的 `NetworkChunkPublisherUpdate` 半径（方块）由**区块半径**换算：
/// 方形被半径 `r` 的圆完全覆盖需要 `r × 16 × √2`，向下取整（第 27 条：不能报大）。
fn radius_blocks(chunk_radius: i32) -> u32 {
    (chunk_radius as f64 * 16.0 * std::f64::consts::SQRT_2).floor() as u32
}

/// One pending `INPUT`, locked to an absolute tick (BACKLOG.md §7.2).
#[derive(Clone)]
struct Reservation {
    /// Absolute tick counter value the input is applied at.
    tick: u32,
    group: String,
    bits: Vec<bool>,
}

/// Formats the empty-container compile warning (BACKLOG.md BUG-014): one red
/// message listing every affected comparator position, at most ten of them.
fn format_empty_container_warning(positions: &[BlockPos]) -> String {
    const MAX_LISTED: usize = 10;
    let mut message = String::from("Redpiler compile warning: comparator at ");
    for (index, pos) in positions.iter().take(MAX_LISTED).enumerate() {
        if index > 0 {
            message.push(',');
        }
        message.push_str(&format!("({},{},{})", pos.x, pos.y, pos.z));
    }
    if positions.len() > MAX_LISTED {
        message.push_str(",(...)");
    }
    message.push_str(" reads some empty containers");
    message
}

pub struct PlotWorld {
    pub x: i32,
    pub z: i32,
    pub chunks: Vec<Chunk>,
    pub to_be_ticked: Vec<TickEntry>,
    pub packet_senders: Vec<PlayerPacketSender>,
    world_send_rate: WorldSendRate,
    pending_block_entities: FxHashMap<BlockPos, CBlockEntityData>,
    pending_sounds: FxHashMap<BlockPos, CSoundEffect>,
    /// 本 tick 真正变过的方块：`flush_block_changes` 之后由 [`Plot::publish_world`]
    /// 取走，转成 Bedrock 的 `UpdateBlock`（BUG-032 第 3 条）。
    /// Java 那条路是 `CMultiBlockChange` 批量包，Bedrock 没有对应批量包，只能一条一条发。
    block_changes: Vec<(BlockPos, u32)>,
    /// 本 tick 变过文字的告示牌位置（BUG-032 A2）：`Plot` 没有玩家列表，
    /// 所以这里只登记，由 [`Plot::publish_world`] 换成 `BlockActorData` 下发。
    pub pending_bedrock_signs: Vec<BlockPos>,
}

impl PlotWorld {
    fn get_chunk_index_for_chunk(&self, chunk_x: i32, chunk_z: i32) -> usize {
        let local_x = chunk_x - self.x * PLOT_WIDTH;
        let local_z = chunk_z - self.z * PLOT_WIDTH;
        (local_x * PLOT_WIDTH + local_z).unsigned_abs() as usize
    }

    fn get_chunk_index_for_block(&self, block_x: i32, block_z: i32) -> Option<usize> {
        let chunk_x = (block_x - (self.x * PLOT_BLOCK_WIDTH)) >> 4;
        let chunk_z = (block_z - (self.z * PLOT_BLOCK_WIDTH)) >> 4;
        if !(0..PLOT_WIDTH).contains(&chunk_x) || !(0..PLOT_WIDTH).contains(&chunk_z) {
            return None;
        }
        Some(((chunk_x << PLOT_SCALE) + chunk_z).unsigned_abs() as usize)
    }

    pub fn get_corners(&self) -> (BlockPos, BlockPos) {
        const W: i32 = PLOT_BLOCK_WIDTH;
        let first_pos = BlockPos::new(self.x * W, 0, self.z * W);
        let second_pos = BlockPos::new(
            (self.x + 1) * W - 1,
            PLOT_BLOCK_HEIGHT - 1,
            (self.z + 1) * W - 1,
        );
        (first_pos, second_pos)
    }
}

impl World for PlotWorld {
    /// Sets a block in storage. Returns true if a block was changed.
    fn set_block_raw(&mut self, pos: BlockPos, block: u32) -> bool {
        let chunk_index = match self.get_chunk_index_for_block(pos.x, pos.z) {
            Some(idx) => idx,
            None => return false,
        };

        // Check to see if block is within height limit
        if pos.y >= PLOT_BLOCK_HEIGHT || pos.y < 0 {
            return false;
        }

        let chunk = &mut self.chunks[chunk_index];
        let changed = chunk.set_block(
            (pos.x & 0xF) as u32,
            pos.y as u32,
            (pos.z & 0xF) as u32,
            block,
        );
        if changed {
            self.block_changes.push((pos, block));
        }
        changed
    }

    /// Returns the block state id of the block at `pos`
    fn get_block_raw(&self, pos: BlockPos) -> u32 {
        let chunk_index = match self.get_chunk_index_for_block(pos.x, pos.z) {
            Some(idx) => idx,
            None => return 0,
        };
        let chunk = &self.chunks[chunk_index];
        chunk.get_block((pos.x & 0xF) as u32, pos.y as u32, (pos.z & 0xF) as u32)
    }

    fn delete_block_entity(&mut self, pos: BlockPos) {
        self.pending_block_entities.remove(&pos);
        let chunk_index = match self.get_chunk_index_for_block(pos.x, pos.z) {
            Some(idx) => idx,
            None => return,
        };
        let chunk = &mut self.chunks[chunk_index];
        chunk.delete_block_entity(BlockPos::new(pos.x & 0xF, pos.y, pos.z & 0xF));
    }

    fn get_block_entity(&self, pos: BlockPos) -> Option<&BlockEntity> {
        let chunk_index = self.get_chunk_index_for_block(pos.x, pos.z)?;
        let chunk = &self.chunks[chunk_index];
        chunk.get_block_entity(BlockPos::new(pos.x & 0xF, pos.y, pos.z & 0xF))
    }

    fn set_block_entity(&mut self, pos: BlockPos, block_entity: BlockEntity) {
        let chunk_index = match self.get_chunk_index_for_block(pos.x, pos.z) {
            Some(idx) => idx,
            None => return,
        };
        if let Some(nbt) = block_entity.to_nbt(true) {
            let block_entity_data = CBlockEntityData {
                x: pos.x,
                y: pos.y,
                z: pos.z,
                // For now the only nbt we send to the client is sign data
                ty: block_entity.ty(),
                nbt: nbt.content,
            };
            self.pending_block_entities.insert(pos, block_entity_data);
        } else {
            self.pending_block_entities.remove(&pos);
        }
        let chunk = &mut self.chunks[chunk_index];
        chunk.set_block_entity(BlockPos::new(pos.x & 0xF, pos.y, pos.z & 0xF), block_entity);
    }

    fn get_chunk(&self, x: i32, z: i32) -> Option<&Chunk> {
        self.chunks.get(self.get_chunk_index_for_chunk(x, z))
    }

    fn get_chunk_mut(&mut self, x: i32, z: i32) -> Option<&mut Chunk> {
        let chunk_idx = self.get_chunk_index_for_chunk(x, z);
        self.chunks.get_mut(chunk_idx)
    }

    fn schedule_tick(&mut self, pos: BlockPos, delay: u32, priority: TickPriority) {
        self.to_be_ticked.push(TickEntry {
            pos,
            ticks_left: delay,
            tick_priority: priority,
        });
    }

    fn pending_tick_at(&mut self, pos: BlockPos) -> bool {
        self.to_be_ticked.iter().any(|e| e.pos == pos)
    }

    fn play_sound(
        &mut self,
        pos: BlockPos,
        sound_id: i32,
        sound_category: i32,
        volume: f32,
        pitch: f32,
    ) {
        if self.world_send_rate.0 == 0.0 {
            return;
        }
        // FIXME: We do not know the players location here, so we send the sound packet to all
        // players A notchian server would only send to players in hearing distance
        // (volume.clamp(0.0, 1.0) * 16.0)
        let sound_effect_data = CSoundEffect {
            sound_id: sound_id + 1,
            sound_name: None,
            has_fixed_range: None,
            range: None,
            sound_category,
            x: pos.x * 8 + 4,
            y: pos.y * 8 + 4,
            z: pos.z * 8 + 4,
            volume,
            pitch,
            // FIXME: How do we decide this?
            seed: 0,
        };
        self.pending_sounds.insert(pos, sound_effect_data);
    }

    fn flush_block_changes(&mut self) {
        for packet in self.chunks.iter_mut().flat_map(|c| c.multi_blocks()) {
            let encoded = packet.encode();
            for player in &self.packet_senders {
                player.send_packet(&encoded);
            }
        }
        for chunk in &mut self.chunks {
            chunk.reset_multi_blocks();
        }
        // 方块实体：Java 走 `CBlockEntityData` 广播；Bedrock 另走 `BlockActorData`（A2）。
        // 先收完再遍历 —— `drain()` 借着 `pending_block_entities`，循环里不能再 `&mut self`。
        let pending: Vec<(BlockPos, CBlockEntityData)> =
            self.pending_block_entities.drain().collect();
        for (pos, block_entity) in pending {
            let encoded = block_entity.encode();
            for player in &self.packet_senders {
                player.send_packet(&encoded);
            }
            // 告示牌要另外通知 Bedrock（本层没有玩家列表，只登记位置，
            // 由 `Plot::publish_world` 换成 `BlockActorData` 发出去）。
            let is_sign = matches!(self.get_block_entity(pos), Some(BlockEntity::Sign(_)));
            if is_sign {
                self.pending_bedrock_signs.push(pos);
            }
        }
        for (_, sound) in self.pending_sounds.drain() {
            let encoded = sound.encode();
            for player in &self.packet_senders {
                player.send_packet(&encoded);
            }
        }
    }
}

/// Result of `/memory reload` (BACKLOG.md §4.4 / BUG-030).
pub(super) enum ReloadOutcome {
    /// Refused: the running circuit was compiled with `-i` (io only).
    CompiledIoOnly,
    /// Files were re-read; no compile was started.
    Reloaded {
        loaded: usize,
        rejected: Vec<String>,
    },
}

impl Plot {
    fn tickn(&mut self, ticks: u64) {
        if self.redpiler.is_active() {
            self.timings.tickn(ticks);
            self.redpiler.tickn(ticks);
            return;
        }

        for _ in 0..ticks {
            self.tick();
        }
    }

    /// Advances `ticks` ticks on behalf of `/radv` (`/radvance`).
    ///
    /// ROM / RAM see **every** tick, exactly like the automatic loop, and the
    /// tick counter advances once per completed tick so an `OUTPUT` sampled
    /// during `/radv 5` carries ticks `n+1 ..= n+5` (BACKLOG.md §7.2).
    fn advance_ticks(&mut self, ticks: u64) {
        if !self.redpiler.is_active() {
            self.tickn(ticks);
            self.publish_world();
            return;
        }
        let exact = CONFIG.io_mode() == IoMode::Exact;
        for _ in 0..ticks {
            self.step_tick(exact);
        }
        self.redpiler.flush(&mut self.world);
        // `EVENT` pushes were removed in 0.5.2-alpha; drain the backend's
        // buffer so it cannot grow without bound.
        self.redpiler.take_io_events();
        if !exact {
            self.sample_io_groups();
        }
        // 同 `publish_world`：`world.flush_block_changes()` 是空实现，真正发包的是
        // `Plot::flush_block_changes` + `send_bedrock_block_changes`（BUG-032 第 34 条）。
        self.world.flush_block_changes();
        self.send_bedrock_block_changes();
        self.last_world_send_time = Instant::now();
    }

    /// Runs exactly one redstone tick and advances the tick counter.
    ///
    /// The order is: apply every reservation that belongs to this tick, run the
    /// tick, bump the counter, then let the memories see the result. In `exact`
    /// mode the IO groups are sampled right away, so an `OUTPUT` line describes
    /// the state at the end of the tick it names (BACKLOG.md §7.2 / §7.4).
    fn step_tick(&mut self, exact: bool) {
        let next = self.tick.wrapping_add(1);
        self.apply_reservations(next);
        self.tickn(1);
        self.tick = next;
        self.run_memories();
        if exact {
            self.sample_io_groups();
        }
    }

    /// Writes every `INPUT` reserved for `tick` into its group.
    ///
    /// Reservations that never became due stay in the queue, so a `/radv` that
    /// stops short of the reserved tick simply leaves them pending
    /// (BACKLOG.md §7.2).
    fn apply_reservations(&mut self, tick: u32) {
        if self.reservations.is_empty() {
            return;
        }
        let mut due: Vec<Reservation> = Vec::new();
        self.reservations.retain(|reservation| {
            if reservation.tick == tick {
                due.push(reservation.clone());
                false
            } else {
                true
            }
        });
        for reservation in due {
            if let Some(group) = self.io.find_mut(&reservation.group) {
                group.write(&mut self.redpiler, &reservation.bits);
            }
        }
    }

    /// `true` while the plot is in manual stepping mode (`/rtps 0`).
    fn rtps_zero(&self) -> bool {
        matches!(self.tps, Tps::Limited(tps) if tps <= 0.0)
    }

    /// Samples every group that has a `POUT` binding and pushes the groups that
    /// fired to the connections bound to them.
    ///
    /// A group is polled **once** per tick, so several connections bound to it
    /// all receive the same value and the change detection is not disturbed by
    /// one of them being registered later. Ungrouped output groups are never
    /// polled.
    fn sample_io_groups(&mut self) {
        if self.pout_bindings.is_empty() || !self.redpiler.is_active() {
            return;
        }
        let mut bound: Vec<String> = Vec::new();
        for groups in self.pout_bindings.values() {
            for name in groups {
                if !bound.contains(name) {
                    bound.push(name.clone());
                }
            }
        }
        let mut fired: Vec<(String, String)> = Vec::new();
        for name in bound {
            let Some(group) = self.io.find_mut(&name) else {
                continue;
            };
            if let Some(bits) = group.poll(&self.redpiler) {
                let hex = encode_hex(&bits, group.matrix);
                fired.push((name, hex));
            }
        }
        for (name, hex) in fired {
            for (conn_id, groups) in &self.pout_bindings {
                if groups.contains(&name) {
                    let _ = self.message_sender.send(Message::IoDataPortOutput {
                        conn_id: *conn_id,
                        name: name.clone(),
                        tick: self.tick,
                        hex: hex.clone(),
                    });
                }
            }
        }
    }

    fn tick(&mut self) {
        self.timings.tick();
        if self.redpiler.is_active() {
            self.redpiler.tick();
            return;
        }

        self.world
            .to_be_ticked
            .sort_by_key(|e| (e.ticks_left, e.tick_priority));
        for pending in &mut self.world.to_be_ticked {
            pending.ticks_left = pending.ticks_left.saturating_sub(1);
        }
        while self.world.to_be_ticked.first().map_or(1, |e| e.ticks_left) == 0 {
            let entry = self.world.to_be_ticked.remove(0);
            mchprs_redstone::tick(self.world.get_block(entry.pos), &mut self.world, entry.pos);
        }
    }

    pub fn send_block_corrections(&mut self, positions: &[BlockPos]) {
        self.publish_world();
        for pos in positions {
            let state = self.world.get_block_raw(*pos);
            for player in &self.players {
                // 两种后端都走这一个入口：Java 发 CBlockUpdate，Bedrock 交给会话线程
                // 转成 UpdateBlock（BUG-032 第 3 条 —— 以前 Bedrock 玩家根本收不到）。
                player.client.send_block_update(*pos, state);
            }
        }
    }

    pub fn broadcast_chat_message(&mut self, message: String) {
        let broadcast_message = Message::ChatInfo(
            0,
            format!("Plot {}-{}", self.world.x, self.world.z),
            message,
        );
        self.message_sender.send(broadcast_message).unwrap();
    }

    pub fn broadcast_plot_chat_message(&mut self, message: &str) {
        for player in &mut self.players {
            player.send_chat_message(&TextComponent::from_legacy_text(message));
        }
    }

    fn change_player_gamemode(&mut self, player_idx: usize, gamemode: Gamemode) {
        self.players[player_idx].set_gamemode(gamemode);
        let _ = self.message_sender.send(Message::PlayerUpdateGamemode(
            self.players[player_idx].uuid,
            gamemode,
        ));
    }

    fn on_player_move(&mut self, player_idx: usize, old: PlayerPos, new: PlayerPos) {
        let old_block = old.block_pos();
        let new_block = new.block_pos();

        if let Some(true) = self.world.get_block(old_block).get_pressure_plate_powered()
            && !self.are_players_on_block(old_block)
        {
            self.set_pressure_plate(old_block, false);
        }

        if let Some(false) = self.world.get_block(new_block).get_pressure_plate_powered()
            && self.players[player_idx].on_ground
        {
            self.set_pressure_plate(new_block, true);
        }
    }

    fn set_pressure_plate(&mut self, pos: BlockPos, new_powered: bool) {
        if self.redpiler.is_active() {
            self.redpiler.set_pressure_plate(pos, new_powered);
            self.publish_world();
            return;
        }

        let mut block = self.world.get_block(pos);
        if let Some(powered) = block.get_pressure_plate_powered() {
            *powered = new_powered;
            self.world.set_block(pos, block);
            mchprs_redstone::update_surrounding_blocks(&mut self.world, pos);
            mchprs_redstone::update_surrounding_blocks(
                &mut self.world,
                pos.offset(BlockFace::Bottom),
            );
        } else {
            warn!("Block at {} is not a pressure plate", pos);
        }
        self.publish_world();
    }

    fn are_players_on_block(&mut self, pos: BlockPos) -> bool {
        for player in &self.players {
            if player.pos.block_pos() == pos && player.on_ground {
                return true;
            }
        }
        false
    }

    fn enter_plot(&mut self, player: Player) {
        self.save();
        if self.players.is_empty() {
            self.world.pending_sounds.clear();
        }
        let spawn_player = player.spawn_packet().encode();
        let metadata = player.metadata_packet().encode();
        let entity_equipment = player.equippment_packet();
        for other_player in &mut self.players {
            other_player.client.send_packet(&spawn_player);
            other_player.client.send_packet(&metadata);
            if let Some(entity_equipment) = &entity_equipment {
                other_player.client.send_packet(&entity_equipment.encode());
            }

            let spawn_other_player = other_player.spawn_packet().encode();
            player.client.send_packet(&spawn_other_player);

            if let Some(other_entity_equipment) = other_player.equippment_packet() {
                player.client.send_packet(&other_entity_equipment.encode());
            }

            let other_metadata = other_player.metadata_packet().encode();
            player.client.send_packet(&other_metadata);
        }

        player.send_system_message(&format!(
            "Entering plot ({}, {})",
            self.world.x, self.world.z
        ));
        // Java 的方块广播只对 Java 连接有意义；Bedrock 玩家的后端是句柄，
        // 它的方块更新要走 Bedrock 的推送通道（后续接入，见 BACKLOG.md BUG-032）。
        if let Some(java_client) = player.client.as_java() {
            self.world
                .packet_senders
                .push(PlayerPacketSender::new(java_client));
        }
        self.scoreboard.add_player(&player);
        // 动态补全（BUG-032 B4）：Bedrock 的软枚举初始值必须等玩家进 plot 才知道
        // （值来自本 plot 的 io.txt / memory 目录 / schems 目录），所以在这里推。
        // 注意：此时 `player` 还没进 `self.players`，所以不能用 `refresh_bedrock_soft_enums`
        // （那个按玩家下标取），只能就地推一次。
        if player.client.is_bedrock() {
            let (schem_files, coord_files, coord_types) = self.bedrock_soft_enum_values(&player);
            player.client.send_soft_enum("schem_files", schem_files);
            player.client.send_soft_enum("coord_files", coord_files);
            player.client.send_soft_enum("coord_types", coord_types);
            // A5：登录时 `StartGame` 声明的是创造模式，存档里如果是旁观者，
            // 客户端会一直按创造模式走。进 plot 时按真实模式补一条 `SetPlayerGameType`。
            player.client.send_game_mode(player.gamemode);
            // BUG-032 第 39 条：把存档里的背包灌回客户端，否则退出重进后物品栏是空的
            // （服务端一直有，只是从来没发过）。
            player.client.send_inventory(bedrock_inventory_snapshot(&player));
        }
        self.players.push(player);
        self.update_view_pos_for_player(self.players.len() - 1, true);
    }

    /// Bedrock 软枚举（动态补全）的取值（BUG-032 B4）。
    ///
    /// 四份列表：
    /// - `schem_files`：`//load` 的文件名（`schems` 目录，`schemati` 开启时按玩家 uuid 分目录）
    /// - `coord_files`：`/coord in` 的第一个参数（`io.txt` + `memory/*.txt`）
    /// - `coord_types`：`/coord new|add <type>` 的类型码
    ///
    /// **没有 `coord_targets`**：`/coord in <file> <name>` 的取值取决于前一个参数
    /// （`io.txt` 时是组名、`memory/*.txt` 时是类型码），Bedrock 软枚举没法按上下文缩小，
    /// 给并集会列出错误候选（2026-10-31 真机反馈），所以那一位保持普通字符串。
    ///
    /// **平台差异**：Bedrock 的软枚举是"每个参数一份平铺列表"，没法像 Java 那样按前文
    /// 上下文缩小范围，所以 `coord_targets` 用了并集（客户端会多列几个候选，不影响正确性）。
    fn bedrock_soft_enum_values(
        &self,
        player: &Player,
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        let mut schem_files = Vec::new();
        let mut dir = std::path::PathBuf::from("./schems");
        if CONFIG.schemati {
            dir.push(crate::utils::HyphenatedUUID(player.uuid).to_string());
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    schem_files.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
        schem_files.sort();

        let coord_files = coord::file_candidates();

        let mut coord_types: Vec<String> = coord::type_candidates(true)
            .into_iter()
            .chain(coord::type_candidates(false))
            .map(|ty| ty.to_string())
            .collect();
        coord_types.sort();
        coord_types.dedup();

        (schem_files, coord_files, coord_types)
    }

    /// 重推一次这个玩家的全部软枚举（BUG-032 A-t2）。
    ///
    /// 只在**文件目录真的变了**之后调用（`//save`、`/coord` 写文件、undo/redo）：
    /// Bedrock 客户端不会自己重新要候选值，服务端不推它就一直是登录时那一份。
    /// 非 Bedrock 玩家直接返回（Java 侧走 `add_command_completions` 那套）。
    fn refresh_bedrock_soft_enums(&mut self, player_idx: usize) {
        let Some(player) = self.players.get(player_idx) else {
            return;
        };
        if !player.client.is_bedrock() {
            return;
        }
        let (schem_files, coord_files, coord_types) = self.bedrock_soft_enum_values(player);
        let schem_count = schem_files.len();
        let coord_count = coord_files.len();
        player.client.send_soft_enum("schem_files", schem_files);
        player.client.send_soft_enum("coord_files", coord_files);
        player.client.send_soft_enum("coord_types", coord_types);
        println!(
            "[B4] 软枚举刷新（玩家 {}）：schem_files={schem_count}、coord_files={coord_count}",
            player.username
        );
    }

    fn get_chunk_distance(x1: i32, z1: i32, x2: i32, z2: i32) -> u32 {
        let x = x1 - x2;
        let z = z1 - z2;
        x.abs().max(z.abs()) as u32
    }

    fn set_chunk_loaded_at_player(
        &mut self,
        player_idx: usize,
        chunk_x: i32,
        chunk_z: i32,
        was_loaded: bool,
        should_be_loaded: bool,
    ) {
        if was_loaded && !should_be_loaded {
            // let unload_chunk = CUnloadChunk { chunk_x, chunk_z }.encode();
            // self.players[player_idx].client.send_packet(&unload_chunk);
        } else if !was_loaded && should_be_loaded {
            if !Plot::chunk_in_plot_bounds(self.world.x, self.world.z, chunk_x, chunk_z) {
                self.players[player_idx]
                    .client
                    .send_packet(&Chunk::encode_empty_packet(chunk_x, chunk_z, PLOT_SECTIONS));
            } else {
                let chunk_idx = self.world.get_chunk_index_for_chunk(chunk_x, chunk_z);
                let chunk_data = self.world.chunks[chunk_idx].encode_packet();
                self.players[player_idx].client.send_packet(&chunk_data);
            }
        }
    }

    pub fn update_view_pos_for_player(&mut self, player_idx: usize, force_load: bool) {
        self.publish_world();
        let view_distance = CONFIG.view_distance as i32;
        let (chunk_x, chunk_z) = self.players[player_idx].pos.chunk_pos();
        let last_chunk_x = self.players[player_idx].last_chunk_x;
        let last_chunk_z = self.players[player_idx].last_chunk_z;

        let update_view = CSetCenterChunk { chunk_x, chunk_z }.encode();
        self.players[player_idx].client.send_packet(&update_view);

        if ((last_chunk_x - chunk_x).abs() <= view_distance * 2
            && (last_chunk_z - chunk_z).abs() <= view_distance * 2)
            && !force_load
        {
            let nx = chunk_x.min(last_chunk_x) - view_distance;
            let nz = chunk_z.min(last_chunk_z) - view_distance;
            let px = chunk_x.max(last_chunk_x) + view_distance;
            let pz = chunk_z.max(last_chunk_z) + view_distance;
            for x in nx..=px {
                for z in nz..=pz {
                    let was_loaded = Self::get_chunk_distance(x, z, last_chunk_x, last_chunk_z)
                        <= view_distance as u32;
                    let should_be_loaded =
                        Self::get_chunk_distance(x, z, chunk_x, chunk_z) <= view_distance as u32;
                    self.set_chunk_loaded_at_player(player_idx, x, z, was_loaded, should_be_loaded);
                }
            }
        } else {
            for x in last_chunk_x - view_distance..=last_chunk_x + view_distance {
                for z in last_chunk_z - view_distance..=last_chunk_z + view_distance {
                    self.set_chunk_loaded_at_player(player_idx, x, z, true, false);
                }
            }
            for x in chunk_x - view_distance..=chunk_x + view_distance {
                for z in chunk_z - view_distance..=chunk_z + view_distance {
                    self.set_chunk_loaded_at_player(player_idx, x, z, false, true);
                }
            }
        }
        self.players[player_idx].last_chunk_x = chunk_x;
        self.players[player_idx].last_chunk_z = chunk_z;
    }

    fn handle_use_item_impl(&mut self, use_item_on: &SUseItemOn, player: usize) {
        let block_pos = BlockPos::new(use_item_on.x, use_item_on.y, use_item_on.z);
        let block_face = BlockFace::from_id(use_item_on.face as u32);

        let cancel = |plot: &mut Plot| {
            plot.send_block_corrections(&[block_pos, block_pos.offset(block_face)]);
        };

        let selected_slot = self.players[player].selected_slot as usize;
        let item_in_hand = if use_item_on.hand == 0 {
            // Slot in hotbar
            self.players[player].inventory[selected_slot + 36].clone()
        } else {
            // Slot for left hand
            self.players[player].inventory[45].clone()
        };

        if !Plot::in_plot_bounds(self.world.x, self.world.z, block_pos.x, block_pos.z) {
            self.players[player].send_system_message("Can't interact with blocks outside of plot");
            cancel(self);
            return;
        }

        if let Some(item) = &item_in_hand {
            let has_permission = self.players[player].has_permission("worldedit.selection.pos");
            if item.item_type == (Item::WoodenAxe) && has_permission {
                let same = self.players[player].second_position == Some(block_pos);
                if !same {
                    self.players[player].worldedit_set_second_position(block_pos);
                }
                cancel(self);
                // FIXME: Because the client sends another packet after this for the left hand for
                // most blocks, redpiler will get reset anyways.
                return;
            }
        }

        if let Some(owner) = self.owner {
            let player = &mut self.players[player];
            if owner != player.uuid && !player.has_permission("plots.admin.interact.other") {
                player.send_no_permission_message();
                cancel(self);
                return;
            }
        } else if !self.players[player].has_permission("plots.admin.interact.unowned") {
            self.players[player].send_no_permission_message();
            cancel(self);
            return;
        }

        if self.redpiler.is_active() {
            let block = self.world.get_block(block_pos);
            let lever_or_button = matches!(block, Block::Lever { .. } | Block::StoneButton { .. });
            if lever_or_button && !self.players[player].crouching {
                self.redpiler.on_use_block(block_pos);
                self.publish_world();
                return;
            } else {
                match self.redpiler.current_flags() {
                    Some(flags) if flags.io_only => {
                        self.players[player].send_error_message(ERROR_IO_ONLY);
                        cancel(self);
                        return;
                    }
                    _ => {}
                }
                self.reset_redpiler();
            }
        }

        if let Some(item) = item_in_hand {
            let cancelled = interaction::use_item_on_block(
                &item,
                &mut self.world,
                UseOnBlockContext {
                    block_face,
                    block_pos,
                    player: &mut self.players[player],
                    cursor_y: use_item_on.cursor_y,
                },
            );
            if cancelled {
                cancel(self);
            }
            // 立即发布：交互产生的方块变化必须当场发给两种后端，不能等定期发包
            // （否则点拉杆/放方块要等十几秒才看得见 —— BUG-032 第 34 条）。
            self.publish_world();
            return;
        }

        let block = self.world.get_block(block_pos);
        if !self.players[player].crouching {
            interaction::on_use(
                block,
                &mut self.world,
                &mut self.players[player],
                block_pos,
                None,
            );
            self.publish_world();
        }
    }

    fn handle_player_digging(&mut self, block_pos: BlockPos, player: usize) {
        let block = self.world.get_block(block_pos);

        if !Plot::in_plot_bounds(self.world.x, self.world.z, block_pos.x, block_pos.z) {
            self.players[player].send_system_message("Can't break blocks outside of plot");
            return;
        }

        // This worldedit wand stuff should probably be done in another file. It's good enough for
        // now.
        let item_in_hand = self.players[player].inventory
            [self.players[player].selected_slot as usize + 36]
            .clone();
        if let Some(item) = item_in_hand {
            let has_permission = self.players[player].has_permission("worldedit.selection.pos");
            if item.item_type == (Item::WoodenAxe) && has_permission {
                self.send_block_corrections(&[block_pos]);
                if let Some(pos) = self.players[player].first_position
                    && pos == block_pos
                {
                    return;
                }
                self.players[player].worldedit_set_first_position(block_pos);
                return;
            }
        }

        if let Some(owner) = self.owner {
            let player = &mut self.players[player];
            if owner != player.uuid && !player.has_permission("plots.admin.interact.other") {
                player.send_no_permission_message();
                self.send_block_corrections(&[block_pos]);
                return;
            }
        } else if !self.players[player].has_permission("plots.admin.interact.unowned") {
            self.players[player].send_no_permission_message();
            self.send_block_corrections(&[block_pos]);
            return;
        }

        match self.redpiler.current_flags() {
            Some(flags) if flags.io_only => {
                self.players[player].send_error_message(ERROR_IO_ONLY);
                self.send_block_corrections(&[block_pos]);
                return;
            }
            _ => {}
        }

        self.reset_redpiler();

        interaction::destroy(block, &mut self.world, block_pos);
        // 同 handle_use_item_impl：破坏方块也要立刻发出去（BUG-032 第 34 条）。
        self.publish_world();

        let effect = CWorldEvent {
            event: 2001,
            x: block_pos.x,
            y: block_pos.y,
            z: block_pos.z,
            data: block.get_id() as i32,
            disable_relative_volume: false,
        }
        .encode();
        for other_player in 0..self.players.len() {
            if player == other_player {
                continue;
            };
            self.players[other_player].client.send_packet(&effect);
        }
    }

    /// After an expensive operation or change in timings, it's important to
    /// call this function so our timings monitor doesn't think we're running
    /// behind.
    fn reset_timings(&mut self) {
        self.lag_time = Duration::ZERO;
        self.last_update_time = Instant::now();
        self.last_nspt = None;
        self.timings.reset_timings();
    }

    /// Handles an automation IO request against the running redpiler circuit.
    ///
    /// Every answer that depends on the circuit lives here: the handshake tick
    /// counter, `POUT` bindings, `INPUT` reservations and `READ`. Errors are a
    /// single code from BACKLOG.md §7.11.
    fn handle_automation_request(
        &mut self,
        conn_id: u64,
        request: AutomationRequest,
    ) -> AutomationResponse {
        // Remember who holds the reservation lock, so the `INPUT <name>
        // <offset> <hex>` form can be restricted to that connection.
        if request.exclusive {
            self.exclusive = Some(conn_id);
        }
        match request.kind {
            RequestKind::Hello => self.handle_hello(),
            RequestKind::Bind { name } => self.handle_bind(conn_id, &name),
            RequestKind::Input { name, offset, hex } => {
                self.handle_input(conn_id, &name, offset, &hex)
            }
            RequestKind::ReadBit { name, bit } => self.handle_read_bit(&name, bit),
            RequestKind::ReadPos { pos } => self.handle_read_pos(pos),
            RequestKind::TickNow => self.handle_tick_now(),
            RequestKind::Radv { ticks } => self.handle_radv(ticks),
        }
    }

    /// `HELLO MCHPRS_IO 0.5.2-alpha [<tick>]`; the tick field is omitted while
    /// no circuit is compiled (BACKLOG.md §7.1).
    fn handle_hello(&self) -> AutomationResponse {
        AutomationResponse::Hello {
            tick: self.redpiler.is_active().then_some(self.tick),
        }
    }

    /// `POUT <name>`: bind the connection to an output group and answer with its
    /// current value.
    ///
    /// A repeated `POUT` for a group the connection is already bound to is
    /// idempotent and pushes nothing (BACKLOG.md §7.3).
    fn handle_bind(&mut self, conn_id: u64, name: &str) -> AutomationResponse {
        if !self.redpiler.is_active() {
            return AutomationResponse::Err("nocompile");
        }
        let Some(group) = self.io.find(name) else {
            return AutomationResponse::Err("name");
        };
        if group.kind.is_input() {
            return AutomationResponse::Err("not_output");
        }
        if !group.active {
            return AutomationResponse::Err("disabled");
        }
        let bound = self
            .pout_bindings
            .get(&conn_id)
            .is_some_and(|groups| groups.contains(name));
        if bound {
            // Re-`POUT` on a group we already listen to means "give me the
            // current value once more"; the binding and the push rules stay
            // exactly the same (BACKLOG.md BUG-007).
            let bits = self
                .io
                .find(name)
                .and_then(|group| group.read(&self.redpiler))
                .unwrap_or_default();
            let hex = encode_hex(&bits, self.io.find(name).and_then(|group| group.matrix));
            return AutomationResponse::Output {
                name: name.to_string(),
                tick: self.tick,
                hex,
            };
        }
        self.pout_bindings
            .entry(conn_id)
            .or_default()
            .insert(name.to_string());

        // Seed the change detector with the current state, so the first poll
        // only reports changes that happen after the binding, then send the
        // current value as the binding confirmation.
        let bits = {
            let Some(group) = self.io.find_mut(name) else {
                return AutomationResponse::Err("name");
            };
            group.prime(&self.redpiler);
            group.read(&self.redpiler).unwrap_or_default()
        };
        let hex = encode_hex(&bits, self.io.find(name).and_then(|group| group.matrix));
        AutomationResponse::Output {
            name: name.to_string(),
            tick: self.tick,
            hex,
        }
    }

    /// `INPUT <name> [<offset>] <hex>`: schedule a word for a group.
    ///
    /// Without an offset it lands on the next tick. With an offset it is a
    /// reservation, which needs `exact` mode plus `rtps 0` and the exclusive
    /// script (BACKLOG.md §7.2 / §7.13).
    fn handle_input(
        &mut self,
        conn_id: u64,
        name: &str,
        offset: Option<u16>,
        hex: &str,
    ) -> AutomationResponse {
        if !self.redpiler.is_active() {
            return AutomationResponse::Err("nocompile");
        }
        if offset.is_some() && !self.reservations_allowed(conn_id) {
            // Reservations only exist in `/tick seq` mode (BACKLOG.md BUG-005).
            return AutomationResponse::Err("mode");
        }
        let Some(group) = self.io.find(name) else {
            return AutomationResponse::Err("name");
        };
        if !group.active {
            return AutomationResponse::Err("disabled");
        }
        if !group.kind.is_input() {
            // The protocol has no separate code for "this group only reads";
            // the request cannot be carried out as written.
            return AutomationResponse::Err("not_output");
        }
        let bits = match decode_hex(hex, group.bits(), group.matrix) {
            Ok(bits) => bits,
            Err(()) => return AutomationResponse::Err("hex"),
        };

        let absolute = self.tick.wrapping_add(u32::from(offset.unwrap_or(1)));
        match self.reserve_input(name, bits, absolute) {
            Ok(()) => AutomationResponse::Input {
                name: name.to_string(),
                tick: absolute,
            },
            Err(code) => AutomationResponse::Err(code),
        }
    }

    /// `true` when the connection may use the reservation form of `INPUT`:
    /// only the exclusive script of an active `/tick seq` session may reserve
    /// (BACKLOG.md BUG-004 / BUG-005).
    fn reservations_allowed(&self, conn_id: u64) -> bool {
        self.seq_mode
            && CONFIG.io_mode() == IoMode::Exact
            && self.rtps_zero()
            && self.exclusive == Some(conn_id)
    }

    /// Adds a reservation, replacing the one for the same tick and group
    /// (BACKLOG.md §7.2: later requests win).
    fn reserve_input(
        &mut self,
        group: &str,
        bits: Vec<bool>,
        tick: u32,
    ) -> Result<(), &'static str> {
        if let Some(existing) = self
            .reservations
            .iter_mut()
            .find(|reservation| reservation.tick == tick && reservation.group == group)
        {
            existing.bits = bits;
            return Ok(());
        }
        if self.reservations.len() >= MAX_RESERVATIONS {
            return Err("full");
        }
        self.reservations.push(Reservation {
            tick,
            group: group.to_string(),
            bits,
        });
        Ok(())
    }

    /// `READ <name>:<bit>`: bit `0` is the group's last coordinate, for analog
    /// groups it counts points and answers the strength of that point
    /// (BACKLOG.md §7.4).
    fn handle_read_bit(&mut self, name: &str, bit: u32) -> AutomationResponse {
        if !self.redpiler.is_active() {
            return AutomationResponse::Err("nocompile");
        }
        let Some(group) = self.io.find(name) else {
            return AutomationResponse::Err("name");
        };
        if !group.active {
            return AutomationResponse::Err("disabled");
        }
        let len = group.points.len();
        let Ok(bit) = usize::try_from(bit) else {
            return AutomationResponse::Err("range");
        };
        if bit >= len {
            return AutomationResponse::Err("range");
        }
        let pos = group.points[len - 1 - bit];
        let power = if group.kind.is_analog() {
            match self.redpiler.query_wire_power(pos) {
                Some(power) => power,
                None => return AutomationResponse::Err("no_io"),
            }
        } else {
            match self.redpiler.query_io(pos) {
                Some((_, powered, _)) => u8::from(powered),
                None => return AutomationResponse::Err("no_io"),
            }
        };
        AutomationResponse::Read {
            target: format!("{}:{}", name, bit),
            tick: self.tick,
            power,
        }
    }

    /// `READ <x,y,z>`: read the IO element at an absolute coordinate. Redstone
    /// dust answers its strength (`0-15`), every other element answers `0/1`
    /// (BACKLOG.md §7.4 / §7.7).
    fn handle_read_pos(&mut self, pos: BlockPos) -> AutomationResponse {
        if !self.redpiler.is_active() {
            return AutomationResponse::Err("nocompile");
        }
        let target = format!("{},{},{}", pos.x, pos.y, pos.z);
        if let Some((_, powered, _)) = self.redpiler.query_io(pos) {
            return AutomationResponse::Read {
                target,
                tick: self.tick,
                power: u8::from(powered),
            };
        }
        match self.redpiler.query_wire_power(pos) {
            Some(power) => AutomationResponse::Read {
                target,
                tick: self.tick,
                power,
            },
            None => AutomationResponse::Err("no_io"),
        }
    }

    /// `TICK NOW`: the latest completed tick (BACKLOG.md §7.2).
    fn handle_tick_now(&self) -> AutomationResponse {
        if !self.redpiler.is_active() {
            return AutomationResponse::Err("nocompile");
        }
        AutomationResponse::Now { tick: self.tick }
    }

    /// Re-validates every `io.txt` group against the freshly compiled graph and
    /// reports the ones that were disabled.
    ///
    /// A failed group only disables itself (BACKLOG.md §3.7). The report goes to
    /// the player that started the compile; scripts cannot compile, so there is
    /// no wire report any more (BACKLOG.md BUG-008).
    fn validate_io_groups(&mut self, source: Option<usize>) {
        self.io.validate(&self.redpiler, &self.world);
        // An io clock may not share its button with a memory clock: both would
        // pulse the same element (BACKLOG.md §5.4).
        let memory_clocks: Vec<BlockPos> = self
            .memories
            .iter()
            .filter(|state| state.active)
            .filter_map(|state| state.memory.clock)
            .collect();
        for group in &mut self.io.groups {
            if let Some(clk) = group.clock
                && memory_clocks.contains(&clk)
            {
                group.active = false;
                group.reason = Some(format!(
                    "the clock ({},{},{}) is also a memory clock",
                    clk.x, clk.y, clk.z
                ));
            }
        }
        let failures: Vec<(String, String)> = self
            .io
            .groups
            .iter()
            .filter(|group| !group.active)
            .map(|group| {
                (
                    group.name.clone(),
                    group.reason.clone().unwrap_or_default(),
                )
            })
            .collect();
        for (name, reason) in &failures {
            warn!("io group `{}` disabled: {}", name, reason);
        }
        if failures.is_empty() {
            return;
        }
        // Automatic compiles do not report (BACKLOG.md §3.7); only a player or
        // the console sees the failures.
        if let Some(player) = source {
            for (name, reason) in &failures {
                self.players[player]
                    .send_error_message(&format!("io group `{}` disabled: {}", name, reason));
            }
        }
    }

    /// `RADV <ticks>`: manual stepping started by a script (BACKLOG.md BUG-008).
    ///
    /// A script may only step while nothing else advances the circuit; the
    /// player command `/radv` has no such restriction (BACKLOG.md BUG-006).
    fn handle_radv(&mut self, ticks: u32) -> AutomationResponse {
        if !self.redpiler.is_active() {
            return AutomationResponse::Err("nocompile");
        }
        if !self.rtps_zero() {
            return AutomationResponse::Err("mode");
        }
        self.advance_ticks(u64::from(ticks));
        AutomationResponse::Radv {
            ticks,
            tick: self.tick,
        }
    }

    /// The tick counter as shown by `/tick`; `None` while nothing is compiled
    /// (BACKLOG.md BUG-003).
    pub(super) fn current_tick(&self) -> Option<u32> {
        self.redpiler.is_active().then_some(self.tick)
    }

    /// `/tick seq`: kick every script, set rtps to 0 and enter reservation mode
    /// (BACKLOG.md BUG-004). Running it again re-arms the session: connections
    /// are dropped again and pending reservations are cleared.
    ///
    /// `Err("nocompile")` means nothing is compiled yet, `Err("performance")`
    /// means `performance = true`; reservation mode does not exist in either
    /// case.
    pub(super) fn start_seq_mode(&mut self) -> Result<(), &'static str> {
        if !self.redpiler.is_active() {
            return Err("nocompile");
        }
        if CONFIG.io_mode() != IoMode::Exact {
            return Err("performance");
        }
        self.seq_mode = true;
        self.reservations.clear();
        self.exclusive = None;
        self.apply_rtps(Tps::Limited(0.0));
        let _ = self.message_sender.send(Message::SeqModeChanged { on: true });
        Ok(())
    }

    /// Applies a new rtps value exactly like the player command does.
    fn apply_rtps(&mut self, tps: Tps) {
        self.sleep_time = sleep_time_for_tps(tps);
        self.timings.set_tps(tps);
        self.tps = tps;
        self.reset_timings();
    }

    /// Player issued `/rtps <n>` (BACKLOG.md BUG-005).
    ///
    /// A plain `/rtps 0` only changes the speed: it neither kicks scripts nor
    /// grants reservations. Automatic speeds leave reservation mode, which is
    /// the only way to unlock apart from a compile.
    fn on_player_rtps(&mut self, args: &[&str]) {
        let Some(arg) = args.first() else {
            return;
        };
        let parsed = arg.parse::<f32>().ok();
        let automatic =
            parsed.is_some_and(|value| value > 0.0) || "unlimited".starts_with(*arg);
        if !automatic {
            return;
        }
        self.seq_mode = false;
        self.exclusive = None;
        let _ = self
            .message_sender
            .send(Message::SeqModeChanged { on: false });
    }

    /// `/io list`: a summary of `io.txt` only (BACKLOG.md §7.11). While no
    /// circuit is compiled only the format errors are known; a compiled plot
    /// also reports the groups that were disabled by validation.
    fn io_summary(&self) -> String {
        let mut message = format!(
            "io.txt: {} group(s), {} format error(s)",
            self.io.groups.len(),
            self.io.errors.len()
        );
        if !self.io.errors.is_empty() {
            let lines: Vec<String> = self
                .io
                .errors
                .iter()
                .map(|error| {
                    if error.line == 0 {
                        "unknown line".to_string()
                    } else {
                        format!("line {}", error.line)
                    }
                })
                .collect();
            message.push_str(&format!(" ({})", lines.join(", ")));
        }
        if self.redpiler.is_active() {
            let disabled: Vec<&str> = self
                .io
                .groups
                .iter()
                .filter(|group| !group.active)
                .map(|group| group.name.as_str())
                .collect();
            message.push_str(&format!(", {} disabled", disabled.len()));
            if !disabled.is_empty() {
                message.push_str(&format!(" ({})", disabled.join(", ")));
            }
            // Analog points only exist in the graph when the compile was given
            // `-d` / `-c`; say so whenever the file declares analog groups
            // (BACKLOG.md §3.9).
            if self.io.groups.iter().any(|group| group.kind.is_analog()) {
                message.push_str("; analog groups require -d -c when compiling");
            }
        }
        message
    }

    fn send_io_list(&mut self, player: usize) {
        let message = self.io_summary();
        self.players[player].send_system_message(&message);
    }

    /// Advances every active memory by one sample. Called right after the
    /// tick so the address reflects the state that tick produced.
    fn run_memories(&mut self) {
        if self.memories.is_empty() {
            return;
        }
        for state in &mut self.memories {
            state.sample(&mut self.redpiler);
        }
        // RAM contents are only written back at explicit events (shutdown,
        // /redpiler reset, a compile crash and /memory reload); the tick loop
        // never touches the disk, so a large RAM cannot stall a tick
        // (BACKLOG.md BUG-021 / BUG-026).
    }

    /// Forces every dirty memory to disk. Used on shutdown, on a compile crash
    /// and before `/memory reload`.
    fn flush_memories(&mut self) {
        for state in &mut self.memories {
            if !state.active {
                // A memory that failed validation must not carry its contents
                // back to disk (BACKLOG.md §7.11: the data is discarded).
                state.discard();
                continue;
            }
            if state.is_dirty() {
                if let Err(err) = state.save() {
                    warn!("{}", err);
                }
            }
        }
    }

    /// Validates every memory against the freshly compiled graph. A memory that
    /// does not match is disabled and reported; the others keep working
    /// (BACKLOG.md BUG-015).
    fn validate_memories(&mut self, source: Option<usize>) {
        for state in &mut self.memories {
            state.reset_edges();
            match state.memory.validate(&self.redpiler, &self.world) {
                Ok(()) => state.active = true,
                Err(invalid) => {
                    state.active = false;
                    let message = memory::describe_invalid(&state.memory, &invalid);
                    warn!("{}", message);
                    if let Some(player) = source {
                        self.players[player].send_error_message(&message);
                    }
                }
            }
        }
    }

    /// Re-reads `memory/*.rom.txt` from disk and re-validates every definition.
    /// Returns `(loaded, active)` (BACKLOG.md BUG-015).
    /// `/tick freeze`: remembers the current rtps, then sets it to 0.
    fn freeze_rtps(&mut self) {
        if self.tps != Tps::Limited(0.0) {
            self.frozen_rtps = Some(self.tps);
        }
        self.apply_rtps(Tps::Limited(0.0));
    }

    /// `/tick unfreeze`: restores the rtps saved by `freeze`. With nothing
    /// saved the plot simply stays as it is.
    fn unfreeze_rtps(&mut self) -> Option<Tps> {
        let restored = self.frozen_rtps.take()?;
        self.apply_rtps(restored);
        Some(restored)
    }

    /// `/io reload`: same contract as `/memory reload` (BACKLOG.md BUG-012) —
    /// refused while a `-i` compile is active, otherwise the compiled state is
    /// dropped and `io.txt` is parsed again. It never starts a compile.
    fn reload_io(&mut self) -> ReloadOutcome {
        if self
            .redpiler
            .current_flags()
            .map(|flags| flags.io_only)
            .unwrap_or(false)
        {
            return ReloadOutcome::CompiledIoOnly;
        }
        if self.redpiler.is_active() {
            self.reset_redpiler();
            self.publish_world();
        }
        self.io = io::IoTable::load(Path::new(IO_FILE));
        let rejected = self
            .io
            .errors
            .iter()
            .map(|error| format!("io.txt line {}: {}", error.line, error.message))
            .collect();
        ReloadOutcome::Reloaded {
            loaded: self.io.groups.len(),
            rejected,
        }
    }

    fn reload_memories(&mut self) -> ReloadOutcome {
        // `-i` (io only) compiles never re-read memory definitions, and a
        // reload is refused while such a compile is running (BACKLOG.md
        // §4.4 / BUG-030).
        if self
            .redpiler
            .current_flags()
            .map(|flags| flags.io_only)
            .unwrap_or(false)
        {
            return ReloadOutcome::CompiledIoOnly;
        }
        if self.redpiler.is_active() {
            // Compiled: write the RAMs back, leave the compiled state, then
            // re-read the files. No compile is triggered here.
            self.flush_memories();
            self.reset_redpiler();
            self.publish_world();
        }
        // Not compiled: nothing is written, the files are simply re-read.
        let report = memory::load_all(Path::new(memory::MEMORY_DIR));
        self.memories = report.memories;
        ReloadOutcome::Reloaded {
            loaded: self.memories.len(),
            rejected: report.rejected,
        }
    }

    fn publish_world(&mut self) {
        if self.redpiler.is_active() {
            self.redpiler.flush(&mut self.world);
            // `EVENT` pushes (and the `SUB` verb behind them) were removed in
            // 0.5.2-alpha; the queue still has to be drained after every flush.
            self.redpiler.take_io_events();
        }
        // Java 玩家：把攒下的 CMultiBlockChange 发出去。
        // **注意**：`self.world.flush_block_changes()` 是 `crates/world` 里的空实现
        // （`fn flush_block_changes(&mut self) {}`），什么都不做。真正发包的是
        // `Plot::flush_block_changes`（本文件 339 行）。写错过一次，后果是
        // 方块变化永远发不出去、玩家点按钮要等十几秒才看到变化（BACKLOG.md BUG-032 第 34 条）。
        self.world.flush_block_changes();
        self.send_bedrock_block_changes();
        self.last_world_send_time = Instant::now();
    }

    /// 把本 tick 变过的方块推给 Bedrock 玩家（BUG-032 第 3 条）。
    ///
    /// Java 客户端靠 `CMultiBlockChange` 批量包拿到这些变化；Bedrock 没有批量包，
    /// 只能逐条 `UpdateBlock`，所以这里一次把整批塞给会话线程（别一个方块一条指令）。
    fn send_bedrock_block_changes(&mut self) {
        let changes = std::mem::take(&mut self.world.block_changes);
        // 告示牌（A2）：`flush_block_changes` 只登记了位置，这里换成本地文档再下发。
        // 和方块缓存一样，**不管有没有 Bedrock 玩家**都要把队列取空。
        let signs = std::mem::take(&mut self.world.pending_bedrock_signs);
        for pos in signs {
            self.push_bedrock_sign(pos);
        }
        // 方块变化同时让区块缓存失效（BUG-032 第 26 条）：如果玩家离开视距再回来，
        // 客户端会重新请求那个区块，而缓存里可能还是旧状态。
        // **这一步不能放在"有没有 Bedrock 玩家"之后** —— 缓存是全 plot 共享的。
        for (pos, _) in &changes {
            self.bedrock_chunk_dirty
                .insert((pos.x.div_euclid(16), pos.z.div_euclid(16)));
        }
        if changes.is_empty() || !self.players.iter().any(|p| p.client.is_bedrock()) {
            return;
        }
        let batch: Vec<(i32, i32, i32, u32)> = changes
            .iter()
            .map(|(pos, state)| (pos.x, pos.y, pos.z, *state))
            .collect();
        for player in &self.players {
            player.client.send_block_updates(&batch);
        }
    }

    fn start_redpiler(&mut self, options: CompilerOptions, source: Option<usize>) {
        debug!("Starting redpiler");
        // Every compile updates the whole plot first, silently: `--update`
        // runs `mchprs_redstone::update` over every non-empty section after the
        // graph is built (BACKLOG.md BUG-023).
        let mut options = options;
        options.update = true;
        // `betterio` is a runtime timing switch, not a compile option
        // (BACKLOG.md BUG-021): push the Config value into the backend
        // before every compile so a restart is all it takes.
        mchprs_redpiler::set_betterio(CONFIG.betterio);
        // A `/coord` edit is not visible until the files are read again; the
        // next compile does that once (BACKLOG.md BUG-022).
        if self.coord_dirty {
            self.io = io::IoTable::load(Path::new(IO_FILE));
            let report = memory::load_all(Path::new(memory::MEMORY_DIR));
            for message in &report.rejected {
                warn!("{}", message);
            }
            self.memories = report.memories;
            self.coord_dirty = false;
        }
        self.scoreboard
            .set_redpiler_state(&self.players, RedpilerState::Compiling);
        self.scoreboard
            .set_redpiler_options(&self.players, &options);

        let bounds = self.world.get_corners();
        let monitor: Arc<TaskMonitor> = Default::default();
        let ticks = self.world.to_be_ticked.drain(..).collect();

        let mut players_need_updates = HashSet::new();
        let compile_result = thread::scope(|s| {
            let handle = s.spawn(|| {
                self.redpiler
                    .compile(&self.world, bounds, options, ticks, monitor.clone())
            });
            while !handle.is_finished() {
                // We'll update the players so that they don't time out.
                for player_idx in 0..self.players.len() {
                    if self.players[player_idx].update() {
                        // Unforunately we can't update a players view position
                        // since we don't have access to the world, but we can
                        // save the players that need updating for later.
                        players_need_updates.insert(player_idx);
                    }
                }
                thread::sleep(Duration::from_millis(20));
            }
            // Join explicitly so the compile's outcome is observable. A compile
            // that stops on an unreadable comparator override reports it to the
            // player instead of unwinding through this thread.
            handle.join()
        });

        // Now that we have ownership of the world again, we can update player view positions
        for player_idx in players_need_updates {
            self.update_view_pos_for_player(player_idx, false);
        }

        match compile_result {
            Ok(Ok(())) => {
                self.scoreboard
                    .set_redpiler_state(&self.players, RedpilerState::Running);
                self.reset_timings();
                // The compile succeeded, but empty containers may have been
                // read as 0 (BACKLOG.md BUG-001 revision, BUG-014). Report every
                // distinct position to the player who started the compile;
                // automatic compiles stay silent.
                if let Some(player) = source {
                    let positions: Vec<BlockPos> = monitor
                        .warnings()
                        .into_iter()
                        .filter_map(|warning| match warning {
                            OverrideError::EmptyContainer { pos, .. } => Some(pos),
                            _ => None,
                        })
                        .collect();
                    if !positions.is_empty() {
                        let message = format_empty_container_warning(&positions);
                        self.players[player].send_error_message(&message);
                    }
                }
                self.validate_memories(source);

                // 0.5.2-alpha: a finished compile starts from tick 0, drops
                // every binding and every reservation, and re-validates the
                // `io.txt` groups against the new graph (BACKLOG.md §7.2 /
                // §7.3).
                self.clear_io_state();
                self.validate_io_groups(source);


            }
            Ok(Err(CompileError::Override(error))) => {
                self.scoreboard
                    .set_redpiler_state(&self.players, RedpilerState::Stopped);
                self.scoreboard
                    .set_redpiler_options(&self.players, &Default::default());
                if let Some(player) = source {
                    let message = match error {
                        OverrideError::EmptyContainer { pos, kind } => format!(
                            "Redpiler compile aborted: comparator at {} reads an empty {}",
                            pos, kind
                        ),
                        OverrideError::Unsupported { pos, .. } => format!(
                            "Redpiler compile aborted: comparator at {} \
                             reads a block with no comparator override",
                            pos
                        ),
                    };
                    self.players[player].send_error_message(&message);
                }
            }
            Ok(Err(CompileError::Cancelled)) => {
                self.scoreboard
                    .set_redpiler_state(&self.players, RedpilerState::Stopped);
                self.scoreboard
                    .set_redpiler_options(&self.players, &Default::default());
            }
            // A panic during compilation is unrecoverable (BACKLOG.md
            // BUG-011): save the plot, report what happened, and terminate the
            // process so a half-dead server is never left running.
            Err(payload) => {
                let reason = panic_payload_message(payload.as_ref());
                let saved_to = self.crash_save();
                // RAM contents are persisted by their own files, so a crash
                // must not lose them (BACKLOG.md BUG-021).
                self.flush_memories();
                error!(
                    "[CRASH] plot ({}, {}) redpiler panic: {} — saved to {} — exiting with code 1",
                    self.world.x, self.world.z, reason, saved_to
                );
                let _ = std::io::stdout().flush();
                let _ = std::io::stderr().flush();
                std::process::exit(1);
            }
        }
    }

    /// Clears the runtime state that belongs to a compiled circuit: the tick
    /// counter, every `POUT` binding, every reservation and the IO groups'
    /// change detectors (BACKLOG.md §7.2 / §7.3).
    fn clear_io_state(&mut self) {
        self.tick = 0;
        self.pout_bindings.clear();
        self.reservations.clear();
        self.exclusive = None;
        // A new compile always ends reservation mode (BACKLOG.md BUG-004).
        if self.seq_mode {
            self.seq_mode = false;
            let _ = self
                .message_sender
                .send(Message::SeqModeChanged { on: false });
        }
        for group in &mut self.io.groups {
            group.active = false;
            group.reason = None;
            group.reset_edges();
        }
    }

    /// Redpiler needs to reset implicitly in the case of any block changes done by a player. This
    /// can be
    fn reset_redpiler(&mut self) {
        // The circuit that drives the memories is about to disappear, so a RAM
        // must not carry unsaved words into the next compile (BACKLOG.md
        // BUG-021).
        self.flush_memories();
        if self.redpiler.is_active() {
            debug!("Discarding redpiler");
            let bounds = self.world.get_corners();
            self.redpiler.reset(&mut self.world, bounds);
            self.scoreboard
                .set_redpiler_state(&self.players, RedpilerState::Stopped);
            self.scoreboard
                .set_redpiler_options(&self.players, &Default::default());

            // reseting redpiler could cause a large amount of block updates
            self.reset_timings();
        }
        // The circuit that owned the tick counter, the bindings, the
        // reservations and the group state is gone.
        self.clear_io_state();
    }

    fn destroy_entity(&mut self, entity_id: u32) {
        let destroy_entity = CRemoveEntities {
            entity_ids: vec![entity_id as i32],
        }
        .encode();
        for player in &mut self.players {
            player.client.send_packet(&destroy_entity);
        }
    }

    fn leave_plot(&mut self, uuid: u128) -> Player {
        let player_idx = self.players.iter().position(|p| p.uuid == uuid).unwrap();
        // **`packet_senders` 不是按玩家下标对齐的**：`enter_plot` 只给 **Java** 后端
        // push 一条（Bedrock 玩家的方块广播走另一条通道）。所以：
        //   - 离开的是 Bedrock 玩家 → 他本来就不在表里，**什么都不删**；
        //   - 离开的是 Java 玩家 → 删"在他之前进来的 Java 玩家数"那个下标。
        // 直接拿玩家下标去 remove 会删错人甚至越界 panic（BUG-032 第 41 条：
        // 场上有基岩玩家时，任何人离开 plot 都会炸掉 plot 线程）。
        if !self.players[player_idx].client.is_bedrock() {
            let java_idx = self.players[..player_idx]
                .iter()
                .filter(|p| !p.client.is_bedrock())
                .count();
            if java_idx < self.world.packet_senders.len() {
                self.world.packet_senders.remove(java_idx);
            }
        }
        let player = self.players.remove(player_idx);

        let destroy_other_entities = CRemoveEntities {
            entity_ids: self.players.iter().map(|p| p.entity_id as i32).collect(),
        }
        .encode();
        player.client.send_packet(&destroy_other_entities);

        let chunk_offset_x = self.world.x << PLOT_SCALE;
        let chunk_offset_z = self.world.z << PLOT_SCALE;
        for chunk in &self.world.chunks {
            player.client.send_packet(
                &CUnloadChunk {
                    chunk_x: chunk_offset_x + chunk.x,
                    chunk_z: chunk_offset_z + chunk.z,
                }
                .encode(),
            );
        }
        self.destroy_entity(player.entity_id);
        self.locked_players.remove(&player.entity_id);
        self.scoreboard.remove_player(&player);
        player
    }

    fn chunk_in_plot_bounds(plot_x: i32, plot_z: i32, chunk_x: i32, chunk_z: i32) -> bool {
        let (x, z) = (chunk_x >> PLOT_SCALE, chunk_z >> PLOT_SCALE);
        plot_x == x && plot_z == z
    }

    fn in_plot_bounds(plot_x: i32, plot_z: i32, x: i32, z: i32) -> bool {
        Plot::chunk_in_plot_bounds(plot_x, plot_z, x >> 4, z >> 4)
    }

    pub fn claim_plot(&mut self, plot_x: i32, plot_z: i32, player: usize) {
        let player = &mut self.players[player];
        database::claim_plot(plot_x, plot_z, &format!("{:032x}", player.uuid));
        let center = Plot::get_center(plot_x, plot_z);
        player.teleport(PlayerPos::new(center.0, 64.0, center.1));
        player.send_system_message(&format!("Claimed plot {},{}", plot_x, plot_z));
    }

    pub fn get_center(plot_x: i32, plot_z: i32) -> (f64, f64) {
        const WIDTH: f64 = PLOT_BLOCK_WIDTH as f64;
        (
            plot_x as f64 * WIDTH + WIDTH / 2.0,
            plot_z as f64 * WIDTH + WIDTH / 2.0,
        )
    }

    pub fn get_next_plot(plot_x: i32, plot_z: i32) -> (i32, i32) {
        let x = plot_x.abs();
        let z = plot_z.abs();

        match x.cmp(&z) {
            Ordering::Greater => {
                if plot_x > 0 {
                    (plot_x, plot_z + 1)
                } else {
                    (plot_x, plot_z - 1)
                }
            }
            Ordering::Less => {
                if plot_z > 0 {
                    (plot_x - 1, plot_z)
                } else {
                    (plot_x + 1, plot_z)
                }
            }
            Ordering::Equal => {
                if plot_x == plot_z && plot_x > 0 || plot_x == x {
                    (plot_x, plot_z + 1)
                } else if plot_z == z {
                    (plot_x, plot_z - 1)
                } else {
                    (plot_x + 1, plot_z)
                }
            }
        }
    }

    fn handle_commands(&mut self) {
        let mut removal_offset = 0;
        for player_idx in 0..self.players.len() {
            let player_idx = player_idx - removal_offset;
            let commands: Vec<String> = self.players[player_idx].command_queue.drain(..).collect();
            for command in commands {
                let mut args: Vec<&str> = command.split(' ').collect();
                let command = args.remove(0);
                // 0.5.2-alpha hooks the command table cannot express:
                // `/rtps` has to keep the automation bridge's manual-mode lock
                // in sync, and `/io list` is not part of `handle_command`.
                if command == "rtps" {
                    self.on_player_rtps(&args);
                } else if command == "io" && args.first() == Some(&"list") {
                    self.send_io_list(player_idx);
                    continue;
                }
                if self.handle_command(player_idx, command, args) {
                    removal_offset += 1;
                }
            }
        }
    }

    fn handle_messages(&mut self) {
        while let Ok(message) = self.message_receiver.try_recv() {
            match message {
                BroadcastMessage::Chat(_sender, message) => {
                    for player in &mut self.players {
                        player.send_chat_message(&message);
                    }
                }
                BroadcastMessage::PlayerJoinedInfo(player_join_info) => {
                    let player_info = CPlayerInfoUpdate {
                        players: vec![CPlayerInfoUpdatePlayer {
                            uuid: player_join_info.uuid,
                            actions: CPlayerInfoActions {
                                add_player: Some(CPlayerInfoAddPlayer {
                                    name: player_join_info.username,
                                    properties: player_join_info.properties,
                                }),
                                update_gamemode: Some(player_join_info.gamemode.get_id()),
                                ..Default::default()
                            },
                        }],
                    }
                    .encode();
                    for player in &mut self.players {
                        player.client.send_packet(&player_info);
                    }
                }
                BroadcastMessage::PlayerLeft(uuid) => {
                    let player_info = CPlayerInfoRemove {
                        players: vec![uuid],
                    }
                    .encode();
                    for player in &mut self.players {
                        player.client.send_packet(&player_info);
                    }
                }
                BroadcastMessage::Shutdown => {
                    let mut players: Vec<Player> = self.players.drain(..).collect();
                    for player in players.iter_mut() {
                        player.save();
                        player.kick("Server closed".into());
                    }
                    self.flush_memories();
                    self.always_running = false;
                    self.running = false;
                    return;
                }
                BroadcastMessage::PlayerUpdateGamemode(uuid, gamemode) => {
                    let player_info = CPlayerInfoUpdate {
                        players: vec![CPlayerInfoUpdatePlayer {
                            uuid,
                            actions: CPlayerInfoActions {
                                update_gamemode: Some(gamemode.get_id()),
                                ..Default::default()
                            },
                        }],
                    }
                    .encode();
                    for player in &mut self.players {
                        player.client.send_packet(&player_info);
                    }
                }
            }
        }
        // Handle messages from the private message channel
        while let Ok(message) = self.priv_message_receiver.try_recv() {
            match message {
                PrivMessage::PlayerEnterPlot(player) => {
                    self.enter_plot(player);
                }
                PrivMessage::PlayerTeleportOther(mut player, username) => {
                    if let Some(other) = self.players.iter().find(|p| p.username == username) {
                        player.teleport(other.pos);
                    }
                    self.enter_plot(player);
                }
                PrivMessage::AutomationRequest { conn_id, request } => {
                    let reply = self.handle_automation_request(conn_id, request);
                    let _ = self.message_sender.send(Message::IoResponse(conn_id, reply));
                }
                PrivMessage::ConsoleCommand(line) => {
                    self.handle_console_command(&line);
                }
                PrivMessage::AutomationDisconnect { conn_id } => {
                    // The owning connection is gone; drop its bindings so no
                    // stale `OUTPUT` is ever pushed to it.
                    self.pout_bindings.remove(&conn_id);
                    if self.exclusive == Some(conn_id) {
                        // The session stays armed in the bridge but nobody may
                        // reserve until `/tick seq` runs again (BACKLOG.md
                        // BUG-004).
                        self.exclusive = None;
                    }
                }
                PrivMessage::BedrockSnapshot {
                    reply,
                    chunk_radius,
                    spawn,
                } => {
                    // 只读快照：在本线程里读方块、编码成 Bedrock 区块，再回传给监听线程。
                    // 不写世界、不发更新（BACKLOG.md BUG-032「最小只读通路」）。
                    let snapshot = self.build_bedrock_snapshot(chunk_radius, spawn);
                    let _ = reply.send(snapshot);
                }
            }
        }
    }

    // Note: `remove_oob_players` was removed on purpose (BACKLOG.md BUG-010).
    // Stepping outside the plot no longer kicks the player into a neighbouring
    // plot; that plot would have to be loaded, which the single-plot rule
    // forbids. Players may wander into the empty space around p0,0 and stay
    // attached to it — the "skyblock" model this server now implements.

    /// 给 Bedrock 客户端打包一份**只读**世界快照（BACKLOG.md BUG-032）。
    ///
    /// 坐标与 Java 版一致：同一个方块，Bedrock 的 y = MCHPRS 的 y - 64。
    /// 出生点取 Java 新玩家的默认位置 `(128, 128, 128)`。
    /// 只读：这里只调用 `get_block_raw`，不会改世界，也不会给任何客户端发更新。
    ///
    /// `chunk_radius` 是**以区块为单位的半径**，由服务端从 `CONFIG.view_distance` 取来
    /// 并随 `PrivMessage::BedrockSnapshot` 传进来 —— 快照覆盖范围必须和
    /// `ChunkRadiusUpdated` 声明的视距一致，否则远处一片虚空（第 16/26 条）。
    fn build_bedrock_snapshot(
        &self,
        chunk_radius: i32,
        spawn: Option<(f64, f64, f64)>,
    ) -> mchprs_network::bedrock::bridge::PlotSnapshot {
        /// Java `PlayerData::default()` 的位置。
        const SPAWN: (i32, i32, i32) = (128, 128, 128);
        // 登录时**只同步生成一小圈**（BUG-032 第 19 条：一次性 289 区块要卡 tick
        // 205~260ms）。其余由 `stream_bedrock_chunks()` 每 tick 按预算补。
        let chunk_radius = chunk_radius.clamp(
            1,
            mchprs_network::bedrock::chunk::INITIAL_RADIUS_CHUNKS,
        );

        // 有存档就按存档位置出发生成区块；否则用默认出生点。
        // 关键：登录时的第一条 MovePlayer 必须直接落在**玩家真实位置**，
        // 不能"先在默认点、随后再传送一百多格"——后者会让客户端相机/视场角异常
        // （BUG-032 第 31 条）。`spawn` 是 MCHPRS（Java）坐标。
        let (center_x, center_z) = match spawn {
            Some((x, _, z)) => (
                (x.floor() as i32).div_euclid(16),
                (z.floor() as i32).div_euclid(16),
            ),
            None => (SPAWN.0 >> 4, SPAWN.2 >> 4),
        };
        // 计时：这一步是同步跑在 plot 线程上的，`view_distance` 越大越久，
        // 会直接卡住 tick（BUG-032 第 19 条）。有日志才能判断要不要搬到工作线程。
        let started = Instant::now();
        let mut chunks = Vec::new();
        let mut sign_count = 0usize;
        for dx in -chunk_radius..=chunk_radius {
            for dz in -chunk_radius..=chunk_radius {
                let chunk_x = center_x + dx;
                let chunk_z = center_z + dz;
                let signs = self.bedrock_signs_in_chunk((chunk_x, chunk_z));
                sign_count += signs.len();
                if !signs.is_empty() && mchprs_network::bedrock::debug::VERBOSE {
                    println!(
                        "[R7] 快照区块 ({chunk_x}, {chunk_z}) 带 {} 块牌子：{:?}",
                        signs.len(),
                        signs
                            .iter()
                            .map(|s| (s.x, s.y, s.z, s.front[0].clone()))
                            .collect::<Vec<_>>()
                    );
                }
                let payload = mchprs_network::bedrock::chunk::build_chunk_from_world_with_signs(
                    |x, y, z| self.world.get_block_raw(BlockPos::new(x, y, z)),
                    chunk_x,
                    chunk_z,
                    1, // minecraft:plains
                    // BUG-032 第 43 条：登录这一批区块也必须带上告示牌文字。
                    // 以前这里用的是 4 参数的 `build_chunk_from_world`（方块实体为空），
                    // 于是**进 plot 时**（含退出重进）那一圈区块里的牌子全是空白的，
                    // 只有后来流送到的远处区块才有字 —— 真机现象就是"重进后字没了"。
                    &signs,
                );
                if !signs.is_empty() && mchprs_network::bedrock::debug::VERBOSE {
                    // 诊断：负载末尾 48 字节（方块实体 NBT 就在最后），能直接看出
                    // "牌子 NBT 到底有没有写进区块负载"。
                    let tail = &payload[payload.len().saturating_sub(48)..];
                    let hex: String = tail.iter().map(|b| format!("{b:02x}")).collect();
                    println!(
                        "[R7] 快照区块 ({chunk_x}, {chunk_z}) 负载 {} 字节，末尾48字节={hex}",
                        payload.len()
                    );
                }
                chunks.push((chunk_x, chunk_z, payload));
            }
        }

        let side = chunk_radius * 2 + 1;
        info!(
            "Bedrock 快照：{}×{} = {} 区块（含 {} 块牌子），耗时 {:?}",
            side,
            side,
            chunks.len(),
            sign_count,
            started.elapsed()
        );
        mchprs_network::bedrock::bridge::PlotSnapshot {
            spawn: match spawn {
                // 存档位置是玩家的真实坐标，不要再 +0.5（那会让玩家出生时轻微偏移）。
                Some((x, y, z)) => (
                    x as f32,
                    (y - mchprs_network::bedrock::chunk::Y_OFFSET as f64) as f32,
                    z as f32,
                ),
                None => (
                    SPAWN.0 as f32 + 0.5,
                    (SPAWN.1 - mchprs_network::bedrock::chunk::Y_OFFSET) as f32,
                    SPAWN.2 as f32 + 0.5,
                ),
            },
            chunks,
            // 会话线程要用这个半径算登录时 publisher 的 `new_view_radius`：
            // 必须**只覆盖真的发了数据的范围**（第 27 条）。
            chunk_radius,
        }
    }

    /// Remove disconnected players
    fn remove_dc_players(&mut self) {
        let message_sender = &mut self.message_sender;

        let mut disconnected_players = Vec::new();
        self.players.retain(|player| {
            let alive = player.client.alive();
            if !alive {
                player.save();
                message_sender
                    .send(Message::PlayerLeft(player.uuid))
                    .unwrap();
                disconnected_players.push(player.entity_id);
            }
            alive
        });
        let had_disconnections = !disconnected_players.is_empty();
        for entity_id in disconnected_players {
            self.destroy_entity(entity_id);
        }
        // A player leaving is the natural end of a session: p0,0 never unloads
        // (BUG-010), so `Drop` can never persist anything while the server
        // keeps running. Writing the RAM contents back here replaces the old
        // periodic flush entirely (BACKLOG.md BUG-021 rule 7b).
        if had_disconnections {
            self.flush_memories();
        }
    }

    /// Update player view positions and handle packets
    fn update_players(&mut self) {
        for player_idx in 0..self.players.len() {
            if self.players[player_idx].update() {
                self.update_view_pos_for_player(player_idx, false);
            }
        }
        // Handle received packets
        for player_idx in 0..self.players.len() {
            self.handle_packets_for_player(player_idx);
        }
        // 位置刷新完再推一次"谁在哪"（BUG-032 第 22 条完整版）。
        self.send_bedrock_player_sync();
        // 再按预算推一批区块（BUG-032 第 26 条 + 第 19 条）。
        self.stream_bedrock_chunks();
    }

    /// 把"线上还有谁、各自在哪"推给每个 Bedrock 玩家（BUG-032 第 22 条完整版）。
    ///
    /// 推**全量**列表（不含收件人自己）：玩家进出、传送、坠落纠正都靠这一份收敛，
    /// 会话线程只做 diff（生成/移动/销毁）。空列表也推 —— 它表示"别人都不在了"。
    fn send_bedrock_player_sync(&mut self) {
        if !self.players.iter().any(|p| p.client.is_bedrock()) {
            return;
        }
        let snapshot: Vec<mchprs_network::bedrock::bridge::BedrockPlayerInfo> = self
            .players
            .iter()
            .map(|p| mchprs_network::bedrock::bridge::BedrockPlayerInfo {
                uuid: p.uuid,
                username: p.username.clone(),
                // MCHPRS（Java）坐标；`y - 64` 由网络层做。
                position: (p.pos.x, p.pos.y, p.pos.z),
                rotation: (p.yaw, p.pitch),
                on_ground: p.on_ground,
                // A1 主手物品：快捷栏在 Java 背包里是 36..=44。
                held_item: p
                    .inventory
                    .get(p.selected_slot as usize + 36)
                    .and_then(|slot| slot.as_ref())
                    .map(|stack| (stack.item_type.get_id(), stack.count)),
                // A3 姿态：潜行/疾跑各一位。
                sneaking: p.crouching,
                sprinting: p.sprinting,
            })
            .collect();
        for player in &self.players {
            if !player.client.is_bedrock() {
                continue;
            }
            let others: Vec<_> = snapshot
                .iter()
                .filter(|info| info.uuid != player.uuid)
                .cloned()
                .collect();
            player.client.send_players(others);
        }
    }

    /// Bedrock 视距（区块半径）：单一来源仍是 `CONFIG.view_distance`（BUG-032 第 16 条）。
    fn bedrock_view_radius(&self) -> i32 {
        CONFIG.view_distance.clamp(1, 32) as i32
    }

    /// 取一个 Bedrock 区块的负载：命中缓存直接复用，否则现生成并写回缓存。
    ///
    /// 缓存失效由 [`Plot::send_bedrock_block_changes`] 里的 `bedrock_chunk_dirty` 负责。
    fn bedrock_chunk_payload(&mut self, chunk: (i32, i32)) -> Vec<u8> {
        if let Some(cached) = self.bedrock_chunk_cache.get(&chunk) {
            return cached.clone();
        }
        // A2：这一块区块里的告示牌，随负载一起发（否则进 plot 时全是空白牌子）。
        let signs = self.bedrock_signs_in_chunk(chunk);
        if !signs.is_empty() && mchprs_network::bedrock::debug::VERBOSE {
            println!(
                "[R7] 区块 ({}, {}) 负载带 {} 块牌子：{:?}",
                chunk.0,
                chunk.1,
                signs.len(),
                signs
                    .iter()
                    .map(|s| (s.x, s.y, s.z, s.front[0].clone()))
                    .collect::<Vec<_>>()
            );
        }
        let payload = mchprs_network::bedrock::chunk::build_chunk_from_world_with_signs(
            |x, y, z| self.world.get_block_raw(BlockPos::new(x, y, z)),
            chunk.0,
            chunk.1,
            1, // minecraft:plains
            &signs,
        );
        self.bedrock_chunk_cache.insert(chunk, payload.clone());
        payload
    }

    /// 收集某个 Bedrock 区块里所有告示牌的四行文字（A2）。
    ///
    /// 行文本在 MCHPRS 里是 **JSON 文本组件**（`{"text":"..."}`），Bedrock 要的是
    /// 纯字符串，所以这里做一次降级（`sign_rows_to_legacy`）。
    fn bedrock_signs_in_chunk(
        &self,
        chunk: (i32, i32),
    ) -> Vec<mchprs_network::bedrock::bridge::BedrockSign> {
        let mut out = Vec::new();
        let chunk_index = self.world.get_chunk_index_for_chunk(chunk.0, chunk.1);
        let Some(chunk_ref) = self.world.chunks.get(chunk_index) else {
            return out;
        };
        for (pos, block_entity) in &chunk_ref.block_entities {
            let BlockEntity::Sign(sign) = block_entity else {
                continue;
            };
            // 局部坐标 → 世界坐标（局部是 `x & 0xF` / `z & 0xF`）。
            let x = chunk.0 * 16 + pos.x;
            let z = chunk.1 * 16 + pos.z;
            out.push(mchprs_network::bedrock::bridge::BedrockSign {
                x,
                y: pos.y,
                z,
                front: sign_rows_to_legacy(&sign.front_rows),
                back: sign_rows_to_legacy(&sign.back_rows),
            });
        }
        out
    }

    /// 一块告示牌的**实时更新**（A2）：玩家改字/放牌后立刻推给每个 Bedrock 客户端。
    ///
    /// 同时让这一个区块的缓存失效 —— 不然之后进 plot 的人（或流送重发）
    /// 拿到的是"改字之前"缓存下来的旧负载，牌子上还是旧文字。
    fn push_bedrock_sign(&mut self, pos: BlockPos) {
        let sign = match self.world.get_block_entity(pos) {
            Some(BlockEntity::Sign(sign)) => (**sign).clone(),
            _ => return,
        };
        self.bedrock_chunk_dirty
            .insert((pos.x.div_euclid(16), pos.z.div_euclid(16)));
        let update = mchprs_network::bedrock::bridge::BedrockSign {
            x: pos.x,
            y: pos.y,
            z: pos.z,
            front: sign_rows_to_legacy(&sign.front_rows),
            back: sign_rows_to_legacy(&sign.back_rows),
        };
        // 诊断（BUG-032 第 43 条真机第二轮）：真机上"字不显示"，必须看清
        // ①方块映射到哪个 Bedrock 运行时 id（是不是牌子）②发下去的文字到底是什么。
        let java_state = self.world.get_block_raw(pos);
        let bedrock_runtime = mchprs_network::bedrock::bedrock_block_state(java_state);
        if mchprs_network::bedrock::debug::VERBOSE {
            println!(
                "[R7] 告示牌 ({}, {}, {}) java_state={java_state} bedrock_runtime={bedrock_runtime} 正面={:?} 反面={:?}",
                pos.x, pos.y, pos.z, update.front, update.back
            );
        }
        let mut sent = 0;
        for player in &self.players {
            if player.client.is_bedrock() {
                player.client.send_sign(update.clone());
                sent += 1;
            }
        }
        if sent > 0 {
            println!(
                "[R7] 告示牌 ({}, {}, {}) 已推给 {sent} 个 Bedrock 客户端",
                pos.x, pos.y, pos.z
            );
        }
    }

    /// 每 tick 给每个 Bedrock 玩家推一批区块（BUG-032 第 26 条增量区块 +
    /// 第 19 条把快照生成移出"一次性"路径）。
    ///
    /// 规则：
    /// - 每个玩家每 tick 最多 `STREAM_BUDGET_CHUNKS` 个、且不超过 `STREAM_BUDGET_MS`；
    /// - 玩家中心区块变了 → 重算方形窗口、publisher 跟着中心走；
    /// - 方块变化的区块（dirty）→ **只让缓存失效**，不重发整块
    ///   （重发会让客户端重建区块，表现为"放一个方块整片区块闪一下"）；
    /// - publisher 半径只在"真的发满了更大一圈"时增长（第 27 条的教训）。
    fn stream_bedrock_chunks(&mut self) {
        use mchprs_network::bedrock::chunk as netchunk;
        // 整轮流送的耗时（第 19 条的验收指标：单 tick 不应超过 20ms）。
        let timeline = Instant::now();

        // 1) 变过方块的区块先让缓存失效（不论有没有 Bedrock 玩家 —— 否则缓存会留旧数据）。
        let dirty: Vec<(i32, i32)> = self.bedrock_chunk_dirty.drain().collect();
        for chunk in &dirty {
            self.bedrock_chunk_cache.remove(chunk);
        }

        // 2) 收集 Bedrock 玩家（先拷贝出来，避免后面同时借用 self.world / streams）。
        let players: Vec<(usize, u128, (f64, f64, f64))> = self
            .players
            .iter()
            .enumerate()
            .filter(|(_, p)| p.client.is_bedrock())
            .map(|(idx, p)| (idx, p.uuid, (p.pos.x, p.pos.y, p.pos.z)))
            .collect();
        if players.is_empty() {
            self.bedrock_streams.clear();
            return;
        }

        let radius = self.bedrock_view_radius();
        let initial = netchunk::INITIAL_RADIUS_CHUNKS;
        let window = ((radius * 2 + 1) * (radius * 2 + 1)) as usize;

        for (idx, uuid, pos) in players {
            let center = (
                (pos.0.floor() as i32).div_euclid(16),
                (pos.2.floor() as i32).div_euclid(16),
            );
            // 把状态取出来单独操作：`bedrock_chunk_payload()` 要 `&mut self`。
            let mut stream = self.bedrock_streams.remove(&uuid).unwrap_or_else(|| {
                // 登录时同步发的那一小圈已经在这个客户端手里。
                let mut sent = HashSet::new();
                for dx in -initial..=initial {
                    for dz in -initial..=initial {
                        sent.insert((center.0 + dx, center.1 + dz));
                    }
                }
                BedrockChunkStream {
                    center,
                    sent,
                    pending: VecDeque::new(),
                    // publisher 在登录时就报满了（见字段注释），这里跟上；
                    // 只有中心变化才需要重发 publisher。
                    published_radius: radius,
                    started: Instant::now(),
                }
            });

            let mut publisher: Option<(i32, i32, i32, u32)> = None;

            // 中心变了：窗口平移（publisher 中心跟着走），**队列整份重排**。
            //
            // `sent` 里留着的算"客户端已有"；其余全部重新排队。这里必须重建而不是沿用旧队列：
            // 玩家边走边等流送时，旧队列里可能全是上一个窗口的坐标，新窗口的环一直排不上，
            // 真机表现就是"走到某处那片区块永远不来"。
            if stream.center != center {
                stream.center = center;
                stream.started = Instant::now();
                stream.sent.retain(|(cx, cz)| {
                    (cx - center.0).abs() <= radius && (cz - center.1).abs() <= radius
                });
                stream.pending = netchunk::pending_chunks(center, radius, &stream.sent).into();
                publisher = Some((
                    pos.0.floor() as i32,
                    (pos.1 - mchprs_network::bedrock::chunk::Y_OFFSET as f64) as i32,
                    pos.2.floor() as i32,
                    radius_blocks(stream.published_radius),
                ));
            }

            // 3) 变过方块的区块**只失效缓存，不重发整块**。
            //
            // 重发整块 `LevelChunk` 会让客户端重建整个区块 —— 放一个方块整片区块闪一下
            // （2026-10-31 真机实测）。方块本身已经由 `send_bedrock_block_changes()` 的
            // `UpdateBlock` 即时改掉了，所以这里只需要保证"下次重建时数据是新的"。
            let mut batch: Vec<(i32, i32, Vec<u8>, u32)> = Vec::new();

            // 4) 队列空了但窗口还没铺满 → 重新排一份（由近到远）。
            if stream.pending.is_empty() && stream.sent.len() < window {
                stream.pending = netchunk::pending_chunks(center, radius, &stream.sent).into();
            }

            // 5) 按预算生成并装批。
            let started = Instant::now();
            let budget = Duration::from_millis(netchunk::STREAM_BUDGET_MS);
            while batch.len() < netchunk::STREAM_BUDGET_CHUNKS && started.elapsed() < budget {
                let Some(chunk) = stream.pending.pop_front() else {
                    break;
                };
                if stream.sent.contains(&chunk) {
                    continue;
                }
                let payload = self.bedrock_chunk_payload(chunk);
                batch.push((chunk.0, chunk.1, payload, netchunk::BEDROCK_SECTION_COUNT as u32));
                stream.sent.insert(chunk);
            }

            // 6) 覆盖情况**只用于日志**：publisher 半径恒为视距（见字段注释），
            //    不能再"随发随长" —— 那样先发的区块会被客户端丢掉且不会补发。
            let covered = netchunk::covered_radius(center, radius, &stream.sent);

            let sending = !batch.is_empty();
            if sending || publisher.is_some() {
                if sending {
                    // 铺满这条日志是"第 19 条真的解决了吗"的验收证据：
                    // 一次性生成是 205~260ms，现在是每 tick 一小批、总耗时分成 N 个 tick。
                    if stream.pending.is_empty() {
                        info!(
                            "Bedrock 区块流送完成：{} 个区块，覆盖半径 {} 区块，耗时 {:?}（每 tick 预算 {} 个 / {}ms）",
                            stream.sent.len(),
                            covered,
                            stream.started.elapsed(),
                            netchunk::STREAM_BUDGET_CHUNKS,
                            netchunk::STREAM_BUDGET_MS
                        );
                    }
                    debug!(
                        "Bedrock 流送：{} 个区块（剩余 {}，已发 {}/{}）",
                        batch.len(),
                        stream.pending.len(),
                        stream.sent.len(),
                        window
                    );
                }
                self.bedrock_streams.insert(uuid, stream);
                self.players[idx]
                    .client
                    .send_chunks(mchprs_network::bedrock::bridge::BedrockChunkBatch {
                        chunks: batch,
                        publisher,
                    });
            } else {
                self.bedrock_streams.insert(uuid, stream);
            }
        }

        // 走掉的人不再需要流送状态。
        let alive: HashSet<u128> = self.players.iter().map(|p| p.uuid).collect();
        self.bedrock_streams.retain(|uuid, _| alive.contains(uuid));

        // 一次流送的总耗时（含区块生成 + 队列维护）。超过 20ms 就是 tick 卡顿的信号，
        // 打出来才有数据判断要不要继续拆（第 19 条的验收指标）。
        let elapsed = timeline.elapsed();
        if elapsed > Duration::from_millis(20) {
            warn!("Bedrock 流送这一轮耗时 {elapsed:?}（超过 20ms，检查预算）");
        }
    }

    fn update(&mut self) {
        self.handle_messages();

        // Tick unconditionally (BACKLOG.md BUG-009). Upstream gated this whole
        // body behind `!self.players.is_empty()`, which froze the redstone
        // simulation, the redpiler flush and every automation sample on a plot
        // with no players — headless operation was impossible. p0,0 is now
        // permanently resident (BUG-010), so it must always tick.
        {
            self.timings.set_ticking(true);
            let now = Instant::now();
            // `last_player_time` keeps its original meaning: the last time a
            // player was actually present. It drives the idle-unload rule
            // below, which must not be refreshed by a headless tick.
            if !self.players.is_empty() {
                self.last_player_time = now;
            }

            let world_send_interval =
                Duration::try_from_secs_f64(1.0 / f64::from(self.world.world_send_rate.0))
                    .unwrap_or(Duration::MAX);

            let batch_interval = world_send_interval.min(Duration::from_millis(50));
            let max_batch_size = match self.last_nspt {
                Some(Duration::ZERO) | None => 1,
                Some(last_nspt) => {
                    let ticks_fit = batch_interval.as_nanos() / last_nspt.as_nanos();
                    // A tick previously took longer than the batch interval.
                    // Run at least one just so we're not stuck doing nothing
                    ticks_fit.clamp(1, u128::from(MAX_BATCH_SIZE)) as u32
                }
            };

            let batch_size = match self.tps {
                Tps::Limited(tps) if tps > 0.0 => {
                    // Very high rates can round the interval to zero and cause division by zero.
                    let dur_per_tick = Duration::try_from_secs_f64(1.0 / f64::from(tps))
                        .unwrap_or(Duration::MAX)
                        .max(Duration::from_nanos(1));
                    self.lag_time += now - self.last_update_time;
                    let ticks_behind = self.lag_time.as_nanos() / dur_per_tick.as_nanos();
                    self.lag_time = Duration::from_nanos(
                        (self.lag_time.as_nanos() % dur_per_tick.as_nanos()) as u64,
                    );
                    ticks_behind.min(u128::from(max_batch_size)) as u32
                }
                Tps::Unlimited => max_batch_size,
                _ => 0,
            };

            self.last_update_time = now;
            if batch_size != 0 {
                let mut ticks_completed = batch_size;
                if self.redpiler.is_active() {
                    // `exact` samples the IO groups after every single tick, so
                    // nothing is merged or dropped; `performance` samples once
                    // at the end of the batch. ROM / RAM always see every tick,
                    // otherwise a lagging batch merges clock edges and loses
                    // RAM writes (BACKLOG.md BUG-027 / §7.5).
                    let exact = CONFIG.io_mode() == IoMode::Exact;
                    for _ in 0..batch_size {
                        // The tick counter and the reservations advance tick by
                        // tick in both modes; only the sampling is batched in
                        // `performance`.
                        self.step_tick(exact);
                    }
                    self.redpiler.flush(&mut self.world);
                    // Flush-time IO events (`EVENT`) were removed in
                    // 0.5.2-alpha; drain them so the backend buffer stays
                    // empty.
                    self.redpiler.take_io_events();
                    if !exact {
                        self.sample_io_groups();
                    }
                } else {
                    for i in 0..batch_size {
                        self.tick();
                        if now.elapsed() > Duration::from_millis(200) {
                            ticks_completed = i + 1;
                            break;
                        }
                    }
                }
                self.last_nspt = Some(self.last_update_time.elapsed() / ticks_completed);
            }

            let now = Instant::now();
            let time_since_last_world_send = now - self.last_world_send_time;
            if time_since_last_world_send > world_send_interval {
                self.last_world_send_time = now;
                // 这里原来写的是 `self.world.flush_block_changes()` —— 那是空函数，
                // 于是定期发包这条路根本没在工作：方块变化只堆在 `world.block_changes` 里，
                // 要等某个别的事件顺手调 `publish_world` 才发出去（实测点按钮滞后十几秒、
                // 红石灯不亮）。正确做法是 Java 批量包 + Bedrock 逐条都发（BUG-032 第 34 条）。
                self.world.flush_block_changes();
                self.send_bedrock_block_changes();
            }
        }

        // Unload the plot after 600 seconds without players unless it is marked
        // as always running. This is now an independent check rather than the
        // `else` of the tick branch. p0,0 is always running, so it never
        // unloads (BACKLOG.md BUG-010).
        if self.players.is_empty() && !self.always_running {
            if self.last_player_time.elapsed().as_secs() > 600 {
                self.running = false;
                self.timings.stop();
            }
        }

        self.update_players();

        // Handle commands before removing players just in case they ran a command before leaving
        self.handle_commands();

        self.remove_dc_players();
    }

    fn create_async_rt() -> Runtime {
        Runtime::new().unwrap()
    }

    fn generate_chunk(layers: i32, x: i32, z: i32) -> Chunk {
        let mut chunk = Chunk::empty(x, z, PLOT_SECTIONS);

        for ry in 0..layers {
            for rx in 0..16 {
                for rz in 0..16 {
                    let block_x = (x << 4) | rx;
                    let block_z = (z << 4) | rz;

                    let block = if block_x % PLOT_BLOCK_WIDTH == 0
                        || block_z % PLOT_BLOCK_WIDTH == 0
                        || (block_x + 1) % PLOT_BLOCK_WIDTH == 0
                        || (block_z + 1) % PLOT_BLOCK_WIDTH == 0
                    {
                        Block::StoneBricks
                    } else {
                        Block::Sandstone
                    };
                    chunk.set_block(rx as u32, ry as u32, rz as u32, block.get_id());
                }
            }
        }
        chunk
    }

    fn from_data(
        plot_data: PlotData,
        x: i32,
        z: i32,
        rx: BusReader<BroadcastMessage>,
        tx: Sender<Message>,
        priv_rx: Receiver<PrivMessage>,
        always_running: bool,
    ) -> Plot {
        let chunk_x_offset = x << PLOT_SCALE;
        let chunk_z_offset = z << PLOT_SCALE;
        let chunks: Vec<Chunk> = plot_data
            .chunk_data
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                c.load(
                    chunk_x_offset + i as i32 / PLOT_WIDTH,
                    chunk_z_offset + i as i32 % PLOT_WIDTH,
                )
            })
            .collect();
        if chunks.len() != NUM_CHUNKS {
            error!("This plot has the wrong number of chunks!");
            let possible_scale = (chunks.len() as f64).sqrt().log2();
            error!("Note: it most likely came from a server running plot scale {}, this server is running a plot scale of {}", possible_scale, PLOT_SCALE);
        }
        let world = PlotWorld {
            x,
            z,
            chunks,
            to_be_ticked: plot_data.pending_ticks,
            packet_senders: Vec::new(),
            world_send_rate: plot_data.world_send_rate,
            pending_block_entities: FxHashMap::default(),
            pending_sounds: FxHashMap::default(),
            block_changes: Vec::new(),
            pending_bedrock_signs: Vec::new(),
        };
        let tps = plot_data.tps;
        Plot {
            last_player_time: Instant::now(),
            last_update_time: Instant::now(),
            last_world_send_time: Instant::now(),
            lag_time: Duration::new(0, 0),
            sleep_time: sleep_time_for_tps(tps),
            last_nspt: None,
            message_receiver: rx,
            message_sender: tx,
            priv_message_receiver: priv_rx,
            players: Vec::new(),
            locked_players: HashSet::new(),
            running: true,
            tps,
            frozen_rtps: None,
            coord: Default::default(),
            coord_dirty: false,
            always_running,
            redpiler: Default::default(),
            memories: {
                let report = memory::load_all(Path::new(memory::MEMORY_DIR));
                for message in &report.rejected {
                    warn!("{}", message);
                }
                report.memories
            },
            timings: TimingsMonitor::new(tps),
            owner: database::get_plot_owner(x, z).map(|s| s.parse::<HyphenatedUUID>().unwrap().0),
            async_rt: Plot::create_async_rt(),
            scoreboard: Default::default(),
            // `io.txt` is parsed once, at plot start; the groups are validated
            // against the graph after every successful compile.
            io: io::IoTable::load(Path::new(IO_FILE)),
            tick: 0,
            pout_bindings: FxHashMap::default(),
            reservations: Vec::new(),
            seq_mode: false,
            exclusive: None,
            bedrock_streams: HashMap::new(),
            bedrock_chunk_cache: HashMap::new(),
            bedrock_chunk_dirty: HashSet::new(),
            world,
        }
    }

    fn load(
        x: i32,
        z: i32,
        rx: BusReader<BroadcastMessage>,
        tx: Sender<Message>,
        priv_rx: Receiver<PrivMessage>,
        always_running: bool,
    ) -> Result<Plot, (Error, Sender<Message>)> {
        let plot_path = format!("./world/plots/p{},{}", x, z);
        Ok(if Path::new(&plot_path).exists() {
            match data::load_plot(plot_path) {
                Ok(data) => Plot::from_data(data, x, z, rx, tx, priv_rx, always_running),
                Err(err) => {
                    return Result::Err((
                        err.context(format!("error loading plot {},{}", x, z)),
                        tx,
                    ))
                }
            }
        } else {
            Plot::from_data(data::empty_plot(), x, z, rx, tx, priv_rx, always_running)
        })
    }

    fn save(&mut self) {
        let world = &mut self.world;
        let chunk_data: Vec<ChunkData> = world.chunks.iter_mut().map(ChunkData::new).collect();
        let data = PlotData {
            tps: self.tps,
            world_send_rate: world.world_send_rate,
            chunk_data,
            pending_ticks: world.to_be_ticked.clone(),
        };
        data.save_to_file(format!("./world/plots/p{},{}", world.x, world.z))
            .unwrap();

        self.reset_timings();
    }

    /// Saves the plot on a crash path and returns the path that was written
    /// (or a marker when saving itself failed).
    ///
    /// Saving from inside a panic handler must not panic again: a second panic
    /// while unwinding aborts the process without any diagnostics, so failures
    /// are swallowed and reported by the caller instead.
    fn crash_save(&mut self) -> String {
        let path = format!("./world/plots/p{},{}", self.world.x, self.world.z);
        match std::panic::catch_unwind(AssertUnwindSafe(|| self.save())) {
            Ok(()) => path,
            Err(_) => format!("{} (SAVE FAILED)", path),
        }
    }

    fn run(&mut self, initial_player: Option<Player>) {
        let _guard = self.async_rt.enter();

        if let Some(player) = initial_player {
            self.enter_plot(player);
        }

        while self.running {
            // Fast path, for super high RTPS
            if self.sleep_time <= Duration::from_millis(5) && !self.players.is_empty() {
                self.update();
                if self.tps != Tps::Unlimited {
                    thread::yield_now();
                }
                continue;
            }

            let before = Instant::now();
            self.update();
            let delta = Instant::now().duration_since(before);

            if delta < self.sleep_time {
                let sleep_time = self.sleep_time - delta;
                thread::sleep(sleep_time);
            } else {
                thread::yield_now();
            }
        }

        self.save();
    }

    /// This function is used in case of an error. It will try to send the player to spawn if this
    /// isn't already a spawn plot.
    fn send_player_away(plot_x: i32, plot_z: i32, player: &mut Player) {
        let (px, pz) = if plot_x == 0 && plot_z == 0 {
            // Can't send players to spawn if spawn crashed!
            Plot::get_center(1, 0)
        } else {
            Plot::get_center(0, 0)
        };
        player.teleport(PlayerPos::new(px, 64.0, pz));
    }

    pub fn load_and_run(
        x: i32,
        z: i32,
        rx: BusReader<BroadcastMessage>,
        tx: Sender<Message>,
        priv_rx: Receiver<PrivMessage>,
        always_running: bool,
        initial_player: Option<Player>,
    ) {
        thread::Builder::new()
            .name(format!("p{},{}", x, z))
            .spawn(
                move || match Plot::load(x, z, rx, tx, priv_rx, always_running) {
                    Ok(mut plot) => plot.run(initial_player),
                    Err((err, tx)) => {
                        if let Some(mut player) = initial_player {
                            player.send_error_message("There was an error loading that plot.");
                            Plot::send_player_away(x, z, &mut player);
                            tx.send(Message::PlayerLeavePlot(player)).unwrap();
                        }
                        tx.send(Message::PlotUnload(x, z)).unwrap();
                        panic!("{err:?}");
                    }
                },
            )
            .unwrap();
    }
}

/// 把 MCHPRS 的 46 格背包映射成 Bedrock 的三份容器（BUG-032 第 39 条）。
///
/// Java 槽位布局（`crates/core/src/player.rs` 建的是 `vec![None; 46]`）：
/// `0..=4` 合成/特殊、`5..=8` 护甲（头/胸/腿/靴）、`9..=35` 主背包 27 格、
/// `36..=44` 快捷栏 9 格、`45` 副手。
/// Bedrock 的 `INVENTORY` 容器是 `0..=8` 快捷栏 + `9..=35` 主背包，顺序与 Java 相同。
fn bedrock_inventory_snapshot(
    player: &Player,
) -> mchprs_network::bedrock::bridge::BedrockInventorySnapshot {
    let item = |slot: usize| {
        player
            .inventory
            .get(slot)
            .and_then(|s| s.as_ref())
            .map(|stack| (stack.item_type.get_id(), stack.count))
    };
    let mut main = Vec::with_capacity(36);
    for slot in 0..9 {
        main.push(item(36 + slot)); // 快捷栏
    }
    for slot in 9..36 {
        main.push(item(slot)); // 主背包
    }
    let armor: Vec<Option<(u32, u8)>> = (5..9).map(item).collect();
    mchprs_network::bedrock::bridge::BedrockInventorySnapshot {
        main,
        armor,
        offhand: item(45),
        selected_slot: player.selected_slot.min(8) as u8,
    }
}

/// 把 MCHPRS 存的**四行 JSON 文本组件**降级成 Bedrock 要的纯字符串（BUG-032 A2）。
fn sign_rows_to_legacy(rows: &[String; 4]) -> [String; 4] {
    [
        sign_row_to_legacy(&rows[0]),
        sign_row_to_legacy(&rows[1]),
        sign_row_to_legacy(&rows[2]),
        sign_row_to_legacy(&rows[3]),
    ]
}

/// 单行：`{"text":"..."}` → `...`（颜色/格式位暂不降级，见 BACKLOG §9.21）。
///
/// 对应 Geyser `MessageTranslator.convertMessageLenient` 的最小可用子集：
/// 纯字符串直接取；对象取 `text` 再递归拼 `extra`；数组按顺序拼接。
/// 解析失败时**原样返回**输入（宁可显示一段 JSON，也不要整行消失）。
fn sign_row_to_legacy(row: &str) -> String {
    fn walk(value: &serde_json::Value, out: &mut String) {
        match value {
            serde_json::Value::String(text) => out.push_str(text),
            serde_json::Value::Array(items) => {
                for item in items {
                    walk(item, out);
                }
            }
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::String(text)) = map.get("text") {
                    out.push_str(text);
                }
                if let Some(extra) = map.get("extra") {
                    walk(extra, out);
                }
            }
            _ => {}
        }
    }
    match serde_json::from_str::<serde_json::Value>(row) {
        Ok(value) => {
            let mut out = String::new();
            walk(&value, &mut out);
            out
        }
        Err(_) => row.to_string(),
    }
}

/// Extracts a human-readable message from a panic payload.
fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

impl Drop for Plot {
    fn drop(&mut self) {
        // Last chance to persist writable memories (BACKLOG.md BUG-021).
        self.flush_memories();
        if !self.players.is_empty() {
            for player in &mut self.players {
                player.save(); // just in case

                let world = &self.world;
                Plot::send_player_away(world.x, world.z, player);

                player.send_error_message("The plot you were previously in has crashed!");
            }

            while !self.players.is_empty() {
                let uuid = self.players[0].uuid;
                let player = self.leave_plot(uuid);
                self.message_sender
                    .send(Message::PlayerLeavePlot(player))
                    .unwrap();
            }
        }
        let world = &self.world;
        self.message_sender
            .send(Message::PlotUnload(world.x, world.z))
            .unwrap();

        self.reset_redpiler();
        self.world
            .chunks
            .iter_mut()
            .for_each(|chunk| chunk.compress());
        self.save();
    }
}

#[test]
fn chunk_save_and_load_test() {
    let mut chunk = Chunk::empty(1, 1, PLOT_SECTIONS);
    chunk.set_block(13, 63, 12, 332);
    chunk.set_block(13, 62, 12, 331);
    let chunk_data = ChunkData::new(&mut chunk);
    let loaded_chunk = chunk_data.load(1, 1);
    assert_eq!(loaded_chunk.get_block(13, 63, 12), 332);
    assert_eq!(loaded_chunk.get_block(13, 62, 12), 331);
    assert_eq!(loaded_chunk.get_block(13, 64, 12), 0);
}

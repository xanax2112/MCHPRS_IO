# 基岩版 spike 验收记录（2026-10-29）

> 本文件由 spike 工程拷贝进仓库，作为 **BUG-032** 的验收证据留存。
> 原始工程在仓库外（Windows 侧 `D:\computer\mchprs\bedrock\bedrock\spike`），
> 正式代码已并入 `crates/network/src/bedrock/`，说明见 `docs/bedrock-integration.md`。
> 文中 `~/bedrock/...` 是 spike 侧的路径写法，不要按它去找本仓库的文件。

---
# 阶段一（Bedrock spike）进度报告

> 更新：2026-10-29 22:2x
> 代码：`~/bedrock/spike/`（Rust，独立 crate，未触碰主仓库）
> 上游：`~/bedrock/bedrock-rs`（本地副本，打过 5 处补丁，见 `HANDOFF.md` §8）
>
> ⚠️ **历史文档**（2026-10-29 的 Termux 时期记录）。`~/bedrock/*` 这些路径已作废：
> 2026-11-01 工作区已拍平，仓库在 `D:\computer\mchprs\bedrock\MCHPRS_IO`，
> 取证材料在 `D:\computer\mchprs\bedrock\refs\`。看当前状态请读
> `docs/BEDROCK-AGENT-NOTES.md` 与 `BACKLOG.md` 的 BUG-032 / §9.x。

---

## 1. 结论速览

| 里程碑 | 状态 | 证据 |
|---|---|---|
| A1 源码落地 | ✅ | 1035 文件 sha256 清单 |
| A2 编译闸门 | ✅ | `cargo build` 通过（aarch64，需 2 处上游补丁） |
| A3 端口就绪 | ✅ | UDP 19132 监听 |
| **B1 协议号** | ✅ **实测 766** | 客户端 `RequestNetworkSettings` 报 766 |
| B2 加密登录 | ✅ | 握手 + AES 加密后正常收发包 |
| B3 资源包 | ✅ | 客户端 `ResourcePackStackFinished` |
| **B4 进入世界** | ✅ | 客户端离开加载界面，能看到天空与云 |
| C1 区块半径 | ✅ | 客户端 `RequestChunkRadius(12)`，回 8 |
| **C2 看到方块** | ✅ **已解决** | 根因是 `NetworkChunkPublisherUpdate` 半径单位是**方块**，见 §0 |
| C3 多区块 | ✅ | 已发 17×17=289 区块 |
| C4 30 秒稳定 | ✅ | 多次会话保持连接，客户端主动退出前无掉线 |
| D1 收输入 | ✅ | 收到 `MovePlayer`/`PlayerAuthInput` 持续上报（含坐标） |
| **D0 玩家状态** | ✅ **已解决** | 空氧气条 + 移动变慢的根因是缺 `BREATHING` 标志位，见 §0.2 |
| **D2/D3 方块交互** | ✅ **已通** | 客户端点方块 → 服务端收到 `PlayerAuthInput.player_block_actions`，含 `action_type=StartDestroyBlock`、`pos=(0,80,2)`、`facing=5`；人类实测“点方块有反应” |
| D4 connect 序列齐备 | ✅ | 8 个剩余包已补齐并与 codec 逐字节一致，见 §0.3 |

---

## 0. C2（方块不渲染）已解决 —— 2026-10-29

**根因**：`NetworkChunkPublisherUpdate.new_view_radius` 的单位是**方块**，不是区块。
我们原先发 `4`（≈0.25 个区块），客户端认为没有可渲染区域，于是方块有碰撞、有选中框、但完全不出几何体。

**正确做法**（照 Geyser `ChunkUtils.updateChunkPosition`）：
```java
radius = ceil((serverRenderDistance + 1) * sqrt(2)) << 4
```
即 8 格视距 → `ceil(9*1.4142)=13` → `13*16 = 208` 个方块。

**同时修正**：`ChunkRadiusUpdated` 由 4 改为 8；区块数量随之 81 → 289。

**排除掉的错误方向（别再试）**：
1. ❌ “block state id 与 1.21.51 不匹配” —— 用 Geyser 内置的 BDS 原始调色板 `bedrock/block_palette.1_21_50.nbt` 验证：stone=2403、air=11996、glass=11468，与 minecraft-data 完全一致；当时看到的“半砖/石墙/玻璃”恰好对应调色板里 2404/2405(slab)、100/1000(wall)、2400(glass)。
2. ❌ 子区块版本 8 vs 9 —— 两种都能工作。最终采用**版本 9 + subchunk index**（负段用补码，-4→0xFC），与 Geyser `GeyserChunkSection` 一致。
3. ❌ 发送时序（spawn 前/后）—— 改成 `SetLocalPlayerAsInitialized` 之后发送仍然必须配合正确的 publisher 半径。

**交叉验证方法（可复用）**：直接调用 Geyser jar 里的序列化类导出参考字节：
`org.geysermc.geyser.level.chunk.GeyserChunkSection.writeToNetwork(...)`。
当时我们生成的整块负载与 Geyser 等价输出**逐字节一致**（111 字节），证明数据层无问题，问题在会话参数。

---

## 0.2 「空氧气条 + 移动变慢」已解决 —— 2026-10-29

**现象**（人类描述）：能进世界、方块正常、天空正常、**没有水底滤镜**，但底部氧气条一直在且不下降，走路明显比 Geyser 慢。

**根因（两条，都靠参考实现定死，没有试错）**：

1. 玩家实体元数据里的 `FLAGS`（元数据 id 0，LONG）缺少 **`BREATHING`（第 36 位）**。
   Geyser `SessionPlayerEntity.setAirSupply()` 的源码注释原话：
   > *"Seemingly required to be sent as of Bedrock 1.21. Otherwise, bubbles will appear as empty"*
   只发 `AIR_SUPPLY=300 / AIR_SUPPLY_MAX=300` 是不够的，两者相等时客户端仍然因为“不在呼吸”而画空氧气条。
2. bedrock-rs 把 LONG 型元数据按 **zigzag** 编码，而 Bedrock 线上是**无符号** varint → 整数值翻倍、标志位全部错位（PATCH-005）。

**修法**：出生后下发与 Geyser `Entity.initializeMetadata() + SessionPlayerEntity.setAirSupply(getMaxAir())` 等价的元数据：

| id | 类型 | 值 | 字段 |
|---|---|---|---|
| 38 | FLOAT | 1.0 | SCALE |
| 3 | BYTE | 0 | COLOR |
| 42 | SHORT | 300 | AIR_SUPPLY_MAX |
| 7 | SHORT | 300 | AIR_SUPPLY |
| 54 | FLOAT | 1.8 | HEIGHT |
| 53 | FLOAT | 0.6 | WIDTH |
| 0 | LONG | 位集 | FLAGS |
| 92 | LONG | 位集 | FLAGS_2 |

`FLAGS` 置位：`CAN_SHOW_NAME(15) | SILENT(18) | CAN_CLIMB(20) | BREATHING(36) | HAS_COLLISION(49) | HAS_GRAVITY(50)`；
`FLAGS_2` 置位：`HIDDEN_WHEN_INVISIBLE(26)`。

**位号怎么来的**（可复用方法，别再数枚举序号）：写 `refs/ProbeFlagBits.java`，用 Geyser jar 里的 Cloudburst `Bedrock_v766.CODEC` 把**单个** `EntityFlag` 序列化成字节，再按 varint 还原置位位置。codec 是权威，枚举序号不是。

**验收**：`bedrock-spike --selftest=meta` 打出的字节与 codec 参考输出**逐字节一致**：

```
010826030000803f0300002a012c0107012c0136036666e63f35039a99193f
00078080d280808280035c0780808020000000
```

人类实测：氧气条消失、移动恢复正常。

**排除掉的方向（别再试）**：
- ❌ 只补 `AIR_SUPPLY/AIR_SUPPLY_MAX` —— 实测“没变化”。
- ❌ 以为客户端认为身体在水里 —— 没有水底滤镜，且 `AirSupply == AirSupplyMax` 时气泡条不该出现，现象与“溺水”无关。
- ❌ 以为要补 `Respawn(SERVER_READY)` —— 抓包里的那条是 Java 后端“设置玩家位置”被翻译出来的，不是 connect 流程的一环。

---

## 0.3 connect() 剩余包已补齐 —— 2026-10-29

方法同 §0.2：写 `refs/ProbeSpawnPackets.java`，用 Cloudburst `Bedrock_v766.CODEC` 序列化出权威字节，Rust 侧用 `--selftest=spawn` 对拍，**8 个包全部逐字节一致**：

| 包 | 参考字节（十六进制） | 我们的取值 |
|---|---|---|
| `SetDifficulty` | `01` | difficulty=1 |
| `PlayerHotbar` | `000001` | slot=0, container=0, select=true |
| `TrimData` | `0000` | 空表（合法） |
| `UnlockedRecipes` | `0100000000` | action=INITIALLY_UNLOCKED(1)，空表 |
| `CraftingData` | `0000000001` | 4 个空表 + clean_recipes=true |
| `ItemComponent` | `00` | 空表（纯原版无自定义物品） |
| `CreativeContent` | `00` | 空表（创造背包列表待补数据） |
| `AvailableCommands` | `0000000000000000` | 8 个空表 |

**实机证据**（`logs/run-windows-26.log`）：登录→出生全程无报错，发送顺序与 Geyser 一致：

```
PlayerList(REMOVE) → DimensionData → StartGame → ChunkRadiusUpdated → PlayStatus(PlayerSpawn)
→ UpdateAttributes → UpdateAdventureSettings → UpdateAbilities → SetActorData → MovePlayer
→ SetDifficulty → PlayerHotbar → TrimData → CraftingData → AvailableCommands → UnlockedRecipes
→ ItemComponent → BiomeDefinitionList → AvailableActorIdentifiers → CameraPresets
→ CreativeContent → GameRulesChanged → NetworkChunkPublisherUpdate → 区块
```

**S3 证据**（同一次会话，人类在游戏里点了一下脚下的方块）：

```
[S3] << 方块交互 action=StartDestroyBlock pos=Some(BlockPos { x: 0, y: 80, z: 2 }) facing=Some(5)
```

即：方块交互事件带着**方块坐标和面**到达了服务端，阶段一 S3 的协议前提成立。

---

## 0.4 创造模式 + 飞行已对齐 —— 2026-10-29

MCHPRS 只支持创造/旁观，所以 spike 也必须跑在创造模式。改动三处：

1. `LevelSettings.game_type` 与 `StartGame.player_game_type`：`Survival` → `Creative`。
2. `UpdateAbilities` 的 `ability_values`：`0x000F` → **`0x0FBF`**。
3. `UpdateAbilities.abilities_set` 保持 `0x0007_FFFF`（后端声明“支持全部能力”）。

**位号来源**：`refs/ProbeCreativeAbilities.java` 用 Cloudburst v766 codec 逐个能力单独序列化后反解：

```
BUILD=0 MINE=1 DOORS_AND_SWITCHES=2 OPEN_CONTAINERS=3 ATTACK_PLAYERS=4 ATTACK_MOBS=5
OPERATOR_COMMANDS=6 TELEPORT=7 INVULNERABLE=8 FLYING=9 MAY_FLY=10 INSTABUILD=11
```

创造模式集合 = 上面除 `OPERATOR_COMMANDS` 外的 11 项 = `0x0FBF`。

**验收**：`--selftest=abilities` 输出与 codec 参考**逐字节一致**：

```
0100000000000000 0100010100 ffff0700 bf0f0000 cdcc4c3d cdcccc3d
```

人类实测：无血条/饥饿条、双击跳跃可飞、空中能悬停。

---

## 2. 已核实的关键事实（都带证据）

1. 客户端协议 **766**，与 `V766` 定义一致。
2. `StartGame` 必须逐字节符合 766 规范，用 `minecraft-data` + `bedrock-protocol` 做参考实现逐字节比对后修正：
   - `experimental_gameplay_override` 是 **普通 bool**（1 字节），不是 `Option<bool>`；
   - `#[nbt]` 字段**没有长度前缀**，且字符串长度用 **varint**（不是 u16）；
   - `experiments` 的长度是 **固定 i32 LE**（4 字节），不是 varint。
3. 区块负载结构（照 PMMP `ChunkSerializer`）：
   - 子区块 v8/v9 均可，按"从世界底部向上堆叠"；
   - 层头 `(bits<<1)|1` = 运行时 ID，单值时随后一个 zigzag varint state id；
   - 之后是**全部 24 段**生物群系；
   - 末尾必须有 **边界方块计数**(varint 0) 和 **方块实体数组**(varint 长度 0)。
4. `StartGame.item_list` 必须下发完整物品表（1.21.50 共 1788 条），否则手机会出问题——已补齐。
5. 运行时 ID：`stone=2403`、`air=11996`、`glowstone=5588`（minecraft-data 1.21.50）。

---

## 3. 下一阶段入口（原「方块不渲染」已关闭，见 §0 与 §0.2）

出生阶段的可见问题已经清零：方块渲染、玩家状态（氧气条/移动速度）、输入上报都正常。

还没做的（按 Geyser `GeyserSession.connect()` 的抓包顺序，属于**非必需**但「抄全」清单）：

| 包 | 现状 | 说明 |
|---|---|---|
| `SyncEntityProperty` ×3 | ⬜ | creaking / bee / armadillo 属性；数据源不在 Geyser 源码里，需从抓包反推 NBT |
| `PlayerList(ADD)` | ⬜ | 需要 `SerializedSkin`，体积较大 |
| `CreativeContent` 实际内容 | ⬜ | 现在是合法空表；创造背包要能搜索物品就得填 1788 条（每条的 `block_runtime_id` 需要 物品→默认方块状态 的映射表） |
| `CraftingData` 实际配方 | ⬜ | 现在是空表 + clean=true |
| `AvailableCommands` 实际命令 | ⬜ | 现在是空表 |

---

## 4. 工程产出

```
~/bedrock/
├── bedrock-rs/          上游副本（5 处本地补丁，见 HANDOFF §8）
├── spike/               本工程
│   ├── src/main.rs      登录链路 + 区块下发 + 玩家元数据 + 自测
│   ├── src/world.rs     区块负载编码（Runtime / Persistent 两种模式）
│   └── src/items_gen.rs 自动生成的 1788 条物品表
├── refs/                参考实现源码 + Java 探针（ProbeFlagBits / ProbeMeta / ProbeAbilityPacket …）
├── tools/               Node 参考实现（minecraft-data / bedrock-protocol / prismarine-chunk）
├── docs/                计划、交接、可行性、补丁记录
└── logs/                每次会话的包级日志
```

**自测入口**（不需要客户端）：
```sh
cd ~/bedrock/spike
./target/debug/bedrock-spike --selftest          # StartGame + 区块构造
./target/debug/bedrock-spike --selftest=meta     # 玩家元数据 SetActorData 字节（与 codec 对拍用）
./target/debug/bedrock-spike --selftest=spawn    # connect 阶段 8 个包负载字节（与 codec 对拍用）
./target/debug/bedrock-spike --selftest=7        # StartGame 序列化往返
./target/debug/bedrock-spike --selftest=9 <path> # 导出 StartGame 原始字节
```

**参考实现校验**（这也是这次踩坑的最大教训）：
```sh
cd ~/bedrock/tools
node ref_parse3.js ~/bedrock/logs/startgame.bin   # 用官方 schema 解析我们的包
```

---

## 5. 教训（写进流程）

第一次出现"破损的数据包"时就该搭参考实现做逐字节 diff，而不是"改一个字段 → 让人试一次"。
后者的代价：人类白试了 5 次，总耗时多出约 40 分钟。
**新规则：任何自定义二进制编码，先有参考实现校验，再接客户端。**

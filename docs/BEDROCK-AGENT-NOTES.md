# 基岩版维护须知（写给未来的自己）

> 这份文件是**给接手基岩版开发的 AI / 人喂的**，不是给玩家看的文档。
> 每次基岩版开工前先读一遍；每次收工前把「§9 未结清单」更新掉。
> 台账 `BACKLOG.md` 是契约（BUG-032 章节 + §9.x），这里只做**地图与边界**，不重复抄台账。

---

## 1. 角色边界（先读这条）

- **我只负责基岩版（Bedrock）**。MCHPRS 的核心（红石引擎、Java 协议、WorldEdit、存储器、
  io.txt、脚本协议）**不属于我的范围**，不要顺手改。
- 允许碰 `crates/core` 的唯一方式：**加 Bedrock 分支 / 加语义化推送方法**，
  Java 行为必须保持字节级不变。凡是会改变 Java 行为的地方，见 §4 的白名单，逐条都在台账里。
- `vendor/bedrock-rs` 是上游 bedrock-rs 的**完整副本**（自带独立 workspace，
  根 `Cargo.toml` 里 `exclude = ["vendor/bedrock-rs"]`）。我们对它有 9 处本地补丁
  （PATCH-001…009，清单见 §3.4），改它要单独记账。
- 权威来源只有两个：**Geyser 源码**（`D:\computer\mchprs\bedrock\refs\Geyser-<hash>`）
  和 **Cloudburst v766 codec**（`refs/geyser-deps` 里的 jar）。二者都**在 MCHPRS_IO 之外**，
  只作取证参考，**绝不进入构建/运行依赖**。猜出来的常量一律不许进代码。

---

## 2. 目录地图（基岩版相关）

```
crates/network/src/bedrock/        ← 基岩版全部协议/会话代码（我的主战场）
  mod.rs            bedrock_block_state / mchprs_item_id / bedrock_item_id 三个换算入口
  session.rs        登录链路 + 主循环（收包、限流日志、各 BedrockCommand 分发）
  translate.rs      入站 Bedrock 包 → Java 入站包（命令/聊天/移动/挥手/写牌…）
  bridge.rs         BedrockEvent / BedrockCommand / BedrockPlayerLink（双向桥）
  protocol.rs       Bedrock 出站包编码器（含 9 个逐字节对拍测试）
  chunk.rs          区块负载编码（子区块/生物群系/边界/方块实体流）
  block_entity.rs   告示牌 NBT：BlockActorData + 区块尾部 NBT 流 + 反解客户端写牌
  entity.rs         其它玩家（PlayerList/AddPlayer/MovePlayer/MobEquipment/Animate）
  inventory.rs      背包容器包（ContainerOpen/InventoryContent/ContainerClose）+ 丢物品判定
  scoreboard.rs     右侧计分板（SetDisplayObjective/SetScore/RemoveScore）
  commands.rs       命令树（AvailableCommands）+ 软枚举（UpdateSoftEnum）
  commands_gen.rs   由 generators/gen_commands.py 生成，不要手改
  creative.rs       R1 创造背包（CreativeContent，1540 条）
  creative_gen.rs / items_gen.rs / block_map.rs / item_map.rs   ← 生成数据，不要手改
  text.rs           聊天（TextPacket）
  data/*.dat        BiomeDefinitionList / AvailableActorIdentifiers 的载荷（include_bytes 内嵌）
crates/core/src/       ← 只允许"加 Bedrock 分支"；改动清单见 §4
crates/text/src/lib.rs ← 新增 to_legacy_text（JSON 组件 → § 码），纯新增
crates/network/examples/bedrock_server.rs ← 本地起 Bedrock 端点的最小示例
docs/bedrock-integration.md、docs/BEDROCK-SPIKE-STATUS.md ← 早期设计/踩坑记录（历史，可参考）
generators/            ← 只在改映射/命令表时用，产物是上面的 *_gen.rs
```

---

## 3. 我对仓库改了什么（vs 基岩接入前）

查看真实差异（自己跑，不要凭记忆）：

```powershell
git diff --stat origin/main..HEAD -- . ":(exclude)vendor"   # 非 vendored 的全部改动
git diff --stat origin/main..HEAD -- vendor                 # vendored 副本（整包新增 + 9 处补丁）
```

### 3.1 纯新增（不影响任何既有行为）

- `crates/network/src/bedrock/**` 整个模块（约 1.4 万行，含生成数据）
- `crates/network/src/lib.rs`：`+ pub mod bedrock;`
- `crates/network/Cargo.toml`：加 `bedrock`（vendored）、`tokio`、`nbtx`、`uuid` 依赖
- `crates/text/src/lib.rs`：`+122` 行，新增 `ColorCode::code_char` / `nearest_code_for_hex` /
  `TextComponent::to_legacy_text`（把 Java JSON 文本降级成 `§` 码，给计分板/告示牌/聊天用）
- `crates/network/examples/bedrock_server.rs`、`generators/*`、`docs/*`
- `Cargo.toml`：`exclude = ["vendor/bedrock-rs"]`

### 3.2 新增字段 / 配置（默认值保持原行为）

- `crates/core/src/config.rs`：`+ bedrock: i64 = 19132`（UDP 监听端口，与 Java `bind_address` 分开）

### 3.3 core 里的接线（**Java 行为不变**）

| 文件 | 改动 | 说明 |
| --- | --- | --- |
| `crates/core/src/player.rs` | `+364` | `PlayerBackend::{Java,Bedrock}` 枚举 + 一串**语义化推送方法**（`send_sidebar` / `send_chat` / `send_players` / `send_chunks` / `send_block_update(s)` / `send_abilities` / `send_teleport` / `send_swing` / `send_game_mode` / `send_sign` / `send_open_sign` / `send_inventory` / `send_soft_enum`）。Java 分支照旧发 Java 包（`send_open_sign` 的 Java 分支与改动前字节相同，只是搬到方法里） |
| `crates/core/src/plot/mod.rs` | `+766` | `enter_plot` 里给 Bedrock 推软枚举/背包/游戏模式；区块流送（`stream_bedrock_chunks`）、玩家快照（`send_bedrock_player_sync`）、登录快照（`build_bedrock_snapshot`）、告示牌（`bedrock_signs_in_chunk` / `push_bedrock_sign`）、`sign_rows_to_legacy` 等。**全部由 `player.client.is_bedrock()` 守卫** |
| `crates/core/src/server.rs` | `+188` | 起 Bedrock UDP 监听线程、`BedrockEvent` 处理、Bedrock 玩家注册（`handle_bedrock_register`）、`PrivMessage` 新增变体。Java 监听路径未改 |
| `crates/core/src/plot/packet_handlers.rs` | `+54` | 新增 `BedrockInventoryUpdate` 的排空应用（必须先于收包处理）；`handle_swing_arm` 里附加一次 `client.send_swing()`（Java 侧是空实现） |
| `crates/core/src/plot/scoreboard.rs` | `+39` | 新增 `bedrock_sidebar()`，在 `add_player`/`set_lines`/`set_line` 处一起推给 Bedrock |
| `crates/core/src/plot/commands.rs` / `coord.rs` | 小 | `//save`、`//schem`、`/coord`（含 undo/redo）之后重推软枚举；`file_candidates` 等改 `pub(super)` |
| `crates/core/src/plot/data.rs` | `+2` | `PlotWorld` 新增字段 `pending_bedrock_signs` 的初始化 |
| `crates/core/src/interaction.rs` | `±24` | 放告示牌改成 `client.send_open_sign(...)`（Java 分支等价）；`on_use` 新增"点已有牌子打开编辑框"，**带 `is_bedrock()` 守卫**，Java 不走 |
| `crates/core/src/plot/packet_handlers.rs` 等 | — | 见上 |

### 3.4 会改变 Java 行为的地方（**只有这几条，全部是 bug 修复/等价重构，已在台账）**

1. `plot/mod.rs::leave_plot` —— 修 panic（BUG-032 第 41 条）：原来按玩家下标删
   `packet_senders`，但那个列表只装 Java 玩家。现在只在 Java 玩家身上按"前面有几个
   Java 玩家"定位。**Java 侧受益**（以前场上有 Bedrock 玩家时会崩服）。
2. `interaction.rs::place_block` —— 把 `COpenSignEditor` 的构造搬到
   `PlayerBackend::send_open_sign`，Java 分支发出的内容与改动前一致。
3. 其余为纯新增或 `is_bedrock()` 守卫内。

> 换句话说：**Java 侧唯一的可见变化，是"基岩玩家在场时离开 plot 不再崩"**。

### 3.5 vendored 补丁（在 `vendor/bedrock-rs` 里）

PATCH-001 `level/src/greedy.rs`（AVX-2 cfg 守卫）、PATCH-002/003
`v662/packets/start_game.rs`（`player_property_data` 手写 newtype + NBT 编码）、
PATCH-004 `player_permission_level.rs`（`enum_repr(i8)`）、PATCH-005 `data_item_type.rs`
（`Int64` 改 `u64`，LONG 元数据是无符号 varint）、PATCH-006 `command_origin_data.rs`
（去掉 `read_to_end` 的 run-on 解码）、PATCH-007 `item_stack_request_action_type.rs`
（`CraftCreative` 字段宽度）、PATCH-008 `item_stack_response_info.rs`
（去掉 run-on 解码，多条目才能解；容器本来就装在 `ItemStackNetResult::Success(..)` 里）、
PATCH-009 `available_commands.rs`（`EnumDataEntry` 字段改 pub）。

---

## 4. 不变量（真机验证过的，别随手动）

- **坐标**：`Bedrock y = MCHPRS y − 64`（`chunk::Y_OFFSET`）；`NetworkBlockPosition.y` 是
  `u32`，负高度按 32 位二补数写。
- **眼睛高度**：客户端上报的 `player_position` / `MovePlayer` / `Respawn` 是**眼睛**
  （脚 + 1.62），`AddPlayer` 是脚。差 1.62 会表现为"两个客户端互相看差两格"。
- **实体 FLAGS**：客户端的**低位区**比 Cloudburst `EntityFlag` 序号后移一位
  （`SNEAKING = bit2`、`SPRINTING = bit4`），**高区位号与 codec 一致**
  （`CAN_SHOW_NAME 15`、`SILENT 18`、`CAN_CLIMB 20`、`BREATHING 36`、`HAS_COLLISION 49`、
  `HAS_GRAVITY 50`）。千万别再"整体 +1"（第 42 条：会导致降落变慢）。
- **告示牌 NBT 键**：`x/y/z/id="Sign"/FrontText/BackText/IsWaxed`，
  每面 `Text`（四行 `\n` 拼、去尾部换行）/`SignTextColor`/`IgnoreLighting`。
  **必须发 `OpenSign` 包客户端才会弹编辑框**（它不会自己开）。
- **背包**：`StartGame.enable_item_stack_net_manager = false`（客户端权威）。
  → 任何 `InventoryContent` 都可能覆盖玩家真实物品；开背包**只发 `ContainerOpen`**（第 45 条）。
  服务端只知道"选中槽/最近同步的槽"，其余槽位一律不要假设。
- **丢物品**：`InventoryTransaction(NORMAL)` + 两条 action，`actions[0]` 来源是
  `WorldInteraction`；判据用"那一格数量变小"，别只信 flag 值。
- **区块尾部**：边界方块计数（1 字节 `0`）之后是**裸网络 NBT 串**（无数量/长度前缀，
  Cloudburst `NbtUtils.createNetworkWriter` 的写法）。生物群系段固定 24 段。
- **字节级对拍**：`crates/network/src/bedrock/protocol.rs` / `scoreboard.rs` / `inventory.rs` /
  `chunk.rs` / `block_entity.rs` 里的 `*_matches_*_bytes` 测试就是护栏，动常量前先跑。

---

## 5. 构建 / 测试 / 运行

```powershell
# 编译（先停服务端，否则 target\debug\mchprs.exe 被占用会失败）
cargo build
cargo test -p mchprs_network bedrock      # 基岩版单测，当前 69 条
cargo test --workspace

# 运行（run/ 里有 Config.toml / io.txt / world / schems，被 .gitignore 忽略）
Set-Location run; & ..\target\debug\mchprs.exe *> D:\computer\mchprs\bedrock\logs\<名字>.log
```

- Java 端口 `25565`（TCP），Bedrock 端口 `19132`（UDP，`Config.toml` 的 `bedrock` 字段）。
- 收包失败/断线/panic 都打标准输出，重定向到日志文件即可事后看。
- **不要用 PowerShell 读写 `.rs`**：`Get-Content` 按 GBK 解码再按 UTF-8 写回会把中文变乱码
  （历史上真把 `session.rs` 写坏过一次）。用编辑器（edit/write 工具），UTF-8 无 BOM、LF。
- `cargo`/`rustfmt` 在这个 shell 里即使成功也可能回报 `[exit code: 1]`，看输出内容。

### 5.1 调试开关（平时全关，排查时改常量再编译）

`crates/network/src/bedrock/debug.rs` 顶部三个写死的常量：

| 常量 | 作用 | 平时 |
| --- | --- | --- |
| `PACKETS` | 逐包收发摘要 `[PKT] << 包名` / `[PKT] >> 包名 (N B)` | `false` |
| `PACKET_HEX` | 出站包 hex（截断 160 字节），查字节布局用 | `false` |
| `VERBOSE` | 明细日志 `[--]/[S3]/[R5]/[A2]/[R7]/[R8]/[R10]`（逐事件、量大） | `false` |

开法：把要看的常量改成 `true` → `cargo build` → 起服务，日志里就会出现对应行。
调用点：入站 `dump_in(&batch)`、每个 `conn.send(&packets)` 之前 `dump_out(&packets)`；
明细日志用宏 `crate::bedrock_verbose!(...)`（等价 `if VERBOSE { println!(...) }`）。
**收工前记得改回 `false`**（否则日志会被刷爆）。

---

## 6. 单独拿出 `MCHPRS_IO` 还能不能跑基岩版？

**能，而且已经实测过**（2026-11-01）：

- 把 `MCHPRS_IO` 拷到 `D:\computer\mchprs\bedrock\tmp\standalone-test\`（排除 `target`/`.git`/`run`），
  在副本里 `cargo build --offline` **通过**。
- 依赖都在仓库内或 crates.io/缓存里：`vendor/bedrock-rs` 是相对路径依赖；
  `nbtx` 是 git 依赖（首次构建需要网络或 cargo 缓存）；`crates/network/src/bedrock/data/*.dat`
  用 `include_bytes!` 内嵌；映射表（`block_map.rs`/`item_map.rs`/`creative_gen.rs`）都是生成进源码的。
- **仓库里没有任何指向仓库外的绝对路径**（`git grep -n -E "D:\\\\|refs/" -- . ":(exclude)vendor"` 只命中
  `BACKLOG.md` 里的**历史取证记录**，不是代码/构建依赖）。
- Geyser 源码、`refs/geyser-deps`、`Probe*.java`、`_patch_*.ps1` **全在仓库外**，
  只在取证时用；`MCHPRS_IO` 的构建与运行都不看它们。

---

## 7. 取证通道（仓库外，别入库）

**完整流程与命令见 `docs/BEDROCK-CAPTURE.md`**（C7 固化后唯一入口）。速记：

1. **Cloudburst 探针**：`refs\tools\probe.ps1`（一条命令编译+运行，别再手拼 classpath）：
   `& refs\tools\probe.ps1 ProbeSign`、`... -List`、`... -WithGeyserClasses`（要 Geyser 自己的类时）。
   源码在 `refs\probes\`，产物在 `refs\out\`。
2. **Geyser packet-spy 抓包**：改 `refs/Geyser-<hash>` 的 checkout（**只在 Geyser 侧**），
   细节写在 `BACKLOG.md` §9.1 附近；用来对照"真实 Geyser 会发什么"。
3. 探针产出（结论都已写进台账 + `#[test]`）：`ProbeEntityFlagOrder`、
   `ProbeInventory`/`ProbeCraftCreative`/`ProbeItemRequest`、`ProbeScoreboard`、`ProbeSoftEnum`、
   `ProbeGeyserChunk`、`ProbeSign`、`ProbeChunkNbt`、`ProbeItemStackResponse`。
4. 排错小工具 `refs/tools/check_state.js`（不在仓库内）：给一个 Java 方块状态 id 或
   Bedrock 运行时 id，反查方块名+属性。用法：

   ```powershell
   $env:NODE_PATH="D:\computer\mchprs\bedrock\tools\node_modules"
   node D:\computer\mchprs\bedrock\refs\tools\check_state.js 4317   # java state
   node D:\computer\mchprs\bedrock\refs\tools\check_state.js 9214   # bedrock runtime id
   ```


---

## 8. 状态与已知限制（指针，细节看台账）

- 已真机验收：R1 创造背包、R2 出生点、R3 计分板、R4 命令、B3 命令树、B4 软枚举、
  第 22 条（看见别的玩家）、第 26/19 条（增量区块流送）、A1/A3/A5/A-t1/A-t2、
  第 38~42 条、第 44/45 条（丢物品镜像、开背包不清空）。
- 已知限制（**不是 bug，别当 bug 修**）：告示牌超宽行不裁剪、富文本不转 `§` 码、
  `item_map` 只有 135 条（表外物品显示空手）、旁观者登录瞬间按创造、
  计分板一定会显示分数数字、创造面板换物品后未切栏前别人看你手里还是旧的。
- 明确**不做**（用户裁决，别自己开工）：A4/A6/A7/A8、B2、B5/B6/B7、C1–C6。

## 9. 未结清单（每轮开工先看这里）

| 项 | 状态 | 线索 |
| --- | --- | --- |
| BUG-032 第 43 条（区块携带的牌子字） | ✅ 功能上关闭（2026-11-01）：进 plot 补推 ±3 区块内最多 64 块牌子 | 台账 §9.28/§9.29；区块内 NBT 仍照发（离线解析证明几何正确），但客户端不吃，所以不依赖它 |
| 告示牌文字颜色 | **必须白色**（`0xFFFFFFFF`）+ `IgnoreLighting=0` | 黑字真机看不见（踩过两次）；发光是临时诊断时用过的，已按用户要求去掉（§9.29.2） |
| 诊断日志（`[R7] 快照区块`/`告示牌包 hex`/`[A2] 解析后`） | 已挂到 `debug::VERBOSE` | 开 `VERBOSE` 才打（§5.1） |
| B1（PATCH-008 `ItemStackResponse`） | ✅ 已修（2026-11-01，待编译） | 台账 §9.27.1；`refs\ProbeItemStackResponse.java` + 测试 `item_stack_response_decodes_cloudburst_bytes` |
| C7（抓包基础设施固化） | ✅ 已做（2026-11-01） | 台账 §9.27.2；`refs\tools\probe.ps1` + `docs/BEDROCK-CAPTURE.md` |

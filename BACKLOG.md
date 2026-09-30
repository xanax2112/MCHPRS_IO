# BACKLOG — 需求与缺陷台账

> **用途**：记录用户提出的 **bug** 与 **新功能** 需求，以及对应的修改方案。
> **用法**：平时**只记录、不动代码**；当用户说"统一改"时，批量实施全部待办条目，作为**一次版本升级**。
> **本文档取代 `src/HANDOFF.md`**（前任交接文档，已完成使命，不再维护）。

---

## 0. 约定

1. **协作铁律**：需求不明确时先复述语义 → 用户明确确认 → 再动代码。
   **严禁执行任何与 Rust 相关的终端操作**（`cargo build` / `cargo run` / `cargo test`、运行 `target/release/mchprs` 或任何编译产物），编译与运行验证一律由用户执行；
   **与 Rust 无关的终端操作允许执行**（只读探查、列出/查看文件、git 只读命令、必要时修复环境权限等）。
2. **记录规则**：每条待办给一个编号——`BUG-nnn` 或 `FEAT-nnn`——并写清「现象 / 根因 / 方案 / 影响文件 / 验证方式 / 状态」。
3. **版本升级**：批量实施时在文末「版本升级记录」登记，并提升 `Cargo.toml` 的 `[workspace.package] version`。
   内核版本定义在 `Cargo.toml` 的 `[workspace.package] version`；自动化协议版本是 `crates/core/src/automation.rs` 里 HELLO 的**硬编码字面量**，两者独立编号。当前**两者均为 `0.2.0-beta`**（见第 5 节）。

## 1. 工作环境速查

- 仓库根（本会话工作区）：`D:\APP\MC\mchprs\MCHPRS_IO-master`
- 运行目录：`<仓库>\MCHPRS_IO`（`Config.toml`、`test.py`、`world/`、`schems/`、`logs/`）
- git 可用，但需完整路径调用：`& 'C:\Program Files\Git\cmd\git.exe' ...`（DSH 进程沿用了安装前的环境变量，直接敲 `git` 会"无法识别"）
- 基线提交：`main` @ `11ff766 Initial commit: MCHPRS_IO automation bridge`
- ⚠️ `core.autocrlf=true` 且 `.gitattributes` 未固定行尾：`checkout` / `stash` / 重置可能把工作区整体改写成 CRLF，产生巨量假 diff

---

## 2. 待处理

### BUG-001　水锅读不到信号 & 空锅/空容器/空堆肥桶导致崩溃

**状态**：✅ **已实施**（0.2.0-beta，待用户编译验证）——水锅正常取值；空锅/空容器/空堆肥桶 → 交给 BUG-004 报错。与 **BUG-004** 一起完成
**登记日期**：2026-09-30（2026-09-30 升级处理方式）
**影响文件**：`crates/redstone/src/comparator.rs`、`crates/redpiler/src/passes/frontend/identify_nodes.rs`

**现象**

- **水锅**（`minecraft:water_cauldron`，1/2/3 格水）接比较器：读出来恒为 **0**，应为 1/2/3。
- **空锅**（`minecraft:cauldron`）接比较器：`/redpiler compile` 时**服务器 panic**（`unreachable!("Block does not override comparators")`）。

原版行为参照（[Minecraft Wiki · Cauldron](https://minecraft.wiki/w/Cauldron)，Redstone component 一节）：
空锅 → 0；1/3 满 → 1；2/3 满 → 2；满 → 3；**岩浆锅 → 3**；细雪锅 → 按层数 1~3。

**根因**（已核实）

`crates/redstone/src/comparator.rs` 中 `has_override`（闸门，决定"要不要走 override 查询"）与 `get_override`（实际取值）两份名单**双向不一致**：

| 方块 | `has_override` | `get_override` | 后果 |
| --- | --- | --- | --- |
| `Block::Cauldron`（空锅） | ✅ 在（:34） | ❌ 不在 | 过闸门却无分支 → 落进 `other =>` → `unreachable!` **panic**（:61–71） |
| `Block::WaterCauldron { level }`（水锅） | ❌ **不在** | ✅ 在（:57） | 闸门永不放行 → **:57 是死代码** → 按普通方块处理 → 恒 **0** |

支撑证据：

1. `has_override` 是**唯一闸门**，全部 5 个调用点都要先过它：
   `comparator.rs` :79、:96、:101；redpiler 侧 `identify_nodes.rs` :177；`input_search.rs` :314。
2. 全仓搜 `WaterCauldron`，除 `crates/blocks/src/generated.rs` 的定义/属性外，只出现在 `comparator.rs:57`，**无任何其它补救逻辑**。
3. 退路同样拿不到信号：水锅 `is_solid()` = **false**（`generated.rs:742` 的 `is_solid` 是显式白名单，里面没有炼药锅），于是 `calculate_input_strength` 走到 `else { base_input_strength }`；而 `diode_get_input_strength` 读到的炼药锅不是电源 → **0**。
4. **编译器同样中招**：redpiler 对 `has_override` 为真的方块生成 `NodeType::Constant`（`identify_nodes.rs:177-180`）；水锅不过闸门 → 不生成节点 → 编译出的电路里比较器输入也是 0。

**处理方式（2026-09-30 升级，已确认）**

| 比较器读到的方块 | 期望行为 |
| --- | --- |
| **水锅** `WaterCauldron { level }`（1/2/3） | **正常取值** → 返回 `level`（1/2/3，与原版一致） |
| **空锅** `Block::Cauldron` | **交给 BUG-004**：编译中止 + 聊天窗口报坐标（不崩溃、终端无输出） |
| **空木桶 / 空熔炉 / 空漏斗** | 同上 |
| **空堆肥桶** `Composter { level: 0 }` | 同上 |
| 非空木桶 / 熔炉 / 漏斗 / 堆肥桶（level 1–8） | 正常取值 |
| 其余合法项（`Cake`、`EndPortalFrame`） | 正常取值（照旧） |

**"空"的判定（已确认）**

- **空锅**：`Block::Cauldron`（单一状态，id 7398；`WaterCauldron` 的 level 是 1–3，没有 level 0）。
- **空容器**（`Barrel` / `Furnace` / **`Hopper`**）：满足**任一**即为空——
  - `world.get_block_entity(pos)` 返回 `None`（**没有方块实体数据**，如原理图未带 BE）；**或**
  - `Some(BlockEntity::Container { comparator_override: 0, .. })`（箱子存在但**空**；见 `crates/blocks/src/block_entities.rs:146-152`，空箱子照样会有 BE、override 算出来是 0）。
  - 另外 `Some(other)`（BE 类型不对）也归为"取不到合法值"。
- **空堆肥桶**：`Block::Composter { level: 0 }`（level 范围 0–8，id 19372–19380）。

> ⚠️ **与原版的差异（已知并接受）**：原版里空锅/空箱子/空堆肥桶读取就是合法的 **0**。本方案把它们当作**错误**，因此任何比较器对着空容器/空堆肥桶的电路，**整次编译都会中止**（不只是那一点读 0）。这是用户明确要求的行为。

**实现方案**

1. `has_override` 增加 `Block::WaterCauldron { .. }` → 使 :57 的 `level` 真正生效（修掉死代码）。
2. 取值改为**可失败形式**（与 BUG-004 统一）：`try_get_override(block, world, pos) -> Option<u8>`
   - `WaterCauldron { level } => Some(level)`；`Composter { level } if level > 0 => Some(level)`；
   - `Cauldron => None`；`Composter { level: 0 } => None`；
   - `Barrel | Furnace | Hopper`：BE 缺失、`comparator_override == 0`、或 BE 类型不对 → `None`；否则 `Some(comparator_override)`；
   - `Cake`、`EndPortalFrame` 照旧 `Some(...)`；**未覆盖的方块 → `None`**（不再 `unreachable!`）。
3. **编译路径**（`identify_nodes.rs:177-180`）遇到 `None` → 上报错误并中止（细节见 BUG-004）。
4. **红石模拟路径**（`get_far_input` / `calculate_input_strength`，:79/:96/:101）用 `.unwrap_or(0)` → **不崩溃、不报错、终端无输出**。
5. 移除 `comparator.rs:61-71` 的临时 `eprintln!`（BUG-004 要求终端无任何输出）。

**报错文案（已确认：区分两类，英文）**

- 空容器类：`Redpiler compile aborted: comparator at (x, y, z) reads an empty barrel`
- 未知方块类：`Redpiler compile aborted: comparator at (x, y, z) reads a block with no comparator override`

> 因此错误类型需要携带**种类**（空容器 / 未知方块 / 具体方块名）而不只是坐标，建议用枚举承载，如 `CompileError::EmptyContainer { pos, kind }` / `CompileError::Unsupported { pos, block }`。

**范围界定（不变）**

岩浆锅 / 细雪锅**不处理**（用户已明确不需要）。
备查事实：这两个方块在 `mc_data/blocks.json` 中有定义，但未登记进 `mc_data/gen_info.yaml` 白名单，而生成器以该白名单为迭代依据（`crates/block_data_gen/src/main.rs:909-913`）→ 不在 `Block` 枚举中（`generated.rs:206-209`）→ 原理图载入时 `Block::from_name(...).unwrap_or(Block::Air)`（`crates/schematic/src/lib.rs:138`）**静默变成空气**，所以它们也进不到这条路径。

**验证方式（由用户执行）**

- 装好水（1/2/3 格）后 `/redpiler compile`，比较器输出 **1/2/3**（水锅正常）；
- 堆肥桶 level 1–8 正常取值；木桶/熔炉/漏斗有物品时按物品数量取值；
- 比较器对着**空锅 / 空木桶 / 空熔炉 / 空漏斗 / 空堆肥桶** → 编译**中止**、状态回到 **Stopped**、**终端无输出**、该玩家聊天窗口出现对应的英文报错 + **绝对坐标**；
- 非 redpiler 的普通红石模拟下，上述"空"情形读到 **0** 且**不崩溃**；
- ⚠️ redpiler 把 override 编成**编译期常量**：必须**先装水/装满物品、再 `/redpiler compile`**；编译后改动不会被重新采样（装水本身也不是红石事件）。

---

### BUG-002　`/gamemode` 等命令无法用 Tab 键补全

**状态**：✅ **已实施**（0.2.0-beta，待用户编译验证）——采用"甲"；服务端解析**未改动**，只补了命令树补全
**登记日期**：2026-09-30
**影响文件**：`crates/core/src/plot/commands.rs`（命令树）、`crates/network/src/packets/clientbound.rs`（参数类型定义）

**现象**

游戏内输入 `/gamemode`，服务器能正确解析并执行（`/gmc`、`/gmsp` 同理），但**按 Tab 补不出来**——服务器没有把这些命令的声明发给客户端。

**根因**（已核实）

客户端的补全完全依赖服务器在玩家加入时下发的 `DeclareCommands` 包，即 `crates/core/src/plot/commands.rs:561` 的 `DECLARE_COMMANDS`。而该命令树里**根本没有 `gamemode` / `gmc` / `gmsp` 节点**：

- 根节点子列表（:565-574）为 `[1,4,5,6,8,10,11,13,18,30,34,41,43,44,45,49,51,52]`，其中无任何 gamemode 节点；
- 全树 53 个节点（0–52）中也没有 gamemode 相关名字。

而命令**执行**是完全独立的另一条链路——`execute_command` 直接按名字匹配：`commands.rs:445` `"gmsp"`、:446 `"gmc"`、:447 `"gamemode"`。两条链路没有关联，所以"能执行、补不出"。

**顺带发现的同类问题**

`/plot lock`（节点 39）与 `/plot unlock`（节点 40）虽然被声明了，却**没有任何父节点引用**：根节点子列表无 39/40，`/plot`（节点 13，:686）的子列表 `[14,15,16,17,19,20,21,22,24,25,27,28,29]` 中也没有。它们成了**游离节点**，同样补不出来（而 `commands.rs:148` / `:158` 确实能执行它们）。

**修复方案**（已确认，等"统一改"时实施）

1. 在 `DECLARE_COMMANDS` 增加节点：
   - `/gamemode` 字面节点（`LITERAL`），`children` 指向参数节点；
   - 参数节点：`flags = ARGUMENT | EXECUTABLE`，`parser = Parser::GameMode`；
   - `/gmc`、`/gmsp`：`LITERAL | EXECUTABLE` 节点（无参数）；
   - 把这三个新节点的下标补进**根节点**的 `children`。
2. 在 `CDeclareCommandsNodeParser`（`crates/network/src/packets/clientbound.rs:397`）新增变体，写出 `minecraft:gamemode` 参数类型。
   - 该参数类型在 1.20.4 确实存在：`mc_data/registries.json:1296`，protocol_id **40**。客户端自身就带该类型，会直接补全 creative / survival / adventure / spectator，**无需**服务器实现补全请求。
3. 把 39 / 40 补进节点 13 的 `children`，修好 `/plot lock`、`/plot unlock` 的补全。

**已确认的处理方式（2026-09-30）**

采用 **甲**：只补补全，**不**支持生存/冒险模式。具体行为：

- 客户端照常补出 4 种模式（creative / survival / adventure / spectator）；
- 服务器收到 **survival 或 adventure** 时，只在该玩家的聊天窗口回红色 **`Unknown gamemode`**（走现成的 `send_error_message`，`crates/core/src/player.rs:629`）；
- 日志：**保留**现有的全局 `info!("… issued command: …")`（`commands.rs:224-229`，写入 `logs/mchprs.log` 与 stdout），**不为本情况新增任何日志**；
- `Gamemode` 枚举保持不变（仍只有 `Creative`、`Spectator`），不新增存档/网络层的模式支持。

**结论：服务端解析逻辑无需任何改动**

`commands.rs:453-461` 现有写法本就是：

```rust
let gamemode = match name {
    "creative" | "1" => Gamemode::Creative,
    "spectator" | "3" => Gamemode::Spectator,
    _ => { self.players[player].send_error_message("Unknown gamemode"); return false; }
};
```

`survival` / `adventure` 自然会落入 `_` 分支 → 红色 `Unknown gamemode`、无新增日志，**与上面确认的行为完全一致**。因此 BUG-002 的服务端解析改动量 = **0**，整条 bug 实际只需改**命令树**（客户端补全）。

**其它细节**

- 数字别名 `1` / `3`：改为 `minecraft:gamemode` 参数类型后，**客户端会按自己的解析器本地拒绝**（该类型只认四个模式名），所以 `/gamemode 1` 在客户端就被拦下、根本发不到服务端。服务端**保留**这两个别名（无害，非原版客户端/工具仍可触发）。
- `change_player_gamemode`（`crates/core/src/plot/mod.rs:388-394`）本身不打任何日志。

**验证方式（由用户执行）**

- `/game` + Tab → 应补出 `gamemode`；
- `/gamemode ` + Tab → 应列出 **4 种**：creative / survival / adventure / spectator；
- 选 `survival` 或 `adventure` 回车 → 玩家聊天窗口出现红色 `Unknown gamemode`；除原有的全局 "issued command" 日志外**无新增日志**；
- `/gamemode creative`、`/gamemode spectator` → 正常切换模式；
- `/gm` + Tab → 应出现 `gmc`、`gmsp`；
- `/plot lo` + Tab → `lock`；`/plot un` + Tab → `unlock`。

**备注**：`handle_command_suggestions_request`（`crates/core/src/plot/packet_handlers.rs:28-35`）目前只处理 `//load ` 前缀、其余直接返回；本方案用 `minecraft:gamemode`，不依赖它。

---

### BUG-003　PIN/POUT 位序改为 YZX（破坏性协议变更）

**状态**：✅ **已实施**（0.2.0-beta，待用户编译验证）——语义已确认并落地
**登记日期**：2026-09-30
**类型**：协议行为变更（**破坏性**——依赖旧位序的外部脚本/点阵会错位）
**影响文件**：`crates/core/src/automation.rs`、`src/protocol.txt`、`src/protocol_zh.txt`、`README.md`、`README_zh.md`、`MCHPRS_IO/test.py`

**现状（已核实，用户的描述正确）**

`crates/core/src/automation.rs:48-50` 的注释与实现一致：

- 点 `(i, j, k)` 位于 `(x0 + i*dx, y0 + j*dy, z0 + k*dz)`；
- 位序号 = **`i*(ny*nz) + j*nz + k`**，位 0 是线路上的最高有效位；
- 实际生成顺序见 `automation.rs:756-758`：`for i { for j { for k { … } } }` —— **k（Z）变化最快**，其次 j（Y），最慢 i（X）。

即"变化快慢"顺序为 **Z → Y → X**，也就是用户说的"ZYX 增大排列"。三处文档同样如此：`src/protocol.txt:87-88`、`src/protocol_zh.txt:76`、`automation.rs:49-50`。

**目标（已与用户确认）**

"按 YZX 排列" = 变化快慢顺序改为 **Y → Z → X**：**Y 变化最快**，其次 Z，最慢 X。于是：

- 新位序号 = **`j + k*ny + i*ny*nz`**（即 `j*(1) + k*(ny) + i*(ny*nz)`）；
- 生成循环改为 `for i { for k { for j { … } } }`；
- **Y 取减小方向**：`dx`、`dz` 维持增大方向（`nx > 1 → dx > 0`，`nz > 1 → dz > 0`）；
- **`dy` 规则（已确认）**：`ny == 1 → dy == 0`；**`ny > 1 → dy < 0`（严格为负）**。
  不允许 `ny > 1` 时取 `dy == 0`，否则多个位会落在**同一坐标**上（同一点被重复计数）；这与既有 `dx`/`dz` "范围 >1 必须严格非零"的约束保持一致，只把符号反过来。

**等价性说明（重要）**

只有当 **ny > 1 且 nz > 1** 时，新旧位序号才真正不同；任一轴为 1 时两者等价。以当前演示的输入端口 `nx=2, ny=8, nz=1` 为例：旧 `i*8 + j`，新 `j + 8i` —— **完全一致**。因此本次变更的实际影响是：**3D 点阵的位序** + **Y 方向的符号约定**。

**必须同步修改的地方**

1. `crates/core/src/automation.rs`
   - :48-50 文档注释改为新公式；
   - :739-741 的 `dy` 校验改为 `ny == 1 → dy == 0`、`ny > 1 → dy < 0`；
   - :756-758 循环次序改为 `for i { for k { for j { … } } }`。
2. 协议文档同步：`src/protocol.txt:85-90`、`src/protocol_zh.txt:74-78`（位序号公式 + 偏移正负规则）。
3. ~~**破坏性变更 → 建议提升协议版本**~~ → **已执行**：协议版本已提升为 `0.2.0-beta`，HELLO 串（`automation.rs:529`）、`src/protocol.txt`、`src/protocol_zh.txt`、`README.md`、`README_zh.md` 全部同步，详见第 5 节。
4. **演示客户端会直接注册失败**（本次变更最直接的破坏点）：
   `MCHPRS_IO/test.py:13` 的 `ADD_IN_OFFSETS = (3, 2, 0)` 与 :19 的 `ADD_OUT_OFFSETS = (0, 2, 0)` 都是 `dy = +2`；新规则要求 `dy < 0`，否则 `PIN` / `POUT` 会直接回 `ERR RANGE`。改法：
   - `ADD_IN`：`dy = -2`，原点 Y 移到顶端 → `ADD_IN_ORIGIN = (241, 25, 252)`（原 Y 范围 11..25）；
   - `ADD_OUT`：`dy = -2`，原点 Y 移到顶端 → `ADD_OUT_ORIGIN = (245, 27, 263)`（原 Y 范围 11..27）；
   - 还需复核 `pack_add_in` / `parse_add_out`（`test.py:37-50`）里的位反转逻辑：Y 方向翻转后，A/B 各位与物理拉杆的上下对应关系会被镜像，需按新映射重新推导。

**验证方式（由用户执行）**

- 更新后的 `test.py` 能注册成功（`OK PIN` / `OK POUT`），500 组随机向量依旧全过；
- 另造一个 `ny > 1 且 nz > 1` 的点阵，核对位序号符合 `j + k*ny + i*ny*nz`；
- 确认 `dy > 0` 被拒绝并返回 `ERR RANGE`。

---

### BUG-004　编译遇到未处理的比较器输入时崩溃（改为优雅中止 + 聊天窗口报坐标）

**状态**：✅ **已实施**（0.2.0-beta，待用户编译验证）——接收 BUG-001 投递的空容器错误 + 未知方块兜底、英文报错、自动编译仅停止。与 **BUG-001** 一起完成
**登记日期**：2026-09-30
**影响文件**：`crates/redstone/src/comparator.rs`、`crates/redpiler/src/passes/frontend/identify_nodes.rs`、`crates/redpiler/src/passes/mod.rs`、`crates/redpiler/src/lib.rs`、`crates/redpiler/src/task_monitor.rs`、`crates/core/src/plot/mod.rs`

**现象**

`/redpiler compile` 时，若比较器读到取不到合法值的方块，该 plot **直接崩溃**，同时终端打印出错方块的坐标。触发情形（详见 BUG-001）：

- 空锅 `Block::Cauldron`（**实测崩溃的就是这个**）；
- 空木桶 / 空熔炉 / 空漏斗（BE 缺失，或 `comparator_override == 0`）；
- 空堆肥桶 `Composter { level: 0 }`；
- 任何"`has_override` 为真但取值逻辑未覆盖"的方块（兜底）。

**根因**（已核实）

1. `comparator::get_override` 的兜底分支是 `unreachable!("Block does not override comparators")`（`comparator.rs:61-71`），前面还有前任加的临时 `eprintln!` 打印坐标。
2. 编译时由 `identify_block` 调用它：`crates/redpiler/src/passes/frontend/identify_nodes.rs:177-180`
   ```rust
   block if comparator::has_override(block) => (
       NodeType::Constant,
       NodeState::ss(comparator::get_override(block, world, pos)),
   ),
   ```
3. 编译跑在 `Plot::start_redpiler` 用 `thread::scope` 起的**子线程**里（`plot/mod.rs:940-944`）。子线程 panic 时，`thread::scope` 会在作用域结束时**把 panic 重新抛给父线程** → 整个 plot 线程崩溃。
4. 因此永远走不到 `plot/mod.rs:964-965` 的 `set_redpiler_state(Running)`，plot 处于崩溃状态。

**期望行为（用户要求）**

1. **不崩溃**；
2. **停止编译**，plot 状态恢复正常，即 `RedpilerState::Stopped`；
3. **终端不打印任何错误**（含删掉那句临时 `eprintln!`）；
4. 在**该玩家的聊天窗口**报错，并给出出错方块的**绝对坐标**。

**建议实现方案**

核心思路：把"未处理方块"从 `panic` 变成**可失败返回值**，再沿编译链上抛到 `start_redpiler`。

> **范围边界（重要）**：本条是**点状修复**——只处理"比较器取不到合法值"这一条路径。**不做**全局 panic 兜底（`catch_unwind` / 全局 panic 静音），其它已知可达的 panic 点**保持原样**。理由与完整清单见第 4 节「已明确不做」。实施时请勿顺手扩大范围。

1. `crates/redstone/src/comparator.rs`
   - 删除 `eprintln!`；
   - 把取值逻辑改成可失败形式，例如 `try_get_override(block, world, pos) -> Option<u8>`，`other =>` 返回 `None`（不再 `unreachable!`）；
   - **红石模拟路径**（`get_far_input`、`calculate_input_strength`，:79/:96/:101）用 `.unwrap_or(0)` 保持不崩——否则 plot 正常 tick 时同样会 panic；
   - 编译路径（`identify_nodes`）改用可失败版本。
2. `crates/redpiler/src/passes/frontend/identify_nodes.rs`
   - `for_each_block_optimized` 的闭包签名是 `FnMut(BlockPos) -> ()`（`crates/world/src/lib.rs:168-174`），**无法直接返回 `Result`**，所以用"错误槽"传递：把出错 `BlockPos` 写进共享单元并中止后续处理。
3. 错误通道（**二选一**）
   - **甲（推荐）**：复用现成的 `TaskMonitor`（`crates/redpiler/src/task_monitor.rs`）——加一个 `error` 字段与 `set_error()/take_error()`，把 monitor 塞进 `CompilerInput`（`crates/redpiler/src/lib.rs:302-305`，只需改 `compile` 里一处构造），`identify_nodes` 即可写错误；`run_passes` / `compile` 检查后提前返回。改动面最小。
   - **乙**：把 `Pass::run_pass` 改为返回 `Result`，逐层上抛。语义更干净，但要改**所有** pass 的签名（`passes/mod.rs:267-274` 及 10 余个实现）。
4. `crates/redpiler/src/lib.rs`
   - `Compiler::compile`（:147）改为返回 `Result`，**错误类型携带种类 + 坐标**（见上文 `CompileError` 两个变体，用于区分两种文案）；同时把 `monitor.cancelled()` 的早退也纳入返回类型区分。
5. `crates/core/src/plot/mod.rs`
   - `start_redpiler`（:927）接收 `Result`：
     - 成功 → `RedpilerState::Running`；
     - **失败 → `RedpilerState::Stopped`** + 向触发编译的玩家发**红色聊天消息**（含绝对坐标，`BlockPos` 的 `Display` 输出 `(x, y, z)`）；
     - 取消 → 同样停在 `Stopped`。
   - ⚠️ 顺带修一个既有问题：当前**无论成败**都在 :964-965 设 `Running`，连 `monitor.cancelled()` 提前返回的情况也会被设成 Running。
   - 需要把"触发者"传进来：`start_redpiler(options, source: Option<usize>)`；玩家命令在 `crates/core/src/plot/commands.rs:189` 传 `Some(player)`，自动编译在 `mod.rs:1457` 传 `None`。
6. **保证终端无输出**：新增代码不得使用 `warn!` / `error!` / `eprintln!`。

**与 BUG-001 的关系（已确认：两条一起做，BUG-001 主动向本条投递错误）**

- BUG-001 已升级：**空锅 / 空木桶 / 空熔炉 / 空漏斗 / 空堆肥桶**不再返回 0，而是**主动交给本条的报错路径**（编译中止 + 聊天报坐标 + 不崩溃 + 终端无输出）。
- 因此本条的职责范围 = ① BUG-001 投递的"空锅/空容器/空堆肥桶"错误；② 兜底：任何"`has_override` 为真但取值逻辑未覆盖"的方块。
- 两条**必须一起实施**——改的是同一个 `get_override` / `try_get_override` 函数，分开做会互相冲突。
- 错误类型需携带**种类**以支持两种文案：`CompileError::EmptyContainer { pos, kind }` / `CompileError::Unsupported { pos, block }`。

**已确认的其余细节（2026-09-30）**

- **报错语言：英文**（与服务器现有消息一致）。两种文案（由 BUG-001 确认）：
  - 空容器类：`Redpiler compile aborted: comparator at (x, y, z) reads an empty barrel`
  - 未知方块类：`Redpiler compile aborted: comparator at (x, y, z) reads a block with no comparator override`
- **自动编译失败（`mod.rs:1457`，无触发玩家）：仅停止，不报错、不打印**。
- **错误通道机制：按"甲"实施**（复用 `TaskMonitor`，见上文第 3 点）——纯内部实现选择，对外行为无差别。

**验证方式（由用户执行）**

- 构造一个会让比较器读到"未覆盖方块"的场景 → `/redpiler compile`：plot **不崩溃**、状态回到 **Stopped**、**终端无任何输出**、该玩家聊天窗口出现含**绝对坐标**的红色报错；
- `/redpiler compile` 成功时状态仍为 Running；
- 编译中取消（若有取消入口）也停在 Stopped。

---

## 3. 待添加功能

（暂无）

---

## 4. 已明确不做

| 条目 | 说明 | 确认时间 |
| --- | --- | --- |
| 岩浆锅 / 细雪锅的比较器支持 | 用户明确只需识别水锅 | 2026-09-30 |
| **其它 panic 崩溃点 / plot 崩溃全局兜底** | 用户明确**不改**。BUG-004 只做比较器取值这**一处点状修复**（未处理方块 → 返回错误 → 中止编译），**不引入** `catch_unwind`、也不做全局 panic 静音。<br>以下已查实、运行时可达的 panic 点**保持原样**：`crates/redpiler/src/passes/frontend/identify_nodes.rs:217`、`crates/redpiler/src/backend/direct/compile.rs:47-58`（≥255 入边）、`crates/core/src/plot/mod.rs:1683`（plot 加载失败）、`crates/redstone/src/lib.rs:260-262`（音符盒缓存不一致）、`crates/redpiler/src/compile_graph/stable_graph.rs`（4 处图不变量）、`crates/redpiler/src/lib.rs:226`、`crates/redpiler/src/passes/analysis/ss_range_analysis.rs:313`、`crates/core/src/plot/worldedit/mod.rs:217-262`、`crates/core/src/plot/monitor.rs:32`。 | 2026-09-30 |

---

## 5. 版本升级记录

### 0.2.0-beta（2026-09-30）

**版本号**：内核 `0.1.0-beta` → **`0.2.0-beta`**（`Cargo.toml` 的 `[workspace.package] version`）；TCP 自动化协议 `0.1.2-beta` → **`0.2.0-beta`**（`crates/core/src/automation.rs` 的 HELLO 串）。两者按用户要求同步为同一版本号。

**包含条目**：BUG-001、BUG-002、BUG-003、BUG-004（四条全部实施）。

**变更摘要**

| 条目 | 实际改动 |
| --- | --- |
| BUG-001 + BUG-004 | `crates/redstone/src/comparator.rs`：`has_override` 补 `WaterCauldron`；新增 `OverrideError`；`get_override` 拆成 `try_get_override`（返回 `Result`，供编译）与 `get_override`（`unwrap_or(0)`，供红石模拟）；空锅/空容器/空堆肥桶 → `EmptyContainer`，未覆盖方块 → `Unsupported`；删除临时 `eprintln!`。<br>`crates/redpiler/src/task_monitor.rs`：新增错误槽 `set_error`/`error`（只记首个错误）。<br>`crates/redpiler/src/lib.rs`：新增 `CompileError { Override, Cancelled }`；`CompilerInput` 增加 `monitor` 字段；`Compiler::compile` 返回 `Result<(), CompileError>`。<br>`crates/redpiler/src/passes/frontend/identify_nodes.rs`：`identify_block` 返回 `Result`，错误写入 monitor 并跳过注释遍。<br>`crates/redpiler/src/passes/mod.rs`：`run_passes` 出错即提前返回。<br>`crates/core/src/plot/mod.rs`：`start_redpiler(options, source)` 按结果显示 Running / **Stopped**，失败时向触发者发红色英文消息（含绝对坐标）；顺带修掉"无条件设 Running"；`handle.join()` 后仅对本条错误优雅处理，其它 panic 仍 `resume_unwind` 保持原行为。<br>`crates/rilc`：两处 `CompilerInput` 构造同步传入 monitor。 |
| BUG-002 | `crates/network/src/packets/clientbound.rs`：`CDeclareCommandsNodeParser` 新增 `GameMode` 变体 → 写出 `minecraft:gamemode`。<br>`crates/core/src/plot/commands.rs`：命令树新增节点 53 `/gamemode`、54 参数、55 `/gmc`、56 `/gmsp` 并挂进根节点；把游离节点 39/40 补进 `/plot` 的 children。服务端解析与日志**未改**（`survival`/`adventure` 本就落 `_` 分支回 `Unknown gamemode`）。 |
| BUG-003 | `crates/core/src/automation.rs`：位序号改为 `j + k*ny + i*ny*nz`；循环改 `for i { for k { for j } }`；`dy` 校验改为 `ny>1 → dy < 0`。<br>`src/protocol.txt` / `src/protocol_zh.txt`：公式、位推进顺序、偏移正负规则；示例会话的 `PIN` 由 `dy=1` 改为原点 y=67、`dy=-1`。<br>`MCHPRS_IO/test.py`：`ADD_IN` 原点 y 11→25、`dy` 2→-2；`ADD_OUT` 原点 y 11→27、`dy` 2→-2；`pack_add_in` / `parse_add_out` 按新位序重算（**保持与旧版相同的物理↔逻辑对应**，故原原理图无需重建），并移除随之失效的 `_REV8` / `_REV12` 查表。 |
| 文档 | `README.md` / `README_zh.md` 协议版本号同步为 `0.2.0-beta`。 |

**验证状态**：⏳ **待用户编译验证**（AI 按铁律未执行任何 cargo 操作）。需要用户跑 `cargo build --release`，并重点确认：

1. BUG-001/004：水锅读数 1/2/3；空锅/空容器/空堆肥桶 → 编译中止、状态 Stopped、终端无输出、聊天窗口报英文错误与坐标；
2. BUG-002：`/game`、`/gm`、`/plot lo` 的 Tab 补全；
3. BUG-003：`MCHPRS_IO/test.py` 能注册成功且 500 组向量全过（`dy` 符号、位映射与原点 Y 均已改）。

**已知未处理**：第 4 节列出的其它 panic 点（用户明确不改）。

**注意**：BUG-003 是破坏性协议变更——任何依赖旧位序（ZYX / `dy` 为正）的外部脚本都必须按新规则改写。

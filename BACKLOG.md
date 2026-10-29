# MCHPRS_IO 项目台账

> 本文件是本项目**唯一真相源**。2026-10-28 全文重写：条目按主题归类、统一重新编号（从 `BUG-001` 起），
> 旧台账（含 0.1–0.4.3 历史与 0.5.0-alpha 讨论过程）归档在 `~/.ssset/backup/BACKLOG.pre-rewrite-2026-10-28.md`，
> 更早的档案在 `~/.ssset/backup/BACKLOG.archive-2026-10-27.md`。
>
> **最后更新**：2026-10-28

---

## 0. 协作约定

**五条铁律**（用户 2026-10-25 下达，优先级高于一切效率诉求）

1. **目标先行**：写代码前必须完全明确用户目标，不明确就反问，禁止猜测与脑补。
2. **只记录不改码**：功能 / 改进 / bug 一律先登记进本台账；**只有用户明确说"写代码"才动代码**，其余时间只能读代码。
3. **写码与编译分离**：说"写代码"才写，说"编译"才跑 `cargo`，两步不自动衔接。
4. **全程中文汇报**：正在做什么、做完什么、怎么做，随时讲清楚。
5. 第 1 条最重要，确认语义永远优先于推进速度。

**附加约定**

- 问题一次问清，优先多选；复杂任务用台账逐项跟踪，完成一项记一项。
- 新建的临时文件一律放 `~/dsh/scratch/`，不留散落文件。
- 写含单引号的源码必须走 Python 脚本（`chr(39)` 构造），直接用 shell heredoc 会剥掉引号。
- 状态标记：`✅ 已完成` / `📝 已定待写码` / `🚧 进行中` / `📋 待做` / `⛔ 作废` / `⏳ 等用户`。
- **以后新增报错 / 新功能**：先判断能不能并进现有条目——同一机制、同一处行为、只是补充细节或换个现象，就并进对应条目（追加说明、必要时改状态），**不新开编号**；只有当它跟现有 19 条都不属于同一回事时，才新增条目，编号接着往后排（当前最后一条是 `BUG-019`）。合并还是新增拿不准时先问，不要自己定。

---

## 1. 环境与产物

| 项 | 值 |
| --- | --- |
| 运行环境 | Android（vivo V2338A / Android 16）上的 Ubuntu 24.04 容器，aarch64 |
| Termux 通道 | SSH 端口 8022，`tx '<命令>'` 直连；sshd 不随重启自启 |
| 仓库 | Termux 侧 `~/mchprs/source/MCHPRS_IO`，git 分支 `main`，远程 `https://github.com/xanax2112/MCHPRS_IO.git` |
| 上游对照副本 | `~/mchprs/source/MCHPRS`（只读，基线 `master` @ `d492e43`） |
| 线上二进制 | `~/mchprs/mchprs`，**0.5.4-alpha**（8,572,208 字节，sha256 `ca895885…`，2026-10-29 14:17 构建）；旧版备份在 `/storage/emulated/0/termux/old/`：`mchprs-0.5.3-alpha-e`（8,574,720）、`-d`、`-c`、`-b`、`-a`、`mchprs-0.5.2-alpha`、`mchprs-0.5.1-alpha-b` |
| 服务器运行目录 | `/storage/emulated/0/termux/MCHPRS_IO/`（`Config.toml`、`io.txt`、`memory/`、`logs/`、`world/`） |
| 归档目录 | `/storage/emulated/0/termux/old/`（2026-10-28 起）：`backup-2026-10-28/`（旧版本二进制、改动前源码、台账快照）、`scratch-2026-10-28/`（构建日志与临时脚本）、`drafts-2026-10-28/`（旧 rom 草稿） |
| 最近提交 | `7a36df6`（0.4.2+0.4.3 合并提交，已推送）；0.5.0-alpha 与 0.5.2-alpha 的改动**全部未提交** |
| 构建 | `cargo build --release`，aarch64 Termux；`/tmp` 不可写，日志写 `~/dsh/scratch/`。**只改源码时约 2 分钟**（只重编 `mchprs_core` 与根包）；**改 `Cargo.toml` 的版本号时约 8 分钟**——15 个 crate 全是 `version.workspace = true`，版本参与 Cargo 指纹，等于全量冷编（2026-10-29 实测：2 个 crate = 2m06s，13 个 crate = 7m42s）；release 开着 `lto = "fat"`，最后链接阶段最贵 |

---

## 2. 条目清单

> 2026-10-28 四次整理：恢复 0.1–0.4.3 的历史条目并按主题压缩，编号压到 **BUG-001…020**。
> 旧编号不再使用，也不提供对照表；完整旧台账在 `old/backup-2026-10-28/BACKLOG.pre-rewrite-2026-10-28.md`。
> `✅` = 已完成；`🚧` = 部分待做；`📋` = 待做；`⏳` = 等用户。

### 2.1 配置（A）

| 编号 | 条目 | 状态 |
| --- | --- | --- |
| BUG-001 | 配置项整理：`performance` 布尔化 + 旧键迁移 | ✅ 已完成 |
| BUG-021 | `Config.toml` 新增 `betterio` 开关（管按钮恢复 2 tick + 红石灯同活板门） | 🚧 0.5.3-alpha 已实现，待实机验证 |

**BUG-001**

- `mode = "exact"/"performance"` → 布尔 `performance`（默认 `false` = exact，可整行省略）。
- 旧键 `mode` 读到即忽略，启动时**删除该行**并补写 `performance = false`，旧值不参与换算；非法值（如 `"yes"`）**启动直接报错**，不回退。
- `automation_port` → `io_port`（默认 25585，绑 `127.0.0.1`）；`automation_data_port_events`、`automation_per_tick_push`、`auto_redpiler` 启动时移除；其余键（`schemati`、`block_in_hitbox`、`velocity` 等）保留。
- 连带作废：旧「配置写错导致 plot panic」——布尔值不需要引号，该错误类型从根上消失。

**BUG-021**
- 实现状态（0.5.3-alpha，未提交）：`Config.toml` 新增 `betterio`（默认 `false`，缺键自动补写）；编译前把该值推进 `mchprs_redpiler::set_betterio`；按钮释放 `betterio ? 2 : 10`；红石灯在 `betterio` 下走活板门路径。启动实测：`betterio = false` 已写进新生成的 `Config.toml`；2 tick / 灯行为待实机电路验证。

- 用户 2026-10-28 提出：在 `Config.toml` 新增一个 `betterio` 开关（布尔）。
- 已确认（用户）：默认值 `false`；只做配置项，改配置文件后重启生效，不加控制台命令、不加协议报文。`false` 时与现在的代码行为完全一致。
- 已确认（用户）：`betterio = true` 时管两件事——
  1. 按钮按下后的恢复时间：**10 tick → 2 tick**。
  2. 红石灯的行为**与活板门一致**（亮、灭都在当 tick 生效）。
- 代码位置（读码事实，未改动）：按钮在 `crates/redpiler/src/backend/direct/mod.rs:224`（`on_use_block`）与 `:328`（`try_use_block`）排 `schedule_tick(node_id, 10, ...)`，由 `tick.rs:60` 释放；红石灯在 `crates/redpiler/src/backend/direct/update.rs:74-80` 排 `schedule_tick(..., 2, ...)`，由 `tick.rs:54` 熄；活板门在 `update.rs` 的 `NodeType::Trapdoor` 分支直接 `set_node`。
- 已定位的改动点（仅供估算，未改）：`crates/core/src/config.rs` 的 `gen_config!` 块（`performance` 之后）。
  - **修正（2026-10-29，用户发现）**：`betterio` 之前**只在已编译电路里生效**——标志只存在于 `mchprs_redpiler`，未编译时走 `crates/redstone/src/lib.rs` 的原版式模拟器，按钮固定 10 tick、灯固定 2 tick。现已把标志下移到 `mchprs_redstone`（`set_betterio` / `betterio`），未编译路径的按钮（`lib.rs:442` 附近）与红石灯分支也读该标志；`mchprs_redpiler` 改为转调同一标志，`server.rs` 启动时按 `CONFIG.betterio` 设一次。已编译、未编译两条路径现在一致。
- 已确认（用户）：生效时机**与其他配置项完全一致**（启动时读 `Config.toml`，改完重启后生效），不额外引入“改完要重新编译电路”的步骤。
- 已确认（用户）：旧 `Config.toml` 缺 `betterio` 键时，启动时**补写默认值** `betterio = false`。
- 已确认（用户）：作用范围是**全局**——整个世界只剩一块地皮（p0,0），不存在按组/按地皮区分的需求。
- 已确认（用户）：与 `performance` **完全独立**——两个开关互不影响，任意组合都允许，没有互斥或联动规则。
- 已确认（用户，2026-10-28 收口）：**`betterio` 目前只控制按钮与红石灯这两项**，没有别的元件跟随；后续要加再开新讨论。

### 2.2 运行模型（R）

| 编号 | 条目 | 状态 |
| --- | --- | --- |
| BUG-002 | 单地皮与无人 tick | ✅ 已完成 |

**BUG-002**：只有一块地皮 **p0,0**（512×512，坐标 −256…255），永远常驻、永不卸载，玩家走出边界也不再被踢到邻区（空岛模型）。**没有玩家也照常 tick**，不再要求在线玩家；电路不会自动编译（见 BUG-016）。

### 2.3 时间推进（B）

| 编号 | 条目 | 状态 |
| --- | --- | --- |
| BUG-003 | tick 模型与 `/tick` 命令 | ✅ 已完成 |
| BUG-004 | 手动步进与速度控制（`/rtps`、`/radv`） | ✅ 已完成 |

**BUG-003 tick 模型与 `/tick`**

- tick：32 bit，回绕按取模；**每次编译清零**。`exact` 每 tick 采样一次，`performance` 每批末尾采样一次。
- `/tick`：只读显示 `Tick: N`；未编译回红字「没有 tick」；其它子命令 → 用法错误。
- `/tick seq`：**已编译**时执行 → 踢出所有脚本 + rtps 设为 0 + 进入预约模式（含独占锁）。
- 未编译 → 拒绝（`/tick seq needs a compiled circuit; compile first.`），不踢人、不改 rtps；`performance = true` 同样拒绝。
- 重复执行 → **重新踢人 + 清空预约**（不幂等）；重新编译 / `/redpiler reset` → 自动退出预约模式。
- 预约模式期间第一个重连的脚本锁定为唯一写入者，其余连接直接关闭；独占脚本断开则解锁且不放行下一个。
- 三参数 `INPUT <name> <offset> <hex>` 只对锁定连接有效，其它情况回 `ERR mode`；排队上限 255 条，同一 tick 同组后发覆盖。

**BUG-004 `/rtps` 与 `/radv`**

- 普通 `/rtps 0`：只改速度，**不踢人、不锁定、不授予预约**；此状态多脚本可同时连接。
- `/rtps <n>`（`n > 0`）或 `/rtps unlimited`：退出预约模式并解锁，不踢人；参数只能是数字或 `unlimited`。
- `/radv <n>`：原 mchprs 语义，**任何速度下都能用**，`n` 可为 0，无上限；`/radvance` 别名保留（玩家侧），终端与玩家一致；批量推进时逐 tick 追帧。
- 脚本侧手动步进是动词 `RADV`（见 BUG-005），要求 `rtps = 0`。

### 2.4 脚本协议（C）

| 编号 | 条目 | 状态 |
| --- | --- | --- |
| BUG-005 | 脚本接口收紧：禁 `/`、`RADV` 动词、无 rtps / 编译权 | ✅ 已完成 |
| BUG-006 | 错误处理：码小写 + 三分类 + `/io list` | ✅ 已完成 |
| BUG-007 | 报文、动词与位序 | ✅ 已完成（0.5.2-alpha 二次构建） |

**BUG-005 权限收紧**

- 脚本发**任何** `/` 开头的行 → `ERR unknown`，脚本侧 `/` 命令通道整体删除（该通道原本是 0.3.0 加的，现只保留给玩家与终端）。
- `/radv` 改为动词 **`RADV <x>`**，**只收大写**，回 `OK RADV <x> <tick>`；`radvance` 不保留；要求 `rtps = 0`，否则 `ERR mode`。
- 脚本**不能修改 rtps**（无 `RTPS` 动词）、**不能发起编译**：`handle_script_command`、`script_compile`、脚本侧 `ERR disabled` 回执路径全部删除。

**BUG-006 错误处理**

- `ERR` 前缀大写、码全小写：`unknown` / `arg` / `name` / `disabled <组名>` / `nocompile` / `range` / `hex` / `form` / `mode` / `full` / `not_output` / `no_io` / `line` / `encoding`；`WARN` 同样（当前不产生）。
- 一条报文多个错误只回第一个；`OK` / `RADV` / `NOW` / 动词名 / 组名不动。
- 三分类：日志类（ROM/RAM/`io.txt` 文件问题）只写 `logs/`；玩家类（编译校验）chat 红字给发起者；脚本类（报文错误）一行 `ERR <小写码>`，不落日志。
- 禁用粒度：ROM/RAM 整文件；`io.txt` 文件级错整文件、单块错只禁该块。
- `/io list`：英文，报组数与格式错误行号（整文件计、不截断），已编译时追加禁用数与组名；存在模电组时追加 `; analog groups require -d -c when compiling`。

**BUG-007 报文、动词与位序**

- 动词：`HELLO`（服务端主动）、`INPUT`、`POUT`、`OUTPUT`、`READ`、`TICK NOW`、`RADV`；客户端不发 `HELLO`，发了按未知动词。
- **动词只接受全大写**（2026-10-28 用户定）：`input` / `Input` / `radv` / `tick now` 一律 `ERR unknown`。
- 已删除：`PIN`、`SUB`、`UNSUB`、`EVENT`、`TICK STEP`、`USE`、`SET` 及旧数据端口坐标语法，一律 `ERR unknown`。`POUT` 不再带参数，绑定成功直接推一条当前值。
- 位序：整体当一个无符号整数、前端补零、**列表第一个坐标是最显著位**（0.4.0 从「按位补零」改成整数前端补零）；`matrix` 按逐行补零后的位数；模电每点一个十六进制位。
- hex 严格度：脚本侧严格（必须等于 `ceil(位宽/4)`，否则 `ERR hex`），memory 文件侧宽松（右对齐）；补零位不占位号。

- **重复 `POUT`（用户 2026-10-28 定）**：对一个**已经 `POUT` 过的端口**再发一次 `POUT`，视为**再取一次当前值**——服务器回一条当前的 `OUTPUT <name> <tick> <hex>`；**绑定关系与其它行为都不变**（不重复订阅、不清订阅、不改变后续推送规则）。
  - 现状（读码事实）：`crates/core/src/plot/mod.rs:924-947` 的 `handle_bind` 对已绑定组直接返回 `AutomationResponse::CommandOk`，而 `CommandOk` 在 `automation.rs:386` 是**什么都不发**（连 `OK` 都没有）——所以现在是“静默幂等、不回当前值”，与新要求不符。
  - 需要同步：`docs/protocol*.md` 里 `POUT` 那节现在写的是“订阅并立刻回一条当前值”，要补上“重复 `POUT` 也会再回一条当前值”。
  - 实现状态（0.5.3-alpha，未提交）：重复 `POUT` 现在回一条当前 `OUTPUT`，绑定不变；文档同步留待后补。

### 2.5 io.txt 与信号（D）

| 编号 | 条目 | 状态 |
| --- | --- | --- |
| BUG-008 | 时钟与推送规则 | ✅ 已完成（0.5.4-alpha：`width = 0` 端口，hex 固定 `0`，无时钟则禁用） |
| BUG-009 | `io.txt` 解析、校验与坐标复用 | ✅ 已完成 |
| BUG-010 | 模电组、红石粉与点状粉判定 | ✅ 已完成（含模电 `matrix:`，0.5.2-alpha） |
| BUG-024 | `io.txt` 输入组白名单支持按钮（脉冲语义） | 🚧 0.5.3-alpha 已实现，待实机验证 |

**BUG-008 时钟与推送**

- 输入组 `INPUT` 后脉冲一次 `clk`，数据与脉冲同在生效 tick。
- 输出组：无 `clk` 时任一位变化就推 `OUTPUT`；有 `clk` 时只在按钮上升沿推。
- 时钟不得与端口坐标重合；io 组之间 `clk` 唯一，重复者拒绝；io 组与 memory 的 `clk` 撞车**只禁用 io 组**。

**`width = 0` 的时钟端口（2026-10-29 用户定，0.5.4-alpha 已实现）**

- `width = 0` 且 `clk` 写了具体坐标 → **合法端口**：没有数据点，只有时钟有意义。
  - `INPUT <name> 0`：`<hex>` **固定写 `0`**，语义是"按一次按钮"，即脉冲一次 `clk`。
  - `OUTPUT`：只在 `clk` 上升沿推送，`<hex>` **固定为 `0`**（含 `POUT` 绑定确认那一帧）。
- `width = 0` 且 `clk = 0, 0, 0`（没有时钟）→ **无意义，编译后校验时整组禁用**并给 reason；`INPUT` 与 `POUT` 都回 `ERR disabled`。此状态可由 `/coord in` 之后的空组自然出现，不必在加载期报错。
- 实现差距（只读代码确认，未改）：
  1. `decode_hex`（`automation.rs:720-728`）位宽 0 时期望 **0 个** hex 字符，而行协议按空白分词写不出空参数 → 现在 `INPUT <name> ` 直接 `ERR arg`；改为"位宽 0 时只接受恰好一个 `0` 字符"。
  2. `encode_hex`（`automation.rs:676-716`）对空位向量返回空串，推送成 `OUTPUT <name> <tick> `（行尾空格）无法解析；改为输出 `0`。
  3. `IoGroup::write`（`io.rs:125-160`）空组照样脉冲 `clk`，数据通道不用改。
  4. 校验落点：`check_group`（`io.rs:283-345`）末尾加"无时钟且位宽 0 → Err"分支。
  5. 文档连带：`docs/protocol*.md` 的 hex 位数规则、`docs/file-formats*.md` 的 `width = 0` 说明。
- ✅ **已实现（0.5.4-alpha，2026-10-29 14:17 构建并安装）**：
  - `decode_hex` 开头加分支：位宽 0 且非 matrix 时，`<hex>` 必须恰好是 `0`，其余一律 `ERR hex`。
  - `encode_hex` 开头加分支：位宽 0 且非 matrix 时恒返回 `0`（修掉原先推送 `OUTPUT <名字> <tick> `、脚本无法解析的问题）。
  - `check_group` 末尾加分支：`bits() == 0` 且 `clk` 为 `None` → `Err("`width = 0` needs a clock (`clk = x, y, z`), ...")`，编译后整组禁用。
  - 文档：`protocol*.md` 的 hex 节、`file-formats*.md` §2.2 / §2.4 / §2.5 均已同步。

**BUG-009 `io.txt` 解析与校验**

- 单文件 `io.txt`（缺失不报错）；块头 `input/output <组名> = [`；必填 `origin` / `clk` / `width`；`origin` 加到所有坐标（含 `clk`）。
- 坐标：单点、步进（`±step, count`，1–255 / 1–1024）、`matrix`（1–3 段、行优先、逐行按 4 bit 补零），复用 memory 的 `parse_list` / `expand_matrix`。
- 组名 `[A-Za-z0-9_]+`，大小写敏感、全局唯一；注释 `//`、`#`、`/* */`；点数不设旧协议 16000 上限。
- 可选版本行（2026-10-28 用户定）：`io.txt` 第一行可以写 `v<版本> hex words addressed`，**0.5.0 及以后**接受，低于 0.5.0 禁用整个文件；不写版本行照旧加载。
- 加载期查语法与 `width`，编译后查元件类型与坐标，失败**整组禁用**，其余照常；行号按整个文件计。

**BUG-010 模电与红石粉**

- `analog_input` / `analog_output` 块；模电列表**必须同时写** `rs_width`（点数）与 `width`（总位数 = `rs_width × 4`），关系不符即禁用（2026-10-28 用户定）；数电组仍只用 `width`（= 点数）。每点 4 bit（0–15），hex 每点一位。（`matrix` 的模电支持见下一条。）
- 点判定 `is_dot`（四面不连）或 `is_cross`（四面相连的十字）；模电**输出**点额外要求 `has_inputs == false`；点需 `-d`、十字需 `-c` 编译。
- 红石粉：只识别孤点；读不限（有入边也能读），写只允许入边为 0 的孤点；写入只能整组 `INPUT`，无单点写强度命令。
- **已实现**（0.5.2-alpha）：`matrix:` 支持模电（2026-10-28 用户提出，规则已定）。现状：`parse_list` 里 `if analog && matrix.is_some()` 直接报 `a matrix: list cannot be analog`，`analog_input` / `analog_output` 与模电 memory 端口都用不了。已定：① `io.txt` 与 memory 两边都支持；② 展开规则与数电完全一致（单行、1–3 段、第一段最快、同一轴可重复、步长 1–255、数量 1–1024）；③ **一个点位算一个十六进制位**，matrix 下总点数 = `count1 × count2 × count3`，hex 长度 = 该点数，行内不补零；④ `rs_width` 与 `width` 的语义不变（`rs_width` = 总点数，`width` = `rs_width × 4`）。文档在实现后同步。

**BUG-024 `io.txt` 输入组白名单支持按钮**
- 实现状态（0.5.3-alpha，未提交）：输入白名单加入 `IoElement::Button`，报错文案改为 `lever, pressure plate or button`；`try_set_powered` 对按钮按“写 1 脉冲、写 0 无为”处理。待实机验证。

- 用户 2026-10-28 提出：`io.txt` 输入的白名单要支持按钮。
- 当前只登记，**未改任何代码**（铁律 2/3）。
- 现状（读码）：`crates/core/src/io.rs:315` 输入组点位白名单是 `IoElement::Lever | IoElement::PressurePlate`，报错文案 `is not a lever or pressure plate`；按钮目前只允许出现在 `clk` 位置（`io.rs:340`），不能当输入组点位。
- 代码连带（不能只改白名单）：`crates/redpiler/src/backend/direct/mod.rs:352` 的 `try_set_powered` 只处理 `Lever | PressurePlate` 并返回 `true`，按钮落进 `_ => false`；所以放开白名单后，写按钮仍会被判成“没写进去”。必须同时让写路径支持按钮（按下 + 按 10 tick 预约恢复，`betterio` 打开时为 2 tick，见 BUG-021）。
- 已定（2026-10-28，用户选择）：**按 B 方案「当脉冲用」**——写 1 触发一次按下（随后由方块本义在 10 tick / `betterio` 打开时 2 tick 后自动弹起），写 0 不产生动作；不提供“强制维持高电平”。
- 已定（2026-10-28）：**输出组白名单不加按钮**，仍是 `Lamp | Trapdoor | NoteBlock`。
- 已定（用户 2026-10-28）：**禁止同一点双角色**——同一个按钮不能既当 `clk` 又当数据点位，沿用现有“`clk` 不得与端口坐标重合”的校验。
- 待办细节（动手时处理）：①错误提示文案要从 `is not a lever or pressure plate` 改成包含按钮；②文档同步（`docs/protocol*.md`、`file-formats*.md`）；③按下期间重复写 1 仍会被 `if powered { return; }` 吞掉（不续时），这是预期行为。
- 关联 BUG-021（`betterio` 决定按钮恢复时间）与 BUG-008（`clk` 上升沿规则）。

### 2.6 存储器（E）

| 编号 | 条目 | 状态 |
| --- | --- | --- |
| BUG-011 | 存储器文件：ROM / RAM 与模电 | ✅ 已完成 |

**BUG-011**

- 首行 `v<版本> hex words addressed`：**0.5.0 及以后**接受（只扫第一行的版本号，后缀不管），低于 0.5.0 拒载；每端口列表首项 `width = N`；坐标行支持单点、步进、`matrix`；`clk` 与 `origin` 必须写全，`origin` 加到所有坐标。
- **ROM**：按键 `address coordinate` / `data coordinate`，数据来自 Logisim hex 文件（0.4.0 引入，0.4.1 稳定）。
- **RAM**：额外 `write enable` / `read enable` / `byte write enable` / `write data coordinate` / `read data coordinate`；`clk` 必须指向真实方块。
- RAM 只**事件驱动**落盘（玩家退出、服务器退出、`/redpiler reset`、编译崩溃、`/memory reload` 之前），运行期不写盘；回写格式与 Logisim 兼容（每行 8 word、按 8 对齐、全空块省略）。
- 校验：同一存储器任意两位落同一坐标报错（跨键也查）；拉杆与地址点相邻即判冲突（0.4.1）；加载期查文件、编译后查元件，任一失败整文件禁用，未落盘数据丢弃。
- 模电：四个角色各选数电或模电（`analog address coordinate` 这类，空格分隔无下划线）；输入点只要是点状粉即可，输出点不得被其它信号驱动、允许比较器读取；每个角色只能出现一个列表，数模按**每条线**选。

- **启动初始化（用户 2026-10-28 新增需求，并入本条）**：
  - 服务器启动时若**没有 `memory/` 目录，就自动创建**。
  - 同时生成 `memory/.rom.txt` 与 `memory/.ram.txt` 两个**初始模板文件**（文件名**字面以点号开头**，Linux 下即隐藏文件；只是模板，**不参与加载**）。
  - 启动时若没有 `io.txt`，也**自动生成**；生成的 `io.txt` 开头必须有版本信息，写法为**只写版本号**：`v0.5.2-alpha`（不带后面的 `hex words addressed`）。
  - 现状（读码事实）：`crates/core/src/server.rs:163-165` 启动只建 `./world/players`、`./world/plots`、`./schems` 三个目录，**不建 `memory/`、不建 `io.txt`**；memory 加载器按 `*.rom.txt` / `*.ram.txt` 扫描 `memory/`，所以落地时要保证点号开头的模板文件**不会被当成存储器加载**。
  - 已定（用户）：模板内容是**骨架，不带任何注释**——基本格式的键全部写出来，`width` / `clk` / `origin` 都填 0；其中 **RAM 模板的 `clk` 填 `0, 0, 0`**。
  - 参考格式（`docs/file-formats_zh.md` §3.2 / §3.3）：文件级键是 `origin` 与 `clk`，端口列表内首项是 `width = N`；`clk = 0` 表示无时钟。**注意**：memory 的 `clk` 必须是**输出类元件**（红石灯 / 铁活板门 / 音符盒，服务器读它的电平），不是按钮——按钮只用于 `io.txt` 的 `clk`（服务器去按它）。此处早期记录写成"按钮"，2026-10-29 已更正。
  - 已定（用户）：`.rom.txt` 模板的 `clk` 写 `clk = 0`（无时钟）；`.ram.txt` 写 `clk = 0, 0, 0`。
  - 已定（用户）：自动生成的 `io.txt` = 版本行 `v0.5.2-alpha` + **一个 `input` 组和一个 `output` 组的骨架**（`origin = 0` / `clk = 0` / `width = 0`），**组名分别为 `none_in`（input）与 `none_out`（output）**。
  - 已定（用户）：骨架里 `width = 0` 的端口列表**不写坐标行**。
  - 已定（用户）：因 `io.txt` 要求**组名全局唯一**（BUG-009），骨架改用可区分的名字——**`none_in` / `none_out`**，不改校验规则。
  - **模板不可加载（用户 2026-10-29 指出，已实测确认）**：`.ram.txt` 模板里的 `write enable = [ width = 0 ]` / `read enable = [ width = 0 ]` 是错的——这两个键在解析器里是**单坐标**（`memory.rs:1163-1195`，`parse_single_or_triple`），写到真实文件名下会被拒绝：
    ```text
    Memory t.ram.txt rejected: line 7: `write enable`: `[` is not a number
    ```
    - 另外：`write enable` / `read enable` 写 `0` 也不合法（`must be a coordinate, not 0`）；`byte write enable` 才是列表（`parse_list`，带 `width`）；`address coordinate` 的 `width` 必须 > 0（`memory.rs` 校验），所以模板里的 `width = 0` 同样不合法。
    - 结论：现有模板**整体不可加载**，只因点号开头被加载器跳过才没报错。
  - **恢复的遗失规则（2026-10-29 从 `old/backup-2026-10-28/BACKLOG.archive-2026-10-27.md` 找回）**——是我重写台账时丢了这条，不是新需求：
    - `write enable` / `read enable`：**必须写**，且是**单坐标**（`0, 0, 0` 形式），**不接受 `0`**。
    - `byte write enable`：**宽松**——`byte write enable = [ width = 0 ]` 表示**没有字节写使能**（写使能只受 `we` 控制）；旧版允许写空列表 `[]`，后来废止，统一写 `[ width = 0 ]`。
    - 带坐标时，`byte write enable` 的条数必须严格等于 `ceil(数据位宽 / 8)`，否则报错并禁用该 RAM。
    - 实测（0.5.3-alpha 二进制）：`we = 1, 1, 1` + `re = 2, 1, 1` + `byte write enable = [ width = 0 ]` 能正常解析；报错只会来自其它键（如 `write data coordinate` 的 `width` 与坐标数不符）。
    - 更正：我先前把“`byte write enable = 0`”记成新需求，属于误记；模板里的 `[ width = 0 ]` 本来就是正确写法。
    - **`bwe` 只在 `we = 1` 时生效（用户 2026-10-29 确认，与现实电路一致）**——现状代码已符合：`crates/core/src/memory.rs:410-418` 先算 `write_enabled`，`if !write_enabled { return; }` 直接返回，之后才读 `byte write enable` 并 `merge_word`；`enables` 为空（`[ width = 0 ]`）时整字写入。另有一层运行期保护：`enables.len()` 与 `ceil(width/8)` 不符时静默不写（加载校验本应提前拒绝）。
  - 已定（用户 2026-10-29）：模板**只要求语法形式正确**，不要求能被加载：
    - `write enable` / `read enable` 改成**单坐标**写法：`write enable = 0, 0, 0`、`read enable = 0, 0, 0`；
    - `byte write enable` 保持**列表**写法（`[ width = 0 ]`）；
    - 各列表的 `width = 0` 保留（语法正确，但加载时仍会因 `width` 必须 > 0 被拒，这是“示意用”的预期）；
    - 顺序按前一条：三个使能块排在 `address coordinate` 上面。
  - 实现（2026-10-29，已编译未提交）：`RAM_TEMPLATE` 改为 `write enable = 0, 0, 0` / `read enable = 0, 0, 0` / `byte write enable = [ width = 0 ]`，并把三个使能块排在 `address coordinate` 之前；`ROM_TEMPLATE` 无使能，未改。二进制已确认包含新写法。
  - 新增调整（用户 2026-10-29，**先不写代码**）：`.ram.txt` 模板里 `write enable` / `read enable` / `byte write enable` 三个块要**放在 `address coordinate` 上面**。
    - 当前模板顺序：`address coordinate` → `write data coordinate` → `read data coordinate` → `write enable` → `read enable` → `byte write enable`。
    - 目标顺序：`write enable` → `read enable` → `byte write enable` → 其余（`address coordinate` 等）。
  - 实现状态（0.5.3-alpha，未提交）：启动创建 `memory/` 与 `.rom.txt` / `.ram.txt` 隐藏模板、`io.txt` 骨架（版本行 `v0.5.3-alpha`）；加载器跳过点号文件；`io.txt` 的版本行现在也接受**只写版本号**；`width = 0` 的空组不再报错而是作为空组加载。启动实测三个文件都正确生成。

### 2.7 终端、命令与其它修复（F）

| 编号 | 条目 | 状态 |
| --- | --- | --- |
| BUG-012 | 终端支持 `/` 指令 | ✅ 已完成 |
| BUG-013 | 命令表与其它历史修复 | ✅ 已完成 |
| BUG-023 | 指令写法升级（`//schem load|save|list`、`/tick freeze|unfreeze|step|rate`）+ Tab 补全 + 派发单一来源 | 🚧 0.5.3-alpha 已实现（补全仍为表驱动、派发未合并），待实机验证 |
| BUG-022 | 木斧取点后用 `/coord in … / add … / new …` 把坐标写入 `rom.txt` / `ram.txt` / `io.txt` | 🚧 0.5.3-alpha 已实现，待实机验证 |

**BUG-012**：stdin 读取，带 `/` 前缀才执行，权限与游戏内一致；需要玩家的命令（`/tp`、`/gamemode`、`/container`、`/speed`）提示 `This command needs a player and cannot be used from the console`；新增 `/stop` 优雅关服；`/tick`、`/tick seq`、`/rtps`、`/radv`、`/io list`、`/memory reload`、`/redpiler` 均可用。

**BUG-012 新增需求：`/io reload`（用户 2026-10-28）**
- 实现状态（0.5.3-alpha，未提交）：`/io` 现在玩家与控制台都支持 `list` / `reload`；`reload_io` 与 `/memory reload` 同流程（`-i` 下拒绝、编译态先重置再读）。控制台实测：`/io reload` 返回 `Reloaded 2 IO group(s).`。
- 要求：支持 `/io reload`，运行期重新读取 `io.txt`，不用重启进程。
- 语义（用户已定）：与 `/memory reload` 一致——先落盘 → **退出编译态** → 重读 `io.txt` → 停住等玩家再编译。
- 归属（用户已定）：并入本条（终端指令）。
- 现状（读码事实）：`/io` 只有 `list` 一个子命令（`crates/core/src/plot/commands.rs:240-246`，其它输入回 `Usage: /io list`）；而且该分支只在**控制台/自动化**处理器里，游戏内玩家指令 `match` 里**没有 `io`**，玩家敲会得到 `Command not found!`，Tab 补全树里也没有 `io`。
- 已定（用户 2026-10-28）：`/io`（含 `list` 与新增的 `reload`）**控制台与游戏内都能用**，并**补进 Tab 补全树**（顺带把一直缺的 `io` 补上）。
- 权限：沿用现有管理类玩家指令的做法（`Player::has_permission`；未启用权限系统时默认允许，见 `crates/core/src/player.rs:495-507`）。具体权限节点名若要单独设（如 `mchprs.io.reload`）再定。
- 已定（用户 2026-10-28）：**`/io reload` 与 `/memory reload` 只在未编译模式生效**；若当前处于编译模式，则**先退出编译态**再生效；**用 `-i` 编译的电路下 reload 不生效**。

**BUG-013**：`/gamemode` 等命令的 **Tab 补全**补齐（0.2.0）；`/whitelist` 因静默失败**整个功能删除**（0.2.1）；告示牌朝向修正（yaw 未归一化，0.4.3）。

**BUG-022 木斧取点写入坐标（`/coord`）**
- 新增需求（用户 2026-10-29）：**文件级被禁用的文件不出现在 `/coord in` 的补全里**——即 `io.txt` 整文件被禁用、或某个 memory 文件加载被拒时，`/coord in` 的文件列表里不显示它们。
  - 待确认：手敲完整文件名是否也拒绝（还是仅从补全里隐藏、仍允许强制写入后再修）。
  - 现状（0.5.3-alpha 事实）：`/coord in` 的文件与组名补全目前**只是静态节点**（补全树里 `Parser::String(0)`，见 `commands.rs` 的 `/coord in [file]` 节点），**并没有真的列出目录/组名**；动态补全（含本条的“过滤被禁用文件”）尚未实现。
- **修复（2026-10-29，用户实测发现；2026-10-29 10:01 编译、已安装）**：
  1. `/coord new|add` 的宽度校验原先写成 `if width != 0 && total > width`，`width = 0` 被整段豁免，导致往 `width = 0` 的组里仍能写入坐标，写出“width=0 却有坐标”的非法文件（实测：`/coord in io.txt none_in` 后 `/coord new data` 成功写入 `133, 8, 130`）。已改成**严格比较** `total > width`（io.txt 与 memory 两处），`width = 0` 表示“一个点都不允许”。
  2. `commit()` 没有把 `self.coord_dirty` 置位，导致“下一次编译自动重载被改过的文件”从未生效（`mod.rs` 里的重载分支永远不进入）。已在写盘成功后补 `self.coord_dirty = true`。
  - 状态：已编译并替换二进制（2026-10-29 10:01 构建）；源码于 2026-10-29 13:0x 提交。
- **修复 3（2026-10-29，用户实测发现；10:01 编译、已安装）**：`/coord in io.txt ` 之后的组名补全只有 `none_in`，缺 `none_out`。原因：候选取的是 `IoTable::load(...).groups`，而 `none_out` 因自身校验失败（`width = 1` 但无坐标）被**组级禁用**、不在 `groups` 里；规则只说“**文件级**禁用才不显示”，组级错误恰恰是 `/coord` 要去补坐标的场景。已改为 `io_group_candidates()` 扫描 `io.txt` **原文**的 `input <名> = [` / `output <名> = [` 行，列出所有声明过的组；文件级禁用时仍整份隐藏（由 `file_candidates` 决定，规则不变）。
- 注意：`file_candidates` 里“`io.txt` 只在 `IoTable` 至少加载出一个组时才出现”的判断暂时保留，用户未要求改。
- 实现状态（0.5.3-alpha）：新增 `crates/core/src/plot/coord.rs`（`in`/`new`/`add`/`undo`/`redo`、8 步历史、`type` 短码表、按行编辑 `io.txt` 与 `memory/*.txt`、紫色回馈）；`/coord` 改过文件后下一次 `/redpiler compile` 会重载一次。**未验证**：需要玩家在游戏里用木斧取点才能跑通，本轮只做到编译通过 + 启动验证。

- 用户 2026-10-28 提出；指令名 `/coord`；取点用 WorldEdit 木斧右键的**第二点**（`Player::second_position`）。
- **语法总表（用户 2026-10-28 口述，按原话记录）**：

```text
/coord
/coord in <file> [name]
/coord new <type> [<偏移> <个数> <方向>]
/coord add <type> [<偏移> <个数> <方向>]
/coord undo
/coord redo
```

- **`/coord in <file> [name]`**
  - `<file>` 为 `…rom`、`…ram`、`io.txt` 三类。
  - `<name>` **仅当 `<file>` 是 `io.txt` 时使用**，是该文件里的 **IO 端口**名，**不区分输入输出**。
  - **多次输入 `/coord in` 时，后一次覆盖前一次**（上下文只保留最后一次）。
  - 已确认（用户）：启动生成的隐藏模板 `memory/.rom.txt` / `.ram.txt` **不参与 `/coord in` 的补全**，也不会被当作目标。
  - 现象（用户 2026-10-29 实测）：**memory 文件被禁用后，`/coord in` 不再把它列为候选**。
    - 代码依据（修复前的 0.5.3 状态）：`coord.rs` 的 `file_candidates()` 对每个 `memory/*.rom|ram` 调 `memory::parse`，**解析失败就跳过**；因此“加载期被拒”的文件不会出现在补全里。
    - 对比：编译期校验失败（文件能解析、但坐标与电路不匹配而被禁用）的文件**仍会出现**，因为过滤只看解析结果。
    - 与已定规则的关系：用户此前定过“**文件级**被禁用的文件不出现在 `/coord in` 补全里”，所以记忆文件解析失败时隐藏是符合规则的（**该规则随后被用户改掉：memory 文件一律显示，见下**）；但组级/编译期错误仍应显示（2026-10-29 已按此修 `/coord in io.txt` 的组名补全）。
    - 已定（用户 2026-10-29）：**memory 文件一律显示**——只要文件存在（非点号开头）就出现在 `/coord in` 补全里，**解析失败也显示**，因为 `/coord` 是按文本编辑的，能用来修。
    - ✅ 已实现（2026-10-29 13:00 编译、13:01 安装）：`file_candidates()` 已去掉 `memory::parse` 过滤，只保留“存在 + 非点号开头 + `.rom`/`.ram` 后缀”；注释同步更新。待实机确认。
    - 注：`io.txt` 的显示规则不变（文件级禁用时整份隐藏）。
- **`/coord new <type> [<偏移> <个数> <方向>]`**
  - `type` 是**短码**；**字节写使能不做特殊规定**。
  - `type` **只需要一个坐标**时，**不需要**后面的参数。
  - `type` **需要多个坐标**时：不带偏移/个数 = 只使用当前这一个点；带了 = 需要多个点。
- **`/coord add <type> [<偏移> <个数> <方向>]`**：参数用法与 `new` 相同。
- **`new` 与 `add` 的区别**：**`new` = 覆盖**，**`add` = 续写**。
  - **只需要一个坐标的 `type`，只能用 `new`**。
  - `new` / `add` 的结果**超过 `width` 时，不予执行**。
  - 文件里 **`origin` 有坐标**时，写入的坐标要**减去 origin**（服务器负责做全局坐标的逆运算，写进文件的是相对坐标）。
  - 类型是**模电**时，宽度与 **`rs_width`** 对比（不是 `width`）。
- **`/coord undo` / `/coord redo`**：撤销 / 重做一次 coord 操作。
- 其它已定：偏移 = 文件语法里 `+<n>x` 的 `n`，个数 = `count`；方向沿用 `//stack` 规则（省略 = `me` 玩家朝向，支持 `l/left`、`r/right`、`u/up`、`d/down`）；写入用**步进缩写**（一行 `x, y, z, ±n轴, 个数`）不展开；**不做写前备份**；**只能在未编译时使用**，回车直接保存；**权限同木斧选点**（`worldedit.selection.pos`）。
- 读码事实（未改动）：方向参数可复用 `ArgumentType::Direction` 与 `player.get_facing()`（`crates/core/src/plot/worldedit/mod.rs:266-305`）；`//stack` 自身不读朝向，朝向来自参数缺省 `me`。
- **`type` 短码表（用户 2026-10-28 确认，数字口用文件键缩写，模电统一 `a_` 前缀）**：

| `type` | 含义 | 对应文件键 | 坐标数 |
| --- | --- | --- | --- |
| `clk` | 时钟 | `clk` | **1**（只能用 `new`） |
| `addr` | 地址 | `address coordinate` | 多 |
| `rd` | 读数据 | ROM 的 `data coordinate`、RAM 的 `read data coordinate` | 多 |
| `wd` | 写数据 | `write data coordinate` | 多 |
| `we` | 写使能 | `write enable` | 多 |
| `re` | 读使能 | `read enable` | 多 |
| `bwe` | 字节写使能 | `byte write enable` | 多 |
| `a_addr` | 模电地址 | `analog address coordinate` | 多（对 `rs_width`） |
| `a_rd` | 模电读数据 | `analog read data coordinate`（ROM 用 `analog data coordinate`） | 多（对 `rs_width`） |
| `a_wd` | 模电写数据 | `analog write data coordinate` | 多（对 `rs_width`） |

  - **`data` 统一改名 `rd`**：ROM 的数据口与 RAM 的读数据口都叫 `rd`（用户 2026-10-28 确认）。
  - 使能类**没有模电版本**，所以 `a_` 系列里没有 `a_we` / `a_re` / `a_bwe`。
  - **`io.txt` 的 `type` 只有两个**：`clk`（单坐标）与 `data`（该组的坐标列表）——**io.txt 用 `data`、memory 用 `rd` 是有意区分**（用户 2026-10-28 确认）。
  - 已定（用户 2026-10-28）：**`type` 的取值随 `in` 进入的对象而不同**——进入 **memory 文件**时是上表那 10 个（`clk`/`addr`/`rd`/`wd`/`we`/`re`/`bwe`/`a_addr`/`a_rd`/`a_wd`）；进入 **`io.txt` 组**时只有 `clk` 与 `data` 两个。
  - 已定（用户 2026-10-28）：**Tab 补全按上下文区分显示**——`/coord new ` 与 `/coord add ` 之后补全出的 `type` 列表，取决于当前 `in` 的是 memory 还是 io.txt。
- 已定（用户 2026-10-28）：**`width` 必须在文件里写死**，`/coord` **不自动更新它**；`add` 就是靠文件里的 `width` 校验数量。
  - 已确认（用户）：`/coord new` **允许写少于 `width` 的点数**；但**编译时若实际点数与 `width` 不相等，就按现有校验禁用**（io.txt 禁该组、memory 禁该文件），不在 `/coord` 阶段拦。
- 已定（用户 2026-10-28）：`undo` / `redo` **深度为 8**，超过 8 步丢弃最旧的记录。
- 已定（用户 2026-10-28）：用 `/coord` 改过文件后，**在输入编译指令（`/redpiler compile`）时重载一次**该文件，保证编译用的是最新内容。
- 已定（用户 2026-10-28）**回馈**（紫色，即 WorldEdit 那套 `send_worldedit_message`）：
  - `/coord new|add` 成功：紫色消息，内容 = **写入的内容 + 写入到的 `type`（全码）**。
  - `/coord in` 成功：紫色消息，说明**进入了哪个文件**（io.txt 时还要带上端口名）。
- 已定（用户 2026-10-28）：回馈里的“全码”= **不带 `coordinate` 的完整词**——`address`、`read data`、`write data`、`read enable`、`write enable`、`byte write enable`、`clk`（模电相应加 `analog ` 前缀）。
- 已定（用户 2026-10-28）：回馈排版**坐标串在前、type 全码在后**，例：`10, 64, 5, +2x, 8 -> read data`。
- 当前只登记，**未改任何代码**（铁律 2/3）。
- 已确认：木斧就是 **WorldEdit 的木斧选点**（聊天框紫色提示 `Second position set to (x, y, z)`），只需要拿这个第二点。
- 已核实：本仓库自带 WorldEdit 实现（`crates/core/src/plot/worldedit/`），右键在 `plot/mod.rs:734`，坐标存 `Player::second_position`，提示在 `player.rs:450`，需权限 `worldedit.selection.pos`。
- 需求原话：能用小木斧右键点击方块，输入指令，能够把点击的坐标直接赋值给 `rom.txt`、`ram.txt`、`io.txt`。

**BUG-023 指令写法升级 + Tab 补全覆盖全部指令**
- 实现状态（0.5.3-alpha，未提交）：`//schem load|save|list`（`load`/`save` 复用 `//load`/`//save`，`list` 列 `./schems`）
- 新增需求（用户 2026-10-29）：**输入编译指令时，先自动执行一次 `//update -p`**（更新整块地皮的所有方块），再开始编译。
  - 现状依据：`//update` 在 `crates/core/src/plot/worldedit/mod.rs:733`，`-p` 的含义是“Update the entire plot”；执行体是 `execute_update`（`worldedit/execute.rs:1001`），对整块地皮逐块调用 `mchprs_redstone::update`。
  - 已定（用户 2026-10-29）：**玩家与控制台两条编译入口都执行**；**每次编译都执行**（含 `-i` 与重复编译）。
  - 已定（用户 2026-10-29）：**放在编译之后**，复用现有 `CompilerOptions::update`（`--update`）路径——`crates/redpiler/src/lib.rs:244` 在编译结束后对 bounds 逐块 `mchprs_redstone::update`，不新增第二套遍历逻辑。
  - 已定（用户 2026-10-29）：扫描范围**只扫非空 section**——现有 `for_each_block_mut_optimized`（`crates/world/src/lib.rs:194-217`）已经 `block_count() == 0` 就跳过整段，所以天然满足，无需改动。
  - 更正：此前我说“整块地皮 512×512×384 ≈ 1 亿次 update”是错的——空 section 会被跳过，实际只遍历有方块的 section；真实成本需要在有电路的世界实测。
  - 已定（用户 2026-10-29）：**先只记录，等明确说“写代码”再实现**。
  - 待定（实现时确认）：自动执行是否绕过 `mchprs.we.update` 权限；是否回显一行提示。
、`/tick freeze|unfreeze|step|rate`（玩家与控制台）、补全树新增 `/tick` 四个子命令、`/io`、`/coord` 与 `/redpiler compile` 的 10 个长参数（含 `--passes=`）。已用控制台实测 `freeze`/`unfreeze`/`rate`/`step` 奇偶校验。**未做**：派发仍是 `match`，只有补全由扩展后的节点表驱动，完整“单一来源”重构未进行。

- 用户 2026-10-28 提出，原话：「指令写法升级，顺带 tab 键补全支持所有指令」。
- 当前只登记，**未改任何代码**（铁律 2/3）。
- 已核对事实（读码得出，未改动）：指令实现分两处**手工维护**——派发在 `crates/core/src/plot/commands.rs` 的 `match` 分支（含 `/plot`、`/redpiler`、`/memory`、`/tick`、`/rtps`、`/radv`、`/io`、`/stop`、`/speed`、`/gamemode`、`/container`、`/worldsendrate`、`/version` 等），Tab 补全树是同一文件里约 900–1290 行**写死的 Node 数组**，末尾再拼上 `worldedit::add_command_completions`；两处不同源，已经漂移。
- 漂移现状（初步核对，动工时逐个校准）：补全树**缺** `/info`、`/claim`、`/io`、`/rtps`、`/radv`、`/stop`、`/plot`、`/speed` 等；补全树**残留已删除**的 `/whitelist`（BUG-013 已整功能删除）、`/toggleautorp`（BUG-016 已删 auto_redpiler）、`/load`；`/redpiler`、`/memory`、`/tick` 的子命令补全不全。
- 已定（用户 2026-10-28，本次「写法升级」的具体内容）：
  1. **`//schem` 命名空间**：把现有的 `//load`、`//save` 挂到 `//schem` 下，`//schem load` 与 `//load` **完全等价**（`//schem save` 与 `//save` 同理）。
  2. **`/tick` 扩展**：
     - `/tick freeze` —— 与 `/rtps 0` 等价。
     - `/tick unfreeze` —— 恢复到 **freeze 之前的那个 rtps 值**。
     - `/tick step 2n` —— 与 `/radv n` 等价，例：`/tick step 2` = `/radv 1`；**服务器不接受奇数的 `/tick step`**。
     - `/tick rate` —— 与 `/rtps` 等价。
  3. **`/redpiler compile` 的参数补全**：要补全**完整参数（长参数）**，不是短参数。
  4. **以上所有新指令都要支持 Tab 补全**；`/tick rate` 的补全默认给 `20`。
- 现状（读码事实，未改动）：
  - `//load` / `//save` 是 WorldEdit 指令表里的条目（`crates/core/src/plot/worldedit/mod.rs` 的 `"/load"`、`"/save"`），`//schem` **目前不存在**。
  - `/rtps` 无参数时打印 RTPS 报告，带参数时设置；`/radv <ticks>` 必须有参数；`/tick` 目前只有无参与 `seq` 两个分支。
  - `/redpiler compile` 的参数由 `CompilerOptions::parse` 解析，长参数有 `--optimize --export --io-only --update --export-dot --wire-dot-out --illegal-states-out --wire-cross-out --print-after-all --print-before-backend` 以及 `--passes=`，短参数为 `-o -e -i -u -d -l -c`。
- 已确认（用户，本次细项）：
  1. `/tick unfreeze` 恢复到 **freeze 之前的那个 rtps 值**（不是固定值）。
  2. `/tick step` 的奇数**直接报错**、不推进；`n` 的上限与 `/radv` 一致（u32）。
  3. `/redpiler compile` 的 Tab 补全列出上面那 **10 个长参数 + `--passes=`**；并且**输入空格后要接着继续补**（不是只在第一个参数位置补一次）。
  4. **改成单一来源**：一张指令表同时生成派发与 Tab 补全，避免两处漂移；**控制台（stdin `/`）同步支持**这些新写法。
  5. `/tick rate` **不带参数时等于设为 20**（不打印报告）；Tab 补全也提示 `20`。
     - 已确认（用户）：因此 `/tick rate` 与 `/rtps` **不是完全等价**——空参行为不同（`/rtps` 空参打印报告，`/tick rate` 空参设 20），有意为之。
  6. `//schem` 除 `load` / `save` 外，**再加 `//schem list`**（列出 schems 目录里的文件）。
- 至此 BUG-023 的规格已完整，等你下“写代码”指令。

### 2.8 编译与稳定性（G）

| 编号 | 条目 | 状态 |
| --- | --- | --- |
| BUG-014 | 编译稳定性与上游对齐 | ✅ 已完成 |
| BUG-015 | 空容器编译警告 | ✅ 已完成 |
| BUG-016 | 删除项：自动编译与演示目录 | ✅ 已完成 |
| BUG-017 | `-i` 编译与 `/memory reload` | ✅ 已完成 |

**BUG-014 编译稳定性与上游对齐**

- 比较器读到无 override 的方块时**优雅中止**并报错，不再 panic 崩溃。
- 编译真正 panic 时：保存存档 → 打印 `[CRASH]` → 退出码 1，不留半死进程。
- 自动化端口绑定失败不再静默，日志明确写出端口被占用、协议不可用。
- 编译失败会丢掉已排队的 tick —— **明确不做**（不恢复排队状态）。
- 上游对齐：`coalesce` 的合并行为、信号强度分析结果与上游一致；性能上只写变化的位、去掉每 tick 的总线 clone。

**BUG-015 空容器编译警告**

- 比较器读到空容器（空锅 / 空桶 / 空堆肥桶）时**按 0 参与编译，不中止**（上游会越界崩溃）。
- 编译成功后给**发起编译的玩家**发一条**红色**警告：列出全部受影响的比较器坐标，最多 10 个，超出部分用 `,(...)` 收尾。
- 同一批警告**合并成一条**；没有发起者（终端 `/redpiler compile`）时**静默**，只写日志。
- 历史：0.2.0/0.3.0 修崩溃并改成警告，0.4.0 改成红色 + 合并列出。

**BUG-016**：`auto_redpiler` 与自动触发整体删除（保留手动 `/redpiler compile`）；`MCHPRS_IO/` 演示目录（含 `test.py`、`io_element.py`、示例世界）删除，演示后续另补。

**BUG-017**：`-i`（`io_only`）下 `/memory reload` 被拒绝，放置/破坏方块也被拒绝并提示；编译状态下 reload = RAM 落盘 → 退出编译状态（清 `POUT` 绑定、tick 归 0）→ 重读 `memory/` → 停住等玩家再编译；非编译状态直接重读、不写盘。

### 2.9 版本与收尾（H）

| 编号 | 条目 | 状态 |
| --- | --- | --- |
| BUG-018 | 版本号、memory 首行与测试清单 | 🚧 代码/二进制已 0.5.2-alpha；memory 文件与测试清单待更新 |
| BUG-019 | 工作区改动未提交 | ✅ 2026-10-28 已原样提交并推送 `main`（`328e2ca`，29 文件 / +4429 −5977）；未打标、未发 Release |

### 2.10 遗留缺陷（I）

| 编号 | 条目 | 状态 |
| --- | --- | --- |
| BUG-020 | `io.txt` 加载错误日志里组名重复一次 | ✅ 已修（0.5.2-alpha） |

**BUG-020 `io.txt` 加载错误日志里组名重复**

现象（0.5.2-alpha 二次构建实测）：

```text
io.txt line 9: group `bad_ratio`: `bad_ratio`: `width = 6` does not match `rs_width = 1` (expected width = 4)
```

原因：`io.rs` 拼报错时又包了一层 `group {}`：

```rust
format!("group `{}`: {}", name, strip_inner_line(&message))
```

而 `parse_list` 的报错本身已经用 `{key}`（这里就是组名）开头，于是出现两层。

修法：二选一，保留一层即可——去掉 `io.rs` 外层包裹，或让 `parse_list` 不再带 `{key}`（后者会影响 memory 的日志，不动它更稳）。

影响：**仅日志观感**，功能无影响；`/io list` 输出只列行号与组数，不受这条影响。修它需要重新编译。

### 2.11 客户端兼容（J）

| 编号 | 条目 | 状态 |
| --- | --- | --- |
| BUG-032 | 基岩版（Bedrock）客户端接入 | 📋 仅完成可行性调研（2026-10-29），未安装环境、未验证、未改代码 |

**BUG-032 基岩版客户端接入（调研）**

用户 2026-10-29 提出：借助现有轮子，分析服务端与基岩版客户端连接的可行性。

- 结论：**直连不可行**——Geyser 只模拟最新 Java 版本，而本服务端 `server.rs:732` 的握手检查只放行 765（1.20.4），会被自己断开；可行链路是 **Geyser(standalone) + ViaProxy → 本服务端**，零改动我们的代码。
- 关键代码事实（约束来源）：
  - 协议固定 765、握手硬校验：`crates/core/src/server.rs:732`。
  - 自定义维度类型 `min_y = 0`、`height = 384`、不发 `skybox` / `default_clock`：`server.rs:800-825`、`crates/network/src/packets/clientbound.rs:130-150`。
  - 光照全零 + `ambient_light = 1.0`：`clientbound.rs:672-700`。
  - 只发原版包子集（约 40 个），无 Update Health / Set Spawn Position / Set Experience。
  - 聊天走 `CSystemChatMessage`（绕开聊天签名问题）：`crates/core/src/player.rs:412`。
  - `/coord`、`//load` 依赖 Brigadier `ask_server` 动态补全，Bedrock 侧无对应机制。
  - 玩家走远不卸载区块（卸载分支被注释）：`plot/mod.rs:654-656`。
- 风险分级：
  - P0：维度高度 0..383 落到 Bedrock 主世界 -64..319 之外，y>319 可能被裁；registry 抬版本时 `skybox` 等缺失字段是否被补全未知。
  - P1：动态补全失效；缺原版包导致出生点 / HUD 异常；计分板与 tab 列表显示。
  - P2：红石线 dot state 观感（Geyser 官方列为不可修复）；全零光照观感。
- 环境可行性：Termux `pkg` 可装 openjdk-21（21.0.13 aarch64）；本机 8 核 / 15GB 内存 / 62GB 空闲，足够跑两个 JVM。
- 其它路线：Rust 原生 Bedrock（bedrock-rs）等于重写 Geyser，不划算；把服务端追平最新 Java 协议工程量远大于 0.5.3 全部工作。
- 详细分析（含链路图、最小验证清单、引用）：`~/dsh/BEDROCK-CLIENT-FEASIBILITY-0.5.3.md`。
- 待定：是否落地最小验证（需用户同意装 JDK 与下载 jar）。

### 2.12 文档（J）

| 编号 | 条目 | 状态 |
| --- | --- | --- |
| BUG-034 | 删除 `manual.md` / `manual_zh.md`，把 memory 的 `clk` 行为写进 `file-formats*` | ✅ 已完成（2026-10-29） |

**BUG-034 文档整理：删 manual，补 memory `clk` 行为**

用户 2026-10-29 指示：删除两份 `manual`，在 `file-formats` 里写清楚 memory 里 `clk` 的行为。

- 删除：`docs/manual.md`、`docs/manual_zh.md`（共享镜像 `/storage/emulated/0/termux/docs/` 的两份副本同步删除）；`README.md`、`README_zh.md`、`CHANGELOG-0.5.3-alpha.md` 里的指向已移除，`UNVERIFIED-0.5.3-alpha.md` 那一行改成"已按要求删除"。
- 新增：`docs/file-formats_zh.md` / `file-formats.md` 的 **3.3 时钟 `clk`** 专节（原 3.3–3.7 顺延为 3.4–3.8），内容：
  - 三种写法：`clk = x, y, z` / `clk = 0`（含 `0, 0, 0`）/ 省略，都归到"有/无时钟"两态。
  - ROM：有时钟只在**上升沿**写 `data`；无时钟**跟随地址变化**；两种情况都只改电平真正变化的位。
  - RAM：时钟**必填**，`clk = 0` 直接拒绝整文件；上升沿顺序固定为"汇编地址 → `read enable` 高则送**旧字**到 `read data` → `write enable` 高则按 `byte write enable` 合并写回"，读优先于写。
  - 元件类型：必须是**输出类**（红石灯 / 铁活板门 / 音符盒），与 `io.txt` 的 `clk`（必须是**按钮**）正好相反。
  - 其它约束：不得与数据点重合或正交相邻；两个存储器不得共用时钟（共用者全禁用）；与 io 组共用时钟时**只禁用 io 组**；边沿按 tick 采样，`performance` 模式不会漏边沿。
- 依据（只读代码确认）：`memory.rs:360-395`（ROM 上升沿 / 无时钟跟随地址）、`memory.rs:397-470`（RAM 上升沿、读优先、字节合并）、`memory.rs:629-631`（`expect_output` 校验时钟）、`memory.rs:768-787`（输出类定义）、`memory.rs:543-560`（同一时钟多存储器拒绝）、`plot/mod.rs:412-421`（`run_memories` 每 tick 都跑）。

## 3. 规格：脚本协议（0.5.2-alpha）

> 本节是协议的唯一文字依据；面向用户的文档是 `docs/protocol.md`（英）与 `docs/protocol_zh.md`（中），
> 写码时按本节同步。BUG-005/006 的改动本节已并入。

### 3.1 报文总表

```text
HELLO MCHPRS_IO 0.5.0-alpha <tick>        （已编译）
HELLO MCHPRS_IO 0.5.0-alpha               （未编译：无 tick 字段）

INPUT <name> <hex>            → OK INPUT <name> <绝对tick>
INPUT <name> <offset> <hex>   → OK INPUT <name> <绝对tick>   仅预约模式

POUT <name>                   → OUTPUT <name> <tick> <hex>   （绑定确认 + 当前值）
OUTPUT <name> <tick> <hex>                                   （推送）

READ <name>:<bit>             → OK READ <name>:<bit> <tick> <power>
READ <x,y,z>                  → OK READ <x,y,z> <tick> <power>

TICK NOW                      → NOW <tick>

RADV <x>                      → OK RADV <x> <tick>           仅 rtps = 0
```

- `<power>`：非红石粉回 `0/1`，红石粉回 `0-15`。
- **任何以 `/` 开头的行 → `ERR unknown`**（不分动词、不做任何转发）。
- **所有动词只接受全大写**（2026-10-28 用户定）；小写一律 `ERR unknown`。
- `HELLO` 的 `<tick>` 只在已编译时出现；客户端**不发** `HELLO`，若发按未知动词 `ERR unknown` 处理。

### 3.2 模式与 `rtps`

| 配置 / 状态 | 推进方式 | `RADV` | 预约 |
| --- | --- | --- | --- |
| `performance = false` + `rtps > 0` | 自动 | `ERR mode` | 不允许 |
| `performance = false` + `rtps = 0`（普通） | 手动 | 允许 | 不允许（带 offset → `ERR mode`） |
| `performance = false` + `/tick seq`（需已编译） | 手动 | 允许 | **允许**（独占脚本） |
| `performance = true` | 批末采样 | `rtps = 0` 时允许 | 不允许；`/tick seq` **直接拒绝**（chat 红字，不踢人、不改 rtps） |

- 脚本不能修改 rtps；`/rtps` 只能玩家或终端执行。
- `/rtps 0` 不踢人、不锁定、不授予预约；`/rtps >0` 退出预约模式并解锁，不踢人。

### 3.3 预约（仅 `/tick seq` 模式）

- `<offset>` 是相对偏移（`1` = 马上到来的 tick），范围 **1…65535**，`0` 或超限 → `ERR range`。
- 不带 offset 的 `INPUT` 在**下一个 tick** 生效（回执绝对 tick = 当前 tick + 1）。
- 服务器收到立即换算成绝对 tick 入队；队列**全局最多 255 条**，满了 → `ERR full`。
- `RADV` 未走到预约 tick 时预约保持有效，到点才执行。
- 同一 tick 同一组**后发覆盖先发**；不同组互不影响。
- 独占：`/tick seq` 后第一个重连的脚本锁定，其余连接直接关闭；脚本断开后解锁但预约模式仍保留。
- 重新编译 / `/redpiler reset` → 退出预约模式、清空预约与 `POUT` 绑定、tick 归 0。
- 重复 `/tick seq` → 重新踢人 + 清空预约。

### 3.4 `POUT` / `OUTPUT`

- `POUT <name>` 绑定当前连接到该组，**不带参数**；成功直接推一条 `OUTPUT`（同时代表绑定成功），**没有 `OK POUT`**。
- 同一组可被多连接绑定；一条连接可绑多组。
- 无 `UNPOUT`，断开即解绑；重新编译清空全部绑定。
- 推送规则：无 `clk` 的组**任一位变化**才推；带 `clk` 的组只在按钮**上升沿**推。
- `INPUT` 推入后立即推送受影响组的当前值（无 clk 组只看变化）。

### 3.5 `READ`

- `READ <name>:<bit>`：`bit 0` 是最低位，对应组的**最后一个坐标**；数电组 `points[width-1-bit]`。
- 模电组按点计数，`<i>` 同样对应最后一个坐标。
- `READ <x,y,z>`：按绝对坐标读 IO 元件，红石粉回 0–15。
- 回执 `<tick>` = 最新已完成 tick，值取该 tick 运行完（下降沿）时的值。
- 读不要求入边为 0。

### 3.6 位序与 hex

- 整体位向量当一个无符号整数、前端补零、**列表第一个坐标是最显著位**；`matrix` 按逐行补零后的位数。
- 模电：每点一个十六进制位（0–f），长度 = 点数。
- hex 长度：脚本侧**严格**（必须等于 `ceil(位宽/4)`，否则 `ERR hex`）；memory 文件侧**宽松**（右对齐）。
- 补零位不占位号、无法寻址。

### 3.7 错误三分类

| 类 | 内容 | 去向 |
| --- | --- | --- |
| 日志类 | ROM / RAM 文件问题、`io.txt` 文件级问题 | 只写 `logs/` |
| 玩家类 | 玩家或终端发起的编译校验问题 | chat 红字给发起者 |
| 脚本类 | 协议报文本身错误 | 回一行 `ERR <小写码>`，不落日志 |

- 一条报文多个错误**只回第一个**。
- 禁用粒度：ROM/RAM 整文件；`io.txt` 文件级错 → 整个文件禁用；单块错 → 只禁该块。
- 脚本不能访问 ROM / RAM，也没有编译入口。

### 3.8 已删除

`PIN`、`SUB`、`UNSUB`、`EVENT`、`TICK STEP`、`pipeline` 配置项、`automation_per_tick_push`、`automation_data_port_events`、`/` 命令通道（脚本侧）。

### 3.9 其它约定

- 单行上限 **65536 字节**，超了回 `ERR line` 后断开。发送积压上限 **1 MiB**，超过断开并记日志。
- `/io list` 文案英文；行号按整个 `io.txt` 计，**不截断**。
- `performance` 的 `OUTPUT <tick>` 取批末采样所属的那个 tick。
- 独占锁定期间新连接**直接关闭**，不发任何报文。
- `/io list`：`io.txt` 中存在模电组时，已编译状态下追加一句提示——模电点需要 `-d` / `-c` 参数编译（2026-10-28 用户定）。

---

## 4. 规格：`io.txt`（数电）

### 4.1 文件与块

- 单文件 `io.txt`，位于服务端工作目录根；**文件不存在不报错**（只打一行提示）。
- 块头：`input <组名> = [` 与 `output <组名> = [`。
- `input` = 输入数据端口（拉杆 / 压力板，脚本写入）；`output` = 输出端口（灯 / 活板门 / 音符盒，脚本读出）。
- 块内**必填** `origin`、`clk`、`width`，键顺序任意；`clk = 0` 表示无时钟。
- `origin` 加到所有坐标，**含 `clk`**（与 memory 一致）。
- **空组不报错**：`width = 0` 且不写坐标在加载期是合法的（启动模板就是这种骨架，供 `/coord` 之后填充）；但从 **0.5.4-alpha** 起，编译后校验要求这种组必须有真实 `clk`，否则整组禁用（见 §2.5 与 BUG-008）。

### 4.2 坐标写法

- 单点：`x, y, z`
- 步进：`x, y, z, ±step, count`（符号必填，步长 1–255，数量 1–1024）
- 矩阵：`matrix: x, y, z, +1x, 3, +1y, 5`，1–3 段、只能单行、行优先（第一段最快）、逐行按 4 bit 补零

### 4.3 校验

- `width` = 该列表展开后的**坐标个数**（数电）。
- 加载期：语法、`width` 一致性、`matrix` 格式、组名重复；行号按整个文件计。
- 编译后：元件类型、坐标是否存在；**失败整组禁用**，其余照常。
- 组名 `[A-Za-z0-9_]+`，大小写敏感，**全局唯一**（input 与 output 之间也查重）。
- 注释支持 `//`、`#`、`/* */`。
- 点数只受"每段 ≤1024 + 坐标范围"限制，不设旧协议的 16000 上限。

### 4.4 时钟约束

- 时钟元件不得与任何端口坐标重合（撞了报错）。
- **io 组之间**的 `clk` 必须唯一，撞了后出现的组拒绝。
- io 组与 memory 的 `clk` 撞车：**只禁用 io 组**，memory 不受影响。
- 输入组写入后**脉冲一次** `clk`；数据与脉冲同在 `INPUT` 生效的那个 tick。
- `width = 0` 的组（0.5.4-alpha）：没有数据位，`INPUT <组名> 0` 只脉冲一次 `clk`（按一次按钮）；`OUTPUT`/`POUT` 的 `<hex>` 固定为 `0`，只在上升沿推送（绑定确认帧也是 `0`）；没有 `clk` 的组编译后禁用。

### 4.5 数模混写

只限"一个列表 / 一个组内"不能混；同一个 `io.txt` 里可以既有数电组又有模电组。

---

## 5. 规格：模电与存储器

### 5.1 `io.txt` 模电块

```text
analog_input  <组名> = [ origin / clk / rs_width / width / 坐标 ]
analog_output <组名> = [ origin / clk / rs_width / width / 坐标 ]
```

| 项 | 结论 |
| --- | --- |
| `rs_width` | 模电点的**个数** |
| `width` | **总位数 = `rs_width × 4`** |
| 每个点 | 固定 4 bit（红石强度 0–15） |
| hex | 每点一个十六进制位，长度 = 点数 |
| `matrix` | **不允许**（只能是单点 / 步进） |
| 校验 | 两个键**都必须写**：`rs_width` == 展开后的坐标数，且 `width` == `rs_width × 4`；关系不符 → 该组禁用 |
| 位序 | 第一个坐标 = 最显著位；一个点内 4 bit 高位在前 |

### 5.2 memory 的模电

| 角色 | 数电键 | 模电键 | 方向 | 模电要求 |
| --- | --- | --- | --- | --- |
| 地址 | `address coordinate` | `analog address coordinate` | 输入 | 点状粉 |
| 写数据 | `write data coordinate` | `analog write data coordinate` | 输入 | 点状粉 |
| 读数据 | `read data coordinate` | `analog read data coordinate` | 输出 | 点状粉，其它信号不得驱动，允许比较器读 |
| ROM 数据 | `data coordinate` | `analog data coordinate` | 输出 | 同上 |

- 数电 / 模电**按每条线选**，可混搭；键名用空格分隔，不带下划线。
- 每个角色只能出现一个列表（数电键与模电键二选一）。
- 输入点只要是点状粉即可，不要求入边为 0；输出点由 memory 驱动，要求无其它输入。
- 模电点只认红石粉。
- 编译参数：点需要 `-d`，十字需要 `-c`，都要加。

### 5.3 存储器文件（`memory/*.rom.txt` / `*.ram.txt`）

- 首行 `v<内核版本> hex words addressed`，**严格匹配**；当前应为 `v0.5.2-alpha`（见 BUG-018）。
- 每个端口列表**第一项必须是 `width = N`**；`width` = 展开后的坐标个数。
- 坐标行与 `io.txt` 同规则；`clk` 与 `origin` 必须写全；`origin` 加到所有坐标。
- ROM 键：`address coordinate`、`data coordinate`；RAM 另有 `write enable`、`read enable`、`byte write enable`、`write data coordinate`、`read data coordinate`，且 RAM 的 `clk` 必须指向真实方块。
- 同一存储器任意两位落在同一坐标 → 报错（跨键也查）。
- 加载期查文件，编译后查元件；任一失败**整文件禁用**，内存中未落盘数据丢弃。

**落盘时机**：玩家退出、服务器退出（SIGINT/TERM/HUP）、`/redpiler reset`、编译崩溃、`/memory reload` 之前。

**回写格式**：Logisim 兼容——每行固定 8 word、行首地址按 8 对齐并按地址总线宽度补足位数、全空的块整块省略、空格子按数据位宽补零。

### 5.4 点状粉判定

- `is_dot`（四面不连）或 `is_cross`（四面相连的十字）；真正被电路驱动的十字会被判失败。
- 模电**输出**点额外要求 `has_inputs == false`。
- 红石粉读写：只识别孤点；读不限，写只允许入边为 0 的孤点；写入只能整组 `INPUT`。

---

## 6. 待确认 / 待做

| # | 项 | 状态 |
| --- | --- | --- |
| 1 | memory 首行改 `v0.5.2-alpha`（BUG-018） | 📋 用户暂缓 |
| 2 | 0.5.2-alpha 实机测试（BUG-018，T-01…T-83） | 📋 待跑 |
| 3 | 工作区改动提交（BUG-019） | ✅ 已推送 `main`（`328e2ca`），工作区干净 |
| 4 | 演示程序（`test.py` 与加法器世界） | 📋 用户后续补 |
| 5 | 文档同步：两份 README、两份 `protocol*.txt` | 📝 随 BUG-005/006 一起改 |
| 6 | `betterio` 与 `performance` 联动（BUG-021） | ✅ 已确认：完全独立，任意组合 |
| 7 | 木斧取点写入坐标（BUG-022） | ✅ 已定：`/coord in/add/new`，规格见 §2.7 |
| 8 | 指令“写法升级”（BUG-023） | ✅ 已定：规格见 §2.7 |
| 9 | 输入组用按钮（BUG-024） | ✅ 已定：B 脉冲语义、输出组不加按钮 |
| 10 | 0.5.3-alpha 未验收清单（含已改未编译的 3 项） | 📋 `UNVERIFIED-0.5.3-alpha.md`（仓库根目录，已复制到共享存储 `docs/`），随验收逐项打勾 |
| 11 | 基岩版客户端接入（BUG-032） | 📋 仅调研：Geyser + ViaProxy 链路可行，P0 风险未验证；分析见 `~/dsh/BEDROCK-CLIENT-FEASIBILITY-0.5.3.md` |
| 12 | `width = 0` 时钟端口规格（并入 BUG-008） | ✅ 已完成（0.5.4-alpha）：hex 固定 `0`，上升沿推送；无时钟的组编译后禁用 |

---

## 7. 版本记录

| 版本 | 内核 / 协议 | 状态 |
| --- | --- | --- |
| `0.5.4-alpha` | 0.5.4-alpha / 0.5.4-alpha | ✅ `width = 0` 端口（hex 固定 `0`、无时钟禁用）+ 文档整理（删 manual、`file-formats` 增 3.3 时钟节）；编译通过（7m42s，仅既有的 3 条死代码/弃用警告）+ 已替换二进制（sha256 `ca895885…`，8,572,208 字节，备份 `old/mchprs-0.5.3-alpha-e`）；**未打 tag、未发 Release** |
| `0.5.2-alpha` | 0.5.2-alpha / 0.5.2-alpha | ✅ 模电 `matrix:`、日志去重、文件版本兼容（0.5.0+）均已完成；编译通过（1m42s）+ 三组冒烟测试通过 + 已替换线上二进制（sha256 `19320ca2…`）；2026-10-28 提交 `main`（`328e2ca`）+ 打标签 `v0.5.2-alpha` + 发布 Release（CI 编译 7 个平台二进制） |
| `0.5.1-alpha` | 0.5.1-alpha / 0.5.1-alpha | ✅ 二次构建：模电双键（`rs_width` + `width`）与动词全大写，编译通过（3m40s）+ 冒烟测试通过 + 已替换线上二进制（sha256 `8c98c83b…`） |
| `0.5.0-alpha` | 0.5.0-alpha / 0.5.0-alpha | ✅ 编译通过、冒烟测试通过、已替换线上二进制（sha256 `5a07ee1c…`） |
| `0.4.3-beta` | 0.4.3-beta / 0.4.3-beta | ✅ 已发布、线上部署、运行验证通过；2026-10-28 补打远程附注标签 `v0.4.3-beta`（→ `7a36df6`）并发布 GitHub Release（Release 页面已于 2026-10-28 删除，tag 保留） |
| `0.4.2-beta` | — | ⚠️ 未单独发布，功能并入 0.4.3-beta |
| `0.4.1-beta` | 0.4.1-beta / 0.4.0-beta | ✅ 已投入使用（行为已被 0.4.3 覆盖） |
| `0.4.0-beta` | 0.4.0-beta / 0.4.0-beta | ✅ 已投入使用 |
| `0.3.0-beta` | 0.3.0-beta / 0.3.0-beta | ✅ 已投入使用 |
| `0.2.x` | 0.2.0–0.2.1 | ✅ 已发布 |

- **远程发布对应关系**：2026-10-28 清空旧 Release——`v0.2.1-beta` / `v0.3.0-beta` / `v0.4.3-beta` 三个 Release 页面已删除（git 标签全部保留，说明文字备份在 `~/dsh/releases-archive-2026-10-28/`）；现存 Release 只有 `v0.5.2-alpha`（<https://github.com/xanax2112/MCHPRS_IO/releases/tag/v0.5.2-alpha>，预发布，CI 编译 7 个平台二进制）与 CI 自动生成的 `preview`。`0.4.2-beta` 从未单独发布。
- **CI 行为**：推 `v*` 标签会触发 `build.yml`（6 个目标 + 通用 macOS 包），发布环节先删后重建 `preview`，再建本版本 Release；推 `main` 不触发任何工作流（只监听 `master` 与 `v*`）。

---

## 8. 上游溯源与归档

- 对比基线：上游 `master` @ `d492e43`，同为 Minecraft 1.20.4 / 协议 765。
- **上游固有缺陷**（本 fork 未引入）：比较器对空容器越界、编译崩溃无优雅中止、命令补全缺失、原 whitelist 静默失败、告示牌朝向 `as u8` 饱和。
- **本 fork 独有设计**：整套自动化协议、存储器文件、`io.txt`、`analog`、单地皮空岛模型、`/tick seq` 预约模式。
- **有意偏离上游**：空容器判为警告并按 0 参与编译；`Compiler::compile` 返回 `Result`；编译 panic 保存并退出。
- **归档**（均在 `/storage/emulated/0/termux/old/backup-2026-10-28/`）：
  - `BACKLOG.pre-rewrite-2026-10-28.md`（本次重写前的完整台账，含 0.5.0-alpha 讨论全过程与旧编号 BUG-001…038）
  - `BACKLOG.archive-2026-10-27.md`（0.1–0.4.3 历史）

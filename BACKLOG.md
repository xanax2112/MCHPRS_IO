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
| 线上二进制 | `~/mchprs/mchprs`，**0.5.2-alpha**（8,478,984 字节，sha256 `19320ca2…`）；0.5.1-alpha（`8c98c83b…`）备份在 `/storage/emulated/0/termux/old/mchprs-0.5.1-alpha-b` |
| 服务器运行目录 | `/storage/emulated/0/termux/MCHPRS_IO/`（`Config.toml`、`io.txt`、`memory/`、`logs/`、`world/`） |
| 归档目录 | `/storage/emulated/0/termux/old/`（2026-10-28 起）：`backup-2026-10-28/`（旧版本二进制、改动前源码、台账快照）、`scratch-2026-10-28/`（构建日志与临时脚本）、`drafts-2026-10-28/`（旧 rom 草稿） |
| 最近提交 | `7a36df6`（0.4.2+0.4.3 合并提交，已推送）；0.5.0-alpha 与 0.5.2-alpha 的改动**全部未提交** |
| 构建 | `cargo build --release`，aarch64 Termux，约 1m45s；`/tmp` 不可写，日志写 `~/dsh/scratch/` |

---

## 2. 条目清单

> 2026-10-28 四次整理：恢复 0.1–0.4.3 的历史条目并按主题压缩，编号压到 **BUG-001…020**。
> 旧编号不再使用，也不提供对照表；完整旧台账在 `old/backup-2026-10-28/BACKLOG.pre-rewrite-2026-10-28.md`。
> `✅` = 已完成；`🚧` = 部分待做；`📋` = 待做；`⏳` = 等用户。

### 2.1 配置（A）

| 编号 | 条目 | 状态 |
| --- | --- | --- |
| BUG-001 | 配置项整理：`performance` 布尔化 + 旧键迁移 | ✅ 已完成 |

**BUG-001**

- `mode = "exact"/"performance"` → 布尔 `performance`（默认 `false` = exact，可整行省略）。
- 旧键 `mode` 读到即忽略，启动时**删除该行**并补写 `performance = false`，旧值不参与换算；非法值（如 `"yes"`）**启动直接报错**，不回退。
- `automation_port` → `io_port`（默认 25585，绑 `127.0.0.1`）；`automation_data_port_events`、`automation_per_tick_push`、`auto_redpiler` 启动时移除；其余键（`schemati`、`block_in_hitbox`、`velocity` 等）保留。
- 连带作废：旧「配置写错导致 plot panic」——布尔值不需要引号，该错误类型从根上消失。

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

### 2.5 io.txt 与信号（D）

| 编号 | 条目 | 状态 |
| --- | --- | --- |
| BUG-008 | 时钟与推送规则 | ✅ 已完成 |
| BUG-009 | `io.txt` 解析、校验与坐标复用 | ✅ 已完成 |
| BUG-010 | 模电组、红石粉与点状粉判定 | ✅ 已完成（含模电 `matrix:`，0.5.2-alpha） |

**BUG-008 时钟与推送**

- 输入组 `INPUT` 后脉冲一次 `clk`，数据与脉冲同在生效 tick。
- 输出组：无 `clk` 时任一位变化就推 `OUTPUT`；有 `clk` 时只在按钮上升沿推。
- 时钟不得与端口坐标重合；io 组之间 `clk` 唯一，重复者拒绝；io 组与 memory 的 `clk` 撞车**只禁用 io 组**。

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

### 2.7 终端、命令与其它修复（F）

| 编号 | 条目 | 状态 |
| --- | --- | --- |
| BUG-012 | 终端支持 `/` 指令 | ✅ 已完成 |
| BUG-013 | 命令表与其它历史修复 | ✅ 已完成 |

**BUG-012**：stdin 读取，带 `/` 前缀才执行，权限与游戏内一致；需要玩家的命令（`/tp`、`/gamemode`、`/container`、`/speed`）提示 `This command needs a player and cannot be used from the console`；新增 `/stop` 优雅关服；`/tick`、`/tick seq`、`/rtps`、`/radv`、`/io list`、`/memory reload`、`/redpiler` 均可用。

**BUG-013**：`/gamemode` 等命令的 **Tab 补全**补齐（0.2.0）；`/whitelist` 因静默失败**整个功能删除**（0.2.1）；告示牌朝向修正（yaw 未归一化，0.4.3）。

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
| BUG-019 | 工作区改动未提交 | ⏳ 等用户指令 |

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
- 空组报错；`width = 0` 的数电组也报错。

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
| 3 | 工作区改动提交（BUG-019） | ⏳ 用户明确不提交 |
| 4 | 演示程序（`test.py` 与加法器世界） | 📋 用户后续补 |
| 5 | 文档同步：两份 README、两份 `protocol*.txt` | 📝 随 BUG-005/006 一起改 |

---

## 7. 版本记录

| 版本 | 内核 / 协议 | 状态 |
| --- | --- | --- |
| `0.5.2-alpha` | 0.5.2-alpha / 0.5.2-alpha | ✅ 模电 `matrix:`、日志去重、文件版本兼容（0.5.0+）均已完成；编译通过（1m42s）+ 三组冒烟测试通过 + 已替换线上二进制（sha256 `19320ca2…`） |
| `0.5.1-alpha` | 0.5.1-alpha / 0.5.1-alpha | ✅ 二次构建：模电双键（`rs_width` + `width`）与动词全大写，编译通过（3m40s）+ 冒烟测试通过 + 已替换线上二进制（sha256 `8c98c83b…`） |
| `0.5.0-alpha` | 0.5.0-alpha / 0.5.0-alpha | ✅ 编译通过、冒烟测试通过、已替换线上二进制（sha256 `5a07ee1c…`） |
| `0.4.3-beta` | 0.4.3-beta / 0.4.3-beta | ✅ 已发布、线上部署、运行验证通过；2026-10-28 补打远程附注标签 `v0.4.3-beta`（→ `7a36df6`）并发布 GitHub Release |
| `0.4.2-beta` | — | ⚠️ 未单独发布，功能并入 0.4.3-beta |
| `0.4.1-beta` | 0.4.1-beta / 0.4.0-beta | ✅ 已投入使用（行为已被 0.4.3 覆盖） |
| `0.4.0-beta` | 0.4.0-beta / 0.4.0-beta | ✅ 已投入使用 |
| `0.3.0-beta` | 0.3.0-beta / 0.3.0-beta | ✅ 已投入使用 |
| `0.2.x` | 0.2.0–0.2.1 | ✅ 已发布 |

- **远程发布对应关系**：`v0.4.3-beta` → Release `0.4.3-beta`（<https://github.com/xanax2112/MCHPRS_IO/releases/tag/v0.4.3-beta>）；仅源码包（tag 自动生成），无二进制附件；`0.4.2-beta` 不单独发布。更晚的 `0.5.x-alpha` 只存在于 Android 本地工作区，未提交、未打标。

---

## 8. 上游溯源与归档

- 对比基线：上游 `master` @ `d492e43`，同为 Minecraft 1.20.4 / 协议 765。
- **上游固有缺陷**（本 fork 未引入）：比较器对空容器越界、编译崩溃无优雅中止、命令补全缺失、原 whitelist 静默失败、告示牌朝向 `as u8` 饱和。
- **本 fork 独有设计**：整套自动化协议、存储器文件、`io.txt`、`analog`、单地皮空岛模型、`/tick seq` 预约模式。
- **有意偏离上游**：空容器判为警告并按 0 参与编译；`Compiler::compile` 返回 `Result`；编译 panic 保存并退出。
- **归档**（均在 `/storage/emulated/0/termux/old/backup-2026-10-28/`）：
  - `BACKLOG.pre-rewrite-2026-10-28.md`（本次重写前的完整台账，含 0.5.0-alpha 讨论全过程与旧编号 BUG-001…038）
  - `BACKLOG.archive-2026-10-27.md`（0.1–0.4.3 历史）

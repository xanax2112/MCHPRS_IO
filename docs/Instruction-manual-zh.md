# MCHPRS_IO 使用手册

## 目录

1. 本仓库新增了什么
2. 隐式规则
3. 编译与运行
4. 仓库结构
5. 游戏内命令
6. `io.txt`
7. 存储器文件
8. 自动化协议
9. Python 测试工具
10. Android APK
11. 常见问题
12. 限制与默认值

---

MCHPRS_IO 是 [MCHPRS](https://github.com/MCHPRS/MCHPRS) 的分支——一个把红石电路**编译成图**、
之后只模拟这张图的 Minecraft 服务器，所以几百万个红石元件的 plot 依然能跑到几万 tick/s。
本仓库在它上面加了 IO / 自动化层、plot 工作流、Python 测试工具和 Android APK。

本手册是项目的统一参考：怎么编译、怎么接线、自动化协议、存储器文件、测试工具。
它取代旧的 `protocol.md` 与 `file-formats.md`。

- 中文：本文件 · English: [Instruction-manual.md](Instruction-manual.md)
- 台账（决策与历史）：[`backlog/BACKLOG.md`](backlog/BACKLOG.md)、
  [`backlog/BEDROCK.md`](backlog/BEDROCK.md)、[`backlog/PYTHON-BACKLOG.md`](backlog/PYTHON-BACKLOG.md)
- 上游设计说明：[`upstream/Redpiler.md`](upstream/Redpiler.md)、[`upstream/RIL.md`](upstream/RIL.md)

---

## 1. 本仓库新增了什么

上游 MCHPRS 已经能把 plot 编译成图、跑到几万 tick/s，并自带 plot、WorldEdit 和投影功能。
本仓库在它之上加了 IO 与自动化层：

| 组件 | 位置 | 提供什么 |
| --- | --- | --- |
| IO 组 | `io.txt`（§6） | 脚本可读写的命名坐标集合，数字/模拟、带时钟 |
| 存储器 | `memory/`（§7） | 兼容 Logisim 的 ROM/RAM，由编译电路驱动；RAM 会回写 |
| 自动化桥 | `io_port`（§8） | 行式 TCP 协议：`HELLO`、`INPUT`、`POUT`/`OUTPUT`、`READ`、`TICK NOW`、`RADV` |
| 时序控制模式 | `/tick seq`（§8.2） | 冻结电路、只放一个脚本、把写入预约到精确的将来 tick |
| 预约批量回复 | §8.4 | 一整批预约只回一条 `OK INPUT <名字> <条数>` |
| 出入度报告 | `/redpiler stats`（§5） | 编译后把 Markdown 格式的入度/出度报告写进 `logs/` |
| 游戏内编辑 IO | `/coord`、`/io`、`/memory`（§5） | 不进外部编辑器也能改 `io.txt` / 存储器，并热重载 |
| 采样与时序开关 | `performance`、`betterio`（§3.3） | 每 tick 或每批采样；更快的按钮/灯时序 |
| 测试工具 | `python/main.py`（§9） | 正确性与流水吞吐测试，带延迟统计 |
| Android APK | `app/`（§10） | 把服务器打成一个带控制台的独立 App |
| 台账 | `docs/backlog/` | 内核、基岩、Python 三条线的决策、历史与未结项 |

上游功能全部原样保留；新增部分都是可选的（没有 `io.txt` 的 plot 与上游行为一致）。

## 2. 隐式规则

这些规则不在任何单独一节的格式说明里，但忽略了一定会踩坑。可以先整体扫一遍，
遇到具体格式再回查对应章节；每条都标了去处。

1. **服务器不接受命令行参数。** `Config.toml`、`io.txt`、`memory/`、`world/`、`logs/` 都相对
   当前工作目录；换个目录启动就会悄悄新建一个空世界。
2. **`io.txt` 在启动时读一次**，`/io reload` 再读，用 `/coord` 改过的会在下次编译前自动重读。
   用外部编辑器改文件必须 `/io reload`。
3. **校验分两段。** 加载时查语法、键、宽度、重名，以及**纯坐标层面的冲突**
   （时钟落在数据点上、两个组共用时钟）——这些不需要编译结果就能判定；
   "那个坐标上到底有没有拉杆、类型对不对"才要等编译后才知道。
   校验失败的块会被禁用，文件其余部分和服务器照常运行。
4. **`clk` 在两个文件族里含义相反。** `io.txt` 里必须是**按钮**（服务器要**按**它）；
   存储器里必须是**输出类**方块（灯 / 铁活板门 / 音符盒，服务器要**读**它的电平）。
5. **被禁用的组回 `ERR disabled`，不是 `ERR name`。** 组存在，只是校验没过——看日志里的行号。
6. **重新编译会清空所有 `POUT` 订阅和全部预约。** 脚本必须在编译后重新订阅。
7. **`/tick seq` 要求已编译且 `performance = false`。** 进入时会踢掉所有已连接脚本。
8. **独占权会自动续接。** 独占脚本断开后它的预约被丢弃，下一个连接自动接管，
   **不需要重新敲 `/tick seq`**；`/rtps <n>`、重新编译或关服才会退出该模式。
9. **一批预约在"静默 5 ms"或"来了任何非 `INPUT` 请求"时结束。** 发批前后不要插入别的动词。
10. **`OK INPUT <名字> <条数>` 只说明接受了多少行。** 必须先核对条数再发 `RADV`，
    否则之后的输出会全部错位归属。
11. **客户端要用空闲超时，不能固定总预算。** 合法的批次本来就要跑好几秒；
    每收到一行就重置 deadline（固定预算就是"大批次 RADV 失败"那个 bug 的成因）。
12. **两个 tick 字段在同一个 tick 的两端。** `INPUT` 锚在 tick 开头、`OUTPUT` 锚在末尾，
    所以游戏内数出的延迟 = 原始差 + 1。
13. **输入后的第一条 `OUTPUT` 未必是稳定值。** 组合电路可能每 tick 推一次中间值直到收敛。
14. **`performance = true` 会合并同一 tick 批次内的采样。** 脚本收到的 `OUTPUT` 更少
    （最终值相同），且 `/tick seq` 不可用。
15. **每次编译前必须执行一次 `//update -p`。** 它把整个 plot 的方块刷一遍
    （会先拆掉已编译电路，正好在编译前用）；不刷直接编译会踩上游冷启动 bug
    （BUG-038），开头几十组输入算错。`--update` 是另一回事：默认关闭，只在电路**被拆**时刷新。
16. **`width = 0` 的组没有数据位。** `INPUT <名字> 0` 只是按一次按钮，`OUTPUT` 恒为 `0`、
    每个上升沿推一次。
17. **模拟点只有在编译时保留才会存在。** 线点要 `-d`（`--wire-dot-out`），十字要 `-c`
    （`--wire-cross-out`），否则该组会被禁用。
18. **`matrix:` 组打包出的 hex 位数比 `ceil(width / 4)` 多**，因为每行单独补零；
    补出来的位不能用 `READ` 寻址。
19. **存储器时钟不能碰数据点**，两个存储器共用一个时钟会两个都被禁用；
    io 组与存储器共用时只禁用 io 组。
20. **RAM 只在特定时机回写**（玩家退出、SIGINT/SIGTERM/SIGHUP、`/redpiler reset`、
    编译崩溃、`/memory reload`）。SIGKILL 会丢未保存的字，编译后校验失败也会丢弃。
21. **脚本没有斜杠命令。** 任何以 `/` 开头的行都回 `ERR unknown`。
22. **自动化端口只绑回环。** 无论 `bind_address` 写什么，它永远只绑 `127.0.0.1`。
23. **没有内置认证。** Java 端按离线 UUID 识别玩家，默认监听 `0.0.0.0:25565`；
    对公网开放前请自行加防火墙或反向代理（与上游一致）。


---

## 3. 编译与运行

### 3.1 编译

```bash
cargo build --release                      # 只 Java 版（默认）
cargo build --release --features bedrock   # 加基岩版支持
```

`lto = "fat"`，首次 release 编译要几分钟；之后只重编改动的 crate。产物是
`target/release/mchprs`。

### 3.2 运行

服务器**不接受命令行参数**，只从当前工作目录读 `Config.toml` 和 `io.txt`。

```bash
cd /path/to/run-dir        # 里面有 Config.toml、io.txt、memory/、world/
mchprs
```

首次启动会补全缺失的 `Config.toml`、`io.txt`、`memory/` 骨架。

### 3.3 `Config.toml`

| 键 | 默认 | 含义 |
| --- | --- | --- |
| `bind_address` | `"0.0.0.0:25565"` | Java 监听地址；其中的 IP 也决定基岩版绑哪块网卡 |
| `bedrock` | `19132` | 基岩版 UDP 端口；`0` 关闭 |
| `motd` | `"Minecraft High Performance Redstone Server"` | 服务器列表文字 |
| `chat_format` | `"<{username}> {message}"` | 聊天模板 |
| `max_players` | `99999` | 人数上限 |
| `view_distance` | `8` | 视距（区块） |
| `schemati` | `false` | 每个玩家独立的 `schems/<uuid>/` 目录 |
| `block_in_hitbox` | `true` | 方块碰撞箱 |
| `io_port` | `25585` | 自动化 TCP 端口，固定只绑 `127.0.0.1` |
| `performance` | `false` | `true` 每个 tick 批次采样一次 IO，`false` 每 tick 采样 |
| `betterio` | `false` | 更快的 IO 时序：按钮 2t 释放、灯瞬间切换 |
| `luckperms` / `velocity` | 未设置 | 可选的 LuckPerms / Velocity 集成 |

旧键（`automation_port`、`mode` 等）会在启动时就地迁移或删除。

`/tick seq` 要求 `performance = false`（逐 tick 精确采样）。除非清楚代价，否则保持关闭：
`performance = true` 会合并同一个 **tick 采样批次**（服务器一次 tick 批处理）内的多次状态变化，
脚本收到的 `OUTPUT` 会更少（最终值相同）。它和 §8.4 的"预约批"不是一回事。

### 3.4 第一次上手

1. 进服，`/plot auto` 拿一个 plot
2. 搭电路
3. 写 `io.txt`（见 §6），先执行一次 `//update -p`，再执行 `/redpiler compile`
4. 用脚本（§8）或 Python 工具（§9）驱动

`io.txt` 在启动时读取；`/io reload` 重新读；用 `/coord` 改过的会在下次编译前自动重读一次。

---

## 4. 仓库结构

```
crates/
  core/      服务器、plot、IO 组、存储器、自动化桥、命令
  network/   Java 协议；基岩版插头（feature "bedrock"）
  redpiler/  编译器：前端、pass、direct 后端（真正跑图的模拟器）
  world/     区块、方块、调色板、存储
  redstone/  未编译时的原版红石模拟
  blocks/    方块类型与属性
  text/ save_data/ schematic/ utils/ rilc/ world_exporter/
src/         二进制入口
python/      自动化测试工具（main.py + core/）
app/         Android APK 打包（build.sh 出 classes.dex + 内置二进制）
docs/        本手册、台账、上游说明
```

运行时写出的东西都在工作目录里：`world/`、`logs/`、`schems/`、`memory/`，以及它读取的
`Config.toml` / `io.txt`。

---

## 5. 游戏内命令

| 命令 | 作用 |
| --- | --- |
| `/redpiler compile [flags]`（`/rp c`） | 编译 plot |
| `/redpiler reset`（`/rp r`） | 丢弃已编译电路 |
| `/redpiler inspect`（`/rp i`） | 打印准星所指方块对应的图节点 |
| `/redpiler stats` | 用默认选项编译，并把出入度报告写进 `logs/`（见下） |
| `/rtps <n \| unlimited>` | 每秒 tick 数；`0` 冻结 |
| `/tick seq` | 进入时序控制模式（§8.2）：踢掉所有已连接脚本，独占锁在脚本断开后自动续接 |
| `/radv <n>` | 冻结状态下手动推进 n tick |
| `/coord in\|new\|add\|undo\|redo` | 游戏内编辑 `io.txt` / 存储器文件（用法见本节表格后） |
| `/io list`、`/io reload` | 查看 / 重读 `io.txt` |
| `/memory reload` | 重读 `memory/` |
| `/plot …` | plot 管理（`auto`、`info`、`claim`、`visit`、`middle` 等） |
| `/version` | 打印版本 |
| `//load`、`//save`、`//schem load\|save\|list`、`//update [-p]` | WorldEdit 投影 |

`/coord` 的用法：

- `/coord in io.txt <组名>` 或 `/coord in <名字>.rom|.ram` —— **选中目标**（进入该文件的编辑模式），
  之后的 `new` / `add` 都写到这里；目标不存在直接报错。
- `/coord new <类型> [步长] [个数] [方向]` —— **替换**该类型的坐标列表（`new` 不是"新建组"）。
- `/coord add <类型> [步长] [个数] [方向]` —— **追加**坐标。
- 锚点是 WorldEdit 魔杖的**第二个选区点**；`方向` 缺省用玩家朝向。
- `/coord undo` / `/coord redo` —— 撤销 / 重做最近一次写入（按文件存历史）。
- 类型：`io.txt` 组用 `clk` / `data`；存储器用 `clk` / `addr` / `rd` / `wd` / `we` / `re` / `bwe`（模拟端口带 `a_` 前缀）。

`/redpiler compile` 的选项（也支持短选项 `-o -e -i -u -d -l -c`）：

| 选项 | 效果 |
| --- | --- |
| `--optimize` | 跑优化 pass（编译明显变慢，可能破坏刁钻电路） |
| `--io-only` | 拆除时只回写 IO 方块 |
| `--update` | 这条电路被拆时刷新全图方块（见下） |
| `--export` | 导出图给 `redpiler_graph` 的消费者 |
| `--export-dot` | 导出 dot 文件 |
| `--wire-dot-out` / `--wire-cross-out` | 把线点 / 十字当输出点（模拟组需要） |
| `--illegal-states-out` | 把非法线状态当输出 |
| `--print-after-all` / `--print-before-backend` | 打印 RIL 电路 |

> **编译前必须先执行一次 `//update -p`。**
> 上游有个冷启动问题（台账 BUG-038）：世界刚加载时方块的编译期状态没同步，
> 直接编译的话开头几十组输入会算错（实测前 65 组全错，之后才全部正确），
> 既不报错也不打日志。`//update -p` 会把整个 plot 的方块刷一遍，
> 刷完再编译就不会踩到。游戏里和服务器终端都能敲。

`--update` **默认不开**。它是另一回事——只在**上一次**电路被拆时刷方块，
因为 reset 消费的是上一次编译存下的选项。

`/redpiler stats` 用默认选项编译一次，然后写
`logs/redpiler_stats_<时间戳>.md`：按类型的节点数、完整的入度/出度分布与占比、
出度/入度最高的节点（带坐标）。控制台和聊天栏只回 `Done.` 或 `ERR <code>`。
冻结状态下也能用。

---

## 6. `io.txt`

`io.txt` 声明 **IO 组**：一组有名字的坐标，脚本可以读写。它放在 `Config.toml` 旁边，
会被校验两次——加载时一次，编译成功后一次（要有编译结果才知道那个坐标上到底是什么方块）。

### 6.1 语法

- UTF-8 文本；注释 `// …`、`# …`、`/* … */`；空行忽略。
- 键是 `key = value`，`=` 两边空格随意。
- 坐标列表用 `[` 开始、`]` 结束；每个条目展开成一个或多个坐标。

| 形式 | 例子 | 含义 |
| --- | --- | --- |
| 单点 | `10, 64, 10` | 一个坐标 |
| 步进 | `10, 64, 10, +2x, 4` | 起点、轴向与方向、个数 |
| 矩阵 | `matrix: 10, 64, 10, +2x, 3, +4y, 5` | 3×5 块，按行展开（§6.7） |

步进：步长 1–255、个数 1–1024、轴 `x`/`y`/`z`、符号必写。红石元件通常隔一格，所以
`+2x` 最常见。

`origin = 0` 表示不偏移；`origin = x, y, z` 会加到该块的**每一个**坐标上，包括 `clk`。
写成单个非零数字（`origin = 5`）是错误。

```toml
input switches = [
origin = 100, 64, -8
clk = 40, 64, 10        # 真实时钟位置：(140, 128, 2)
width = 2
0, 0, 0                 # 真实坐标：(100, 64, -8)
2, 0, 0                 # 真实坐标：(102, 64, -8)
]
```

### 6.2 块

块头是 `<角色> <名字> = [`，以 `]` 结束。

| 角色 | 方向 | 允许的方块 |
| --- | --- | --- |
| `input` | 脚本 → 电路 | 拉杆、压力板 |
| `output` | 电路 → 脚本 | 红石灯、铁活板门、音符盒 |
| `analog_input` | 脚本 → 电路 | 红石线点 |
| `analog_output` | 电路 → 脚本 | 没有其他东西驱动的红石线点 |

`io.txt` 的**文件首行**可以是可选的版本戳，两种写法等价：完整形式
`v0.6.2-beta hex words addressed`，或裸形式 `v0.6.2-beta`。低于 0.5.0 的版本戳会让整个文件失效；
没有版本戳时按当前格式解析。

### 6.3 必填键

| 键 | 位置 | 含义 |
| --- | --- | --- |
| `origin` | 每个块 | 加到该块所有坐标上的偏移 |
| `clk` | 每个块 | 时钟元件，**必须是按钮**；`0` 表示无时钟 |
| `width` | 每个块 | 数字组：点数；模拟组：总位数 |
| `rs_width` | 仅模拟组 | 模拟点数 |

数字组：`width` 等于展开后的点数。模拟组：`rs_width` 等于点数，且 `width = rs_width * 4`；
不一致则整组禁用。

名字是 `[A-Za-z0-9_]+`，区分大小写，整个文件内唯一。

### 6.4 校验

| 检查 | 时机 | 结果 |
| --- | --- | --- |
| 语法 / 块未闭合 / 块头错误 | 加载 | 整个 `io.txt` 禁用 |
| 缺键、宽度错、重名、时钟落在数据点上 | 加载 | 该块禁用 |
| 坐标上没有元件 / 类型不对 | 编译后 | 该块禁用 |
| `clk` 与另一个 io 组重复 | 加载 | 后出现的块禁用 |
| `clk` 与存储器共用 | 编译后 | io 块禁用，存储器不受影响 |
| `width = 0` 却没有真实 `clk` | 编译后 | 该组禁用 |

### 6.5 时钟与运行行为

| 情况 | 行为 |
| --- | --- |
| 有时钟的组收到 `INPUT` | 同一 tick 内写入数据并按一次按钮 |
| 无时钟的组收到 `INPUT` | 只写数据，不脉冲 |
| 有时钟的输出组 | 只在上升沿推一次 `OUTPUT` |
| 无时钟的输出组 | 任意一位变化就推 |
| 重新编译 | 所有 `POUT` 绑定与预约全部清空 |

`width = 0` 的组没有数据位：`INPUT <名字> 0` 表示按一次按钮，`OUTPUT` 恒为 `0`，按上升沿推送。

模拟点只有在编译时显式保留才会存在：线点要 `-d`（`--wire-dot-out`），十字要 `-c`
（`--wire-cross-out`）。

### 6.6 例子

```toml
input counter = [
origin = 0
clk = 30, 64, 10
width = 4
0, 64, 10
2, 64, 10
4, 64, 10
6, 64, 10
]

output display = [
origin = 0
clk = 0
width = 8
10, 64, 20, +2x, 8
]

analog_input bus = [
origin = 0
clk = 0
rs_width = 2
width = 8
40, 64, 0
42, 64, 0
]

output grid = [
origin = 0
clk = 0
width = 6
matrix: 0, 64, 30, +2x, 3, +3z, 2
]
```

### 6.7 `matrix:` 展开

`matrix: <原点>, <±步长轴>, <个数>`，**1–3** 段，必须写在一行（逗号分隔后共 5、7 或 9 段）。
步长 1–255 且带符号，个数 1–1024，轴 `x`/`y`/`z`。

展开是混合进制、**第一段变化最快**，等价于嵌套循环；同一个轴可以出现多次（偏移相加）。
总点数 = 各段个数之积，必须等于 `width`（按点数算）。

数字组的填充：`count1` 个点组成一行，每行**在前面补零到 4 位的整数倍**，
`rows = count2 * count3`。所以打包后的 hex 长度是 `rows * ceil(count1 / 4)`，
可能和 `ceil(width / 4)` 不同。

6 个灯摆成 3 列 × 2 行，矩阵写法与普通写法对比：

| | 矩阵写法 | 普通写法 |
| --- | --- | --- |
| 打包 | `0ABC` `0DEF` | `00ABCDEF` |
| A=1 B=0 C=1 D=1 E=0 F=1 时的位 | `0101` `0101` | `00101101` |
| hex | `55` | `2d` |

两者都声明 `width = 6`，`READ <名字>:<bit>` 指向的也是同一批灯，但 hex 文本不同。
换成 5 列 × 2 行，矩阵写法打包 4 位 hex，普通写法只有 3 位。

模拟组的 `matrix:` 尚未实现（会报 `a matrix: list cannot be analog`，BUG-010）。

---

## 7. 存储器文件

`memory/<名字>.rom.txt`（只读）与 `memory/<名字>.ram.txt`（可写），镜像兼容 Logisim。

### 7.1 文件头与结构

```text
v0.6.2-beta hex words addressed
origin = 0
clk = 0

address coordinate = [
width = 4
-8, 64, 0, +2x, 4
]

data coordinate = [
width = 8
2, 64, 0, +2x, 8
]

00: 0f 3c 69 00 00 00 00 00
```

- 版本戳接受 ≥ 0.5.0 的任意版本；`hex words addressed` 后缀可省略，裸 `v0.6.2-beta` 等价。
- `origin` 与 `clk` 是文件级键；`origin` 会加到每个端口坐标上（含 `clk`）。
- 每个端口列表以 `width = N` 开头，展开后必须正好 N 个点。
- 键行到数据段开始为止（第一行 `地址:` 或 hex 字）。
- 文件名以点开头的（`memory/.rom.txt`）是首启动写下的骨架模板，加载时跳过。

### 7.2 `clk`

| 写法 | 含义 |
| --- | --- |
| `clk = x, y, z` | 真实时钟：读该方块的**电平**，只在**上升沿**动作 |
| `clk = 0` / `clk = 0, 0, 0` / 不写 | 无时钟 |

- **ROM**：时钟可选。有时钟时上升沿把当前地址的字驱动到数据口；无时钟时跟随地址，
  采样到的地址一变就把新字**驱动到数据口**（不是写进存储器）。
- **RAM**：时钟必填（`clk = 0` 会被拒绝，整个文件不加载）。
- 一个上升沿按顺序做：拼出地址；若读使能为高，把**旧**字驱动到读数据口；若写使能为高，
  按字节写使能合并新字并写入；没有上升沿则读写都不发生。
- 时钟元件必须是**输出类**方块（红石灯 / 铁活板门 / 音符盒），因为服务器是**读**它的。
  这与 `io.txt` 相反——那里的 `clk` 必须是按钮，因为服务器要**按**它。
- 时钟不能落在数据点上，也不能与数据点正交相邻；两个存储器不能共用一个时钟
  （都会被禁用并记录原因）；io 组与存储器共用时钟时**只禁用 io 组**。
- 边沿逐 tick 采样，`performance` 模式也不会合并（与 io 组采样不同）。

### 7.3 端口键

| 角色 | 数字键 | 模拟键 | 方向 |
| --- | --- | --- | --- |
| 地址 | `address coordinate` | `analog address coordinate` | 输入 |
| 写数据 | `write data coordinate` | `analog write data coordinate` | 输入 |
| 读数据 | `read data coordinate` | `analog read data coordinate` | 输出 |
| ROM 数据 | `data coordinate` | `analog data coordinate` | 输出 |
| 写使能 | `write enable` | — | 输入 |
| 读使能 | `read enable` | — | 输入 |
| 字节写使能 | `byte write enable` | — | 输入 |

`write enable` / `read enable` 是单个坐标且必填。`byte write enable` 是列表，
`[ width = 0 ]` 表示"没有字节写使能"，否则个数必须等于 `ceil(width / 8)`，
且只在写使能为高时生效。模拟键名用空格分隔（没有下划线）。同一角色数字键与模拟键互斥，
但同一个存储器里可以混用。地址位宽最大 63 bit。

### 7.4 数据段

- `<地址>:` 设定当前地址，后面的字顺序存储；没有冒号的行从上一个地址继续。
- 地址与字都是 hex（可带 `0x`）；短字在前面补零，长字保留低位；没有数据的地址读作 0。
- 矩阵端口在打包前按行补零。

### 7.5 RAM 回写

RAM 在运行时不写盘。触发回写的时机：有玩家退出、收到 SIGINT/SIGTERM/SIGHUP（SIGKILL 不算）、
`/redpiler reset`、编译崩溃、`/memory reload` 之前。

写出的格式兼容 Logisim：每行 8 个字、行首对齐到 8 的倍数、没有字的块整段省略、
空单元补到数据位宽、文件头改成当前内核版本、原子替换（临时文件 + rename）。

### 7.6 校验

| 检查 | 时机 | 结果 |
| --- | --- | --- |
| 文件头版本、语法、宽度不符、模拟键配对 | 加载 | 整个文件禁用 |
| 端口坐标上没有元件 / 类型不对 | 编译后 | 整个文件禁用 |
| 同一存储器的两位落在同一坐标 | 加载 | 整个文件禁用 |
| 数据点碰到地址 / 时钟 / 使能点 | 加载 | 整个文件禁用 |
| 读数据与写数据共用坐标 | 加载 | 整个文件禁用 |

编译后校验失败时，未保存的 RAM 内容会被丢弃；被禁用的存储器永不回写。

### 7.7 例子

```text
v0.6.2-beta hex words addressed
origin = 0
clk = 20, 64, 0

address coordinate = [
width = 4
-8, 64, 2, +2x, 4
]

write data coordinate = [
width = 8
2, 64, 2, +2x, 8
]

read data coordinate = [
width = 8
2, 64, 4, +2x, 8
]

write enable = -8, 64, 4
read enable = -8, 64, 6

00: 00
```

---

## 8. 自动化协议

脚本连 `127.0.0.1:<io_port>`（默认 25585），一行一条消息、以 `\n` 结尾、UTF-8。
连上后服务器立即发 `HELLO`，之后每个请求各回一行。

### 8.1 连接

```text
HELLO MCHPRS_IO 0.6.2-beta 1234      ← 已编译，当前 tick 1234
HELLO MCHPRS_IO 0.6.2-beta           ← 还没编译
```

- 动词**必须大写**，小写会被拒。
- 没有斜杠命令：以 `/` 开头的行回 `ERR unknown`。
- 组名区分大小写，必须存在于 `io.txt`。

### 8.2 两种模式

| | 普通模式 | 时序控制模式（`/tick seq`） |
| --- | --- | --- |
| 进入 | 默认 | 玩家执行 `/tick seq` |
| 谁能驱动 | 任何脚本 | **一个**脚本：最先（重新）连上的那个 |
| 电路速度 | 自动，或 `/rtps 0` 冻结 | 始终冻结（rtps 0） |
| 带 offset 的 `INPUT` | `ERR mode` | 可以，预约未来 tick |
| `RADV` | 只在冻结时可用 | 可用 |
| 退出 | — | `/rtps <n>`、任意重新编译、或关服 |

`/tick seq` 要求已编译且 `performance = false`（精确采样）。它会踢掉所有已连接脚本；
最先重连的拿到独占锁，之后的连接被关闭。独占脚本断开后锁自动释放、预约清空，
下一个连接自动接替——**不需要重新敲 `/tick seq`**。

### 8.3 动词

| 动词 | 作用 | 回复 |
| --- | --- | --- |
| `INPUT <名字> <hex>` | 写整个输入组 | `OK INPUT <名字> <tick>` |
| `INPUT <名字> <offset> <hex>` | 预约 `offset` tick 之后写入（仅时序控制模式） | 批量回，见 §8.4 |
| `POUT <名字>` | 订阅输出组 | 立即回一条 `OUTPUT` |
| `OUTPUT <名字> <tick> <hex>` | 值变化时服务器推送 | — |
| `READ <名字>:<位>` | 读单个点 | `OK READ <名字>:<位> <tick> <power>` |
| `READ <x,y,z>` | 读一个坐标 | `OK READ <x,y,z> <tick> <power>` |
| `TICK NOW` | 当前 tick | `NOW <tick>` |
| `RADV <x>` | 手动推进 x tick | `OK RADV <x> <tick>` |

### 8.4 预约的批量回复

时序控制模式下，一连串 `INPUT <名字> <offset> <hex>` **不会逐条回复**。服务器收集预约，
等到这一批**静默 5 ms**（或来了任何非 `INPUT` 请求）后回一条：

```text
OK INPUT <名字> <条数>
```

`条数` 是这一批里该组被接受的预约行数。错误仍然立即回 `ERR <code>`。

脚本要按这个顺序：

1. 先发一次 `TICK NOW` 拿到基准 tick
2. 把整批一次发出（尽量单次写）
3. **先核对批量回复的条数**，再发 `RADV`
4. 预约表最多容纳 **65 536** 条未来写入

因为回复是按**预约批**（与 §3.3 的 tick 采样批次无关）的，客户端必须用**空闲超时**（每收到一行就把 deadline 重置），
而不是固定的总预算：大批次本来就要跑好几秒。仓库自带的 Python 客户端就是这么做的。

### 8.5 tick 语义

两个 tick 字段锚定在同一个 tick 的两端，所以原始差比游戏里数出的延迟少 1。

- `OK INPUT <名字> <tick>` —— 值**生效**的那个 tick（在该 tick 最开头写入，早于红石求值）。
  当前 tick 为 N 时：不带 offset → N+1；offset k → N+k。
- `OUTPUT <名字> <tick> <hex>` —— 在这个 tick 的**末尾**采样。`OK READ` 用同样的约定。

```text
游戏里数出 3t
OK INPUT  add_in 1000      ← 从 tick 1000 开始就带着新值
OUTPUT    add_out 1002 05  ← 在 tick 1002 末尾采样
1002 - 1000 = 2            ← 比 3 少 1
```

`游戏内延迟 = OUTPUT tick − OK INPUT tick + 1`。注意第一条 `OUTPUT` 未必是稳定值：
组合电路可能每个 tick 都推中间值，直到收敛。

### 8.6 `INPUT`、`POUT`、`READ`、`RADV`

- `INPUT` 一次写一整组，`<hex>` 是这一组的位打包成一个整数（第一个坐标是最高位，前面补零）。
  有时钟的组会在同一 tick 里按一次按钮。
- `POUT` 订阅并立即推一次当前值，没有 `OK` 行。对已订阅的组重复发 `POUT` 会再推一次当前值，
  订阅关系不变。重新编译会清掉所有订阅。
- 有时钟的输出组只在上升沿推；无时钟的组任意一位变化就推。
- `READ 名字:3` 读第 3 位（`bit 0` 是最后一个坐标）；`READ 10,64,10` 读任意元件：
  拉杆/灯回 0/1，红石线回 0–15。
- `TICK NOW` 返回最近完成的 tick；`RADV` 只在冻结时推进。

### 8.7 位与 hex

```text
width = 4，点 A B C D
A=1 B=0 C=1 D=1  →  1011  →  b
```

模拟组每个点一个 hex 位（0–15）。矩阵组按行分别打包（§6.7），所以 hex 长度可能大于
`ceil(width / 4)`。`width = 0` 的端口永远回 `0`。

### 8.8 错误与限制

`ERR <code>`，code 为小写：

| code | 含义 |
| --- | --- |
| `unknown` | 未知动词，或 `/` 开头的行 |
| `arg` | 参数不对 |
| `name` | 没有这个组 |
| `disabled` | 组存在但校验没过 |
| `nocompile` | 没有编译结果 |
| `range` | 数字越界 |
| `hex` | hex 格式错 |
| `mode` | 当前模式不允许 |
| `full` | 预约队列满 |
| `not_output` | 组方向不对 |
| `no_io` | 该坐标上没有元件 |
| `line` / `encoding` | 行太长 / 不是 UTF-8 |

限制：单行最长 65 536 字节；未读输出超过 16 MiB（所有连接合计 32 MiB）会被断开。

### 8.9 五分钟上手

```text
$ nc 127.0.0.1 25585
HELLO MCHPRS_IO 0.6.2-beta 0
POUT display
OUTPUT display 0 00
INPUT switches b
OK INPUT switches 1
OUTPUT display 1 0f
READ switches:0
OK READ switches:0 1 1
TICK NOW
NOW 1
```

---

## 9. Python 测试工具

`python/main.py` 走自动化协议，用你写的一行函数当标准答案比对电路。两种模式共用一套配置。

### 9.1 模式

| `mode` | 做什么 | 前置条件 |
| --- | --- | --- |
| `"correctness"` | 写一组、等一会、读结果，判值并收集延迟 | 普通模式（`rtps > 0`） |
| `"pipeline"` | 按固定 `interval` 灌一整批，一次 `RADV` 推进，逐组判定并报吞吐 | `/tick seq` |

```bash
cd python
python3 main.py            # 先改顶部的 mode 与配置变量
```

### 9.2 配置（全部是小写普通变量）

| 变量 | 含义 | 默认 |
| --- | --- | --- |
| `mode` | `"correctness"` / `"pipeline"` | `"correctness"` |
| `pin` / `pout` | 输入 / 输出组名 | `"mul_in"` / `"mul_out"` |
| `in_bits` / `out_bits` | 组位宽 | `32` / `32` |
| `total` | 总共跑多少组 | `2**20` |
| `batch` | 每批多少组（≤ 65 536），同时是进度打印间隔 | `32768` |
| `coverage` | `random` / `full`（从 `start_value` 起）/ `specified` | `"random"` |
| `seed`、`start_value`、`specified` | 向量来源细节 | `0`、`0`、`[]` |
| `wait_seconds` | correctness：每组等待；pipeline：空闲超时 + 批末收尾巴 | `0.05` |
| `delay` | 元件延迟（tick） | `27` |
| `interval` | 相邻输入间隔（tick，≥2） | `3` |
| `host` / `port` | 服务器地址 | `127.0.0.1` / `25585` |
| `svg` | `"off"` / `"error"` / `True` | `"off"` |

最小可跑示例（直接改 `main.py` 顶部，流水模式）：

```python
mode = "pipeline"
pin, pout = "mul_in", "mul_out"
in_bits = out_bits = 32
total, batch = 4096, 1024
coverage, seed = "random", 0
wait_seconds = 0.05
delay, interval = 27, 3
host, port = "127.0.0.1", 25585
```

跑前先 `/redpiler compile` 与 `/tick seq`。换成 correctness 只改 `mode = "correctness"`，
并把 `total` 调到几十~几百（每组要等满 `wait_seconds`）。

### 9.3 判定语义

- **correctness**：发 `INPUT`，在 `wait_seconds` 内只保留 `OK INPUT` 和**最后一条** `OUTPUT`
  （其余丢弃）；到点发 `POUT` 取权威值判值，并记 `delay = 最后一条 OUTPUT 的 tick - in_tick + 1`。
- **pipeline**：`TICK NOW` → 灌整批 → 核对 `OK INPUT <名字> <条数>` → `RADV span`，
  用**空闲超时**（距上一行 `wait_seconds`，不是总预算）收集推送 → 按 `in_tick + delay - 1`
  逐组判定 → 之后再收 `wait_seconds` 的尾巴。相邻推送间隔必须等于 `interval`，否则记间隔异常。
- 日志写到 `python/log/<模式>_<时间戳>.log`。

---

## 10. Android APK

`app/build.sh` 不用 Gradle 就能出可安装的 APK：改 ELF、`javac` 编 Java、`d8` 转 dex、
`aapt2` 链资源、把二进制打成 `lib/arm64-v8a/libmchprs.so`、`apksigner` 签名。

```bash
cd app
bash build.sh              # 内置不带基岩的 ~/mchprs/mchprs，出 mchprs.apk（versionCode 610）
BEDROCK=1 bash build.sh    # 内置带基岩的 ~/mchprs/mchprs-bedrock，出 mchprs-bedrock.apk（611）
```

两个 APK 的包名都是 `com.dsh.mchprs`，同一台手机上只能装一个。

包内默认附带（`WITH_ASSETS=1`）：`docs/` 顶层的两份手册、`python/`（`main.py`、
`README.md`、`core/`）与根目录的 `README.md` / `README_zh.md`；**不带** `docs/backlog/`
（台账）、`docs/upstream/` 与 `docs/mchprsio/`。

这些文件只在**第一次启动**时释放到工作目录，之后 App 不再碰它们；想重新释放，
删掉工作目录里的 `.apk-assets-done` 再启动。释放本身也只补缺、不覆盖已有文件。

环境变量可覆盖：`ANDROID_JAR`、`KEYSTORE`、`MCHPRS_BIN`、`VERSION_CODE`；
`NO_BINARY=1` 只打界面壳（`ui-preview.apk`），`WITH_ASSETS=0` 不内置任何文件。

App 启动服务器时工作目录是 `/storage/emulated/0/termux/MCHPRS_IO`，所以 `Config.toml`、
`io.txt`、`memory/`、`world/`、`logs/` 都在那里。

---

## 11. 常见问题

### 11.1 日志在哪

- 服务器：`./logs/mchprs.log.<日期>`（按天滚动），同时输出到 stdout；App 另外写
  `logs/app.log`（启动、停止、心跳）。
- 日志级别用环境变量 `MCHPRS_LOG`（默认 `INFO`），例如 `MCHPRS_LOG=debug ./mchprs`。
  旧的 `output.log` 会在启动时被移进 `logs/`。
- Python 工具：`python/log/<模式>_<时间戳>.log`。
- `/redpiler stats`：`logs/redpiler_stats_<时间戳>.md`。

### 11.2 怎么读 `/redpiler stats` 的报告

- `# nodes` / `# links` 是总量；`[node types]` 按方块类型给节点数、入/出度总和与最大值。
- `[in-degree distribution]` / `[out-degree distribution]` 覆盖 `0..=最大值` 每一档并带占比；
  出度为 0 的多是"汇"（线网、灯），入度为 0 的是源（拉杆、时钟）。
- `[nodes with out-degree >= 2]` 是扇出热点，坐标可以直接 `/tp` 过去看；
  `[nodes with in-degree >= 2]` 是汇入点（比较器、多路合并）。
- 同构电路的扇出通常呈等差梯度；出现孤立的极高扇出节点，多半是结构没按预期合并。

### 11.3 版本与兼容

- 文件格式：`io.txt` 与存储器都只要求版本戳 ≥ 0.5.0，更老的文件会被拒绝并记录原因。
- 协议没有版本协商：客户端只从 `HELLO` 里读版本串，服务端不因版本拒绝连接。
  批量预约回复 `OK INPUT <名字> <条数>` 是本仓库 0.6.1-alpha 起才有的，
  老客户端在大批次上会误判——这也是自带 Python 客户端要求"先核对条数再 `RADV`"的原因。
- 本手册对应 `0.6.2-beta`。

### 11.4 症状对照表

| 现象 | 原因 / 处理 |
| --- | --- |
| `HELLO` 里没有 tick | 没编译 —— `/redpiler compile` |
| 冻结时结果全错 | `rtps 0` 或 `/tick seq` —— 退出时序控制模式 |
| 正确性测试开头几十组全错、之后全对 | 编译前没执行 `//update -p`（上游冷启动 bug BUG-038，见 §5） |
| `ERR nocompile` | plot 上没有编译结果 |
| `ERR full` | 预约超过 65 536 条 |
| 客户端被断开 | 未读输出超过 16 MiB，或单行超过 64 KiB |
| `Failed to bind the automation bridge` | 端口被占 —— 已有另一个服务器在跑 |
| `io.txt` 的组被禁用 | 看日志里的行号；`/io reload` 重试 |
| 存储器文件被拒 | 版本头 < 0.5.0、宽度不符、或 RAM 没写 `clk` |
| `/tick seq` 被拒 | 没编译，或 `performance = true` |
| pipeline 的 `RADV` 超时 | 空闲超时对当前 tick 速率太小，或服务器没在推进 |

---

## 12. 限制与默认值

| 项 | 限制 / 默认 |
| --- | --- |
| 自动化端口 | `io_port = 25585`，只绑 `127.0.0.1` |
| 协议单行 | ≤ 65 536 字节 |
| 未读输出 | 单连接 16 MiB，合计 32 MiB |
| 预约 | 65 536 条 |
| 批次结算 | 静默 5 ms |
| 组名 | `[A-Za-z0-9_]+`，唯一，区分大小写 |
| 点位宽 | 数字 1 bit，模拟 4 bit |
| 步长 / 矩阵个数 | 步长 1–255，个数 1–1024 |
| 地址总线 | ≤ 63 bit |
| 存储器目录 | `Config.toml` 旁的 `memory/` |
| 存储器后缀 | `.rom.txt`、`.ram.txt` |

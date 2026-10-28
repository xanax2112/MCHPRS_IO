[English](manual.md) | **中文**

# MCHPRS_IO 使用手册（0.5.3-alpha）

本手册是一份**从零到跑通**的完整说明：怎么启动、配置文件每个键是什么、`io.txt` 与存储器文件怎么写、外部脚本怎么用协议驱动电路、所有命令（玩家与控制台）、以及常见报错怎么排查。

更细的格式细节在另外两份文档里，本手册只做摘要并给出链接：

- 自动化协议完整报文表：**[protocol_zh.md](protocol_zh.md)**
- `io.txt` 与 `memory/*.txt` 的完整语法：**[file-formats_zh.md](file-formats_zh.md)**

---

## 1. 这是什么

MCHPRS_IO 是 [MCHPRS](https://github.com/MCHPR/MCHPRS) 的分支，专门为**红石电路的快速调试**服务。它把整块地皮编译成一张红石图，每 tick 用极小的代价推进，因此能在原版远远跑不动的规模下测试电路。

在此之上它加了三件事：

1. **`io.txt`**：用一份文本文件声明“哪些方块是输入、哪些是输出”，不必再靠玩家手动拉拉杆。
2. **存储器（ROM / RAM）**：把 Logisim 风格的十六进制镜像直接挂成电路里的存储体。
3. **自动化协议**：外部程序通过 TCP 发纯文本报文，读输出、写输入、按 tick 精确推进。

---

## 2. 快速开始

### 2.1 启动

把服务器程序放在一个目录里直接运行（该目录就是“运行目录”）：

```sh
./mchprs
```

第一次启动会自动创建：

| 路径 | 内容 |
| --- | --- |
| `Config.toml` | 配置（首次运行写入默认值） |
| `world/players`、`world/plots` | 存档 |
| `schems/` | 投影文件（`//schem save/load`） |
| `memory/` | 存储器定义目录 |
| `memory/.rom.txt`、`memory/.ram.txt` | **隐藏模板**，说明两种文件的骨架（不会被当作存储器加载） |
| `io.txt` | IO 定义骨架，首行是版本号，含一个空输入组与一个空输出组 |
| `logs/` | 运行日志 |

### 2.2 进游戏

默认监听 `127.0.0.1:25565`（见配置 `bind_address`）。用对应版本（Minecraft 1.20.4 / 协议 765）的客户端连接即可，世界只有一块 512×512 的地皮 `p0,0`（坐标 −256…255）。

### 2.3 编译与运行

电路不会自动编译，需要手动编译：

```text
/redpiler compile            （玩家指令，世界内）
/redpiler compile            （控制台，stdin 输入同样要以 / 开头）
```

编译成功后地皮进入“已编译”状态，红石开始按服务器 tick 推进；再执行一次 `compile` 会重新编译，并把 tick 归零。

### 2.4 最小可用的 `io.txt`

```text
input  add_in  = [
origin = 0
clk = 0
width = 16
130, 35, 168, -2y, 8
127, 35, 168, -2y, 8
]
output add_out = [
origin = 0
clk = 0
width = 8
131, 34, 176, -2y, 8
]
```

- `width` 是这个端口由几个点组成（必须与坐标展开后的点数一致）。
- `clk = 0` 表示这组没有时钟；有时钟时必须指向一个**按钮**。
- 坐标可以写成 `x, y, z, ±n轴, count` 的步进缩写。

改完 `io.txt` 后可以直接 `/io reload` 生效，不用重启了（见 §7.3）。

### 2.5 第一次自动化测试

```text
POUT add_out            → OUTPUT add_out 1234 00
INPUT add_in 0003       → OK INPUT add_in 1235
```

`INPUT` 0.0003 表示“把 3 写进 16 位输入组”。服务器会在下一个 tick 让它生效，随后的 `OUTPUT` 行就是电路结果。协议细节见 §6。

---

## 3. 配置文件 `Config.toml`

配置文件在启动时读取一次，**改完要重启**才生效。缺键时会自动补写默认值，因此升级版本后旧文件不会被拒绝。

| 键 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `bind_address` | 字符串 | `0.0.0.0:25565` | 游戏监听地址 |
| `motd` | 字符串 | `Minecraft High Performance Redstone Server` | 服务器列表里显示的名字 |
| `chat_format` | 字符串 | `<{username}> {message}` | 聊天格式 |
| `max_players` | 整数 | `99999` | 最大玩家数 |
| `view_distance` | 整数 | `8` | 视距（区块） |
| `schemati` | 布尔 | `false` | 开启后投影文件按玩家 UUID 分目录存放 |
| `luckperms` | 表 | 无 | 权限数据库配置，未配置时所有权限默认放开 |
| `block_in_hitbox` | 布尔 | `true` | 碰撞箱相关开关 |
| `velocity` | 表 | 无 | Velocity 代理支持 |
| `io_port` | 整数 | `25585` | 自动化协议端口，只监听 `127.0.0.1` |
| `performance` | 布尔 | `false` | `false` = 每 tick 采样（exact）；`true` = 每批 tick 末尾采样一次 |
| `betterio` | 布尔 | `false` | **0.5.3 新增**，见下 |

### 3.1 `betterio`（0.5.3 新增）

`betterio = false` 时行为与旧版本完全一致。改成 `true` 后有两处时序变化：

| 元件 | `false` | `true` |
| --- | --- | --- |
| 石按钮 | 按下后 **10 tick** 自动弹起 | 按下后 **2 tick** 自动弹起 |
| 红石灯 | 亮起无延迟、熄灭延迟 **2 tick** | 与活板门一致，亮灭都在当 tick 生效 |

注意事项：

- 它与 `performance` **完全独立**，任意组合都允许。
- 按钮在任何模式下按一次都只能产生一次上升沿，按下期间重复触发会被忽略；`betterio` 只是把这个窗口从 10 tick 缩短到 2 tick。
- 用 `io.txt` 的 `clk` 做时钟时，这个窗口决定“最快能多久推一次时钟”。

### 3.2 命令行动参

```sh
./mchprs                 # 正常启动
./mchprs -i              # 只刷新 IO 元件（用于只改 io.txt 的快速重载）
./mchprs <world path>    # 指定世界路径
```

`-i`（`io_only`）编译时，`/io reload`、`/memory reload` 都会被拒绝——这类编译不会再读文件。

---

## 4. 世界模型

| 项 | 说明 |
| --- | --- |
| 地皮 | 只有一块 `p0,0`，512×512，坐标 −256…255，永远常驻、永不卸载 |
| 无人推进 | 没有玩家在线也照常 tick，不要求有玩家 |
| 编译 | 不自动编译，必须 `/redpiler compile` |
| tick | 32 位，回绕按取模；**每次编译清零** |
| 采样 | `performance = false` 时每 tick 采样一次 IO 组；为 `true` 时每批 tick 末尾采样 |

---

## 5. 文件格式（摘要）

完整语法、注释规则、矩阵写法、校验细节见 **[file-formats_zh.md](file-formats_zh.md)**。

### 5.1 `io.txt`

- 一个文件里可以有很多组，组名 `[A-Za-z0-9_]+`、**全局唯一**（输入输出共用一个命名空间）。
- 每个块的写法：

```text
input  组名 = [
origin = 0
clk = 0
width = N
坐标……
]
```

- `origin` 会**加到所有坐标上**（包括 `clk`）。
- `clk` 必须是**按钮**（或 `0` 表示无时钟）；一个时钟不能被两个组共用，也不能与存储器时钟撞车。
- 输入组允许的元件：**拉杆、压力板、石按钮**；输出组允许：**红石灯、铁活板门、音符盒**。
- 文件首行可以写版本号（`v0.5.3-alpha` 或 `v0.5.3-alpha hex words addressed` 都接受，0.5.0 及以上），不写也能加载。
- `width = 0` 且没有坐标的空组是合法的（服务器生成的骨架就是这样），可以用 `/coord` 往里填。

### 5.2 存储器 `memory/*.rom.txt` / `*.ram.txt`

```text
v0.5.3-alpha hex words addressed
origin = 0
clk = 0, 0, 0

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

- 端口角色：`address coordinate` / `data coordinate`（ROM）/ `write data coordinate` / `read data coordinate` / `write enable` / `read enable` / `byte write enable`；模电版本在键名加 `analog `。
- 位置列第一项必须是 `width = N`；模电列表还要 `rs_width`，且 `width = rs_width × 4`。
- RAM 的 `clk` 必须指向真实按钮；ROM 可以无时钟（跟随地址变化）。
- RAM 只在事件点落盘：玩家退出、关服、`/redpiler reset`、编译崩溃、`/memory reload`、`/io reload`。
- 文件名以 `.` 开头的（如模板 `.rom.txt`）**不会被加载**。

---

## 6. 自动化协议（摘要）

完整报文表：**[protocol_zh.md](protocol_zh.md)**。端口默认 `127.0.0.1:25585`，纯文本、一行一条。

### 6.1 连接与模式

连接后服务器主动发：

```text
HELLO MCHPRS_IO 0.5.3-alpha 1234     ← 已编译，1234 是当前 tick
HELLO MCHPRS_IO 0.5.3-alpha          ← 未编译，没有 tick 字段
```

两种模式：

| 模式 | 进入方式 | 特点 |
| --- | --- | --- |
| 普通模式 | 默认 | 电路自动按 `rtps` 推进；`INPUT` 不能带偏移 |
| 时序控制模式 | 玩家执行 `/tick seq` | 冻结电路、踢掉所有脚本、只允许一个脚本重连；允许 `INPUT <名字> <偏移> <hex>` 预约到未来 tick |

### 6.2 动词

| 报文 | 作用 | 应答 |
| --- | --- | --- |
| `INPUT <名字> <hex>` | 写整个输入组 | `OK INPUT <名字> <生效 tick>` |
| `INPUT <名字> <偏移> <hex>` | 预约到未来（仅时序控制模式） | 同上 |
| `POUT <名字>` | 订阅输出组并立刻取一次当前值 | `OUTPUT <名字> <tick> <hex>` |
| `OUTPUT <名字> <tick> <hex>` | 服务器推送 | — |
| `READ <名字>:<位>` | 读组里某一位 | `OK READ … <tick> <功率>` |
| `READ <x,y,z>` | 按坐标读 | `OK READ … <tick> <功率>` |
| `TICK NOW` | 问当前 tick | `NOW <tick>` |
| `RADV <n>` | 冻结时手动推进 n tick | `OK RADV <n> <tick>` |

**动词只接受全大写**，`input` 会得到 `ERR unknown`。

### 6.3 重复 `POUT`（0.5.3 起）

对一个**已经订阅过**的输出组再发一次 `POUT`，服务器会**再回一条当前值**（`OUTPUT`），订阅关系与后续推送规则都不变。适合在长脚本里“随时戳一下现在是多少”。

### 6.4 tick 的两种含义

这是最容易踩的一处：

- `OK INPUT` 里的 tick 是**输入生效的 tick**：写入发生在该 tick 最开头，这一 tick 从头到尾都带着新值。
- `OUTPUT` 里的 tick 是**采样 tick**：值取自该 tick 的**末尾**（算完这一 tick、处理完存储器之后）。

所以两者相减比玩家在游戏里数出来的延迟**少 1**：

```text
玩家在游戏里数出 3t 延迟
OK INPUT  add_in 1000
OUTPUT    add_out 1002 05
1002 - 1000 = 2          ← 差值是 2，真实延迟是 3
```

即 `玩家观测延迟 = OUTPUT tick − OK INPUT tick + 1`。

### 6.5 错误码

`ERR <小写码>`，常见：`unknown`（动词不认）、`arg`（参数不对）、`name`（组名不存在）、`nocompile`（未编译）、`disabled`（组被禁用）、`range`、`hex`（hex 位数不对）、`mode`（模式不允许，如普通模式下带偏移）、`full`（预约队列满）、`not_output`（`POUT` 了输入组）、`no_io`（坐标不是 IO 元件）。

---

## 7. 命令总表

玩家的指令在游戏聊天框里敲，控制台的指令在服务器 stdin 输入（同样以 `/` 开头）。

### 7.1 编译与运行

| 命令 | 位置 | 说明 |
| --- | --- | --- |
| `/redpiler compile [参数]` | 玩家 + 控制台 | 编译当前地皮（`/rp c`） |
| `/redpiler inspect` | 玩家 | 打印某处的调试信息 |
| `/redpiler reset` | 玩家 + 控制台 | 退出编译态 |
| `/rtps <数值>` | 玩家 + 控制台 | 设置每秒 tick 数；不带参数打印最近 10s/1m/5m/15m 的 RTPS |
| `/radv <n>` | 玩家 + 控制台 | 冻结状态下手动推进 n tick |
| `/stop` | 玩家 + 控制台 | 优雅关服 |
| `/version` | 玩家 + 控制台 | 版本信息 |

`/redpiler compile` 的参数（Tab 补全会给出完整长参数）：

| 参数 | 作用 |
| --- | --- |
| `--optimize` | 开启优化 pass（编译更慢但运行更快） |
| `--export` | 导出图到二进制 |
| `--io-only`（`-i`） | 只刷新 IO 元件 |
| `--update` | 重置后更新输入区所有方块 |
| `--export-dot` | 导出 dot 图 |
| `--wire-dot-out` | 把点状粉当作输出（屏幕电路） |
| `--illegal-states-out` | 把非法线态当作输出 |
| `--wire-cross-out` | 把十字粉当作输出 |
| `--print-after-all` | 每个 pass 后打印 RIL |
| `--print-before-backend` | 后端编译前打印 RIL |
| `--passes=<列表>` | 指定要跑的 pass（仅 rilc 用） |

### 7.2 `/tick` 与时间控制（0.5.3 扩展）

| 命令 | 等价于 | 说明 |
| --- | --- | --- |
| `/tick` | — | 打印当前 tick |
| `/tick seq` | — | 进入时序控制模式（冻结、踢脚本、`rtps 0`） |
| `/tick freeze` | `/rtps 0` | 冻结；会记住冻结前的 rtps |
| `/tick unfreeze` | 恢复 | 恢复成**冻结前的那个 rtps 值** |
| `/tick step <偶数>` | `/radv (n/2)` | `step 2` = `radv 1`；**只接受偶数**，奇数直接报错 |
| `/tick rate [数值]` | `/rtps` | 设置 rtps；**不带参数等于设为 20**（注意 `/rtps` 不带参数是打印报告，两者空参行为不同） |

### 7.3 `/io`

| 命令 | 说明 |
| --- | --- |
| `/io list` | 列出组数、格式错误行号、已编译时追加禁用数与组名 |
| `/io reload` | 重新读取 `io.txt`，不用重启（0.5.3 新增） |

两者玩家与控制台都能用。`/io reload` 的规则与 `/memory reload` 一致：只有在**未编译**状态才真正生效；已编译时会先退出编译态再重读；`-i` 编译时拒绝执行。

### 7.4 `/memory`

| 命令 | 说明 |
| --- | --- |
| `/memory reload` | RAM 落盘 → 退出编译态 → 重读 `memory/` → 等玩家再编译 |

### 7.5 `/coord`：木斧取点写坐标（0.5.3 新增）

前提：用 **WorldEdit 木斧**右键方块选好点（聊天框会弹紫色 `Second position set to (x, y, z)`），`/coord` 取的是**第二点**。

```
/coord in <file> [name]           进入目标（切换后续 new/add 的作用对象）
/coord new <type> [步长] [个数] [方向]
/coord add <type> [步长] [个数] [方向]
/coord undo
/coord redo
```

**`in` 的目标**

| 写法 | 含义 |
| --- | --- |
| `/coord in io.txt <组名>` | 进入 `io.txt` 的某个组（不区分输入输出） |
| `/coord in xx.rom` | 进入 `memory/xx.rom.txt` |
| `/coord in xx.ram` | 进入 `memory/xx.ram.txt` |

- `<file>` 与 `<组名>` 都支持 Tab 补全；隐藏模板（`.rom.txt` / `.ram.txt`）不会出现在补全里。
- 连续 `in` 多次时，后一次覆盖前一次。

**`type` 短码**

| `type` | 含义 | 坐标数 |
| --- | --- | --- |
| `clk` | 时钟 | 1（只能用 `new`） |
| `addr` | 地址 | 多 |
| `rd` | 读数据（ROM 的数据口也叫 `rd`） | 多 |
| `wd` | 写数据 | 多 |
| `we` | 写使能 | 多 |
| `re` | 读使能 | 多 |
| `bwe` | 字节写使能 | 多 |
| `a_addr` / `a_rd` / `a_wd` | 模电地址 / 读数据 / 写数据 | 多 |

进 `io.txt` 的组之后，`type` 只有 `clk` 与 `data` 两个（Tab 补全会按上下文切换列表）。

**步长、个数、方向**

- 步长 = 文件语法里 `+<n>x` 的那个 `n`；个数 = 该段展开成几个点。
- 方向沿用 `//stack` 的规则：省略 = `me`（玩家朝向），也支持 `l/left`、`r/right`、`u/up`、`d/down`、`n/s/e/w`。
- 不写步长/个数 = 只写入当前这一个点。

**写文件规则**

- `new` = **覆盖**该端口的坐标；`add` = **追加**到末尾。
- 单坐标的 `type`（如 `clk`）只能用 `new`。
- 结果超过文件里写死的 `width`（模电对照 `rs_width`）时**不予执行**。
- 文件里 `origin` 非零时，写入的是**减去 origin 后的相对坐标**。
- `width` 不会被自动修改：写少了没问题，但**编译时点数与 `width` 不相等就会按现有校验禁用该组/该文件**。
- 不做写前备份；`undo`/`redo` 深度 **8**，超过则丢弃最旧的一条。
- 只能在**未编译**时使用，回车即保存。
- 写成功会弹紫色消息：`10, 64, 5, +2x, 8 -> read data`（坐标串在前、`type` 全码在后）。
- `/coord` 改过文件后，下一次 `/redpiler compile` 会**自动重载一次**这些文件。

### 7.6 `//schem`：投影文件（0.5.3 新增）

| 命令 | 等价于 | 说明 |
| --- | --- | --- |
| `//schem load <文件名>` | `//load` | 把投影读进剪贴板 |
| `//schem save <文件名>` | `//save` | 把剪贴板写成投影 |
| `//schem list` | — | 列出 `./schems` 里的投影文件 |

文件名必须以 `.schem` 或 `.schematic` 结尾。

### 7.7 其它世界编辑命令

沿用 MCHPRS 的 WorldEdit 实现：`//pos1` `//pos2` `//set` `//copy` `//paste` `//stack` `//move` `//expand` `//contract` `//rotate` `//flip` `//undo` `//redo` 等，别名如 `//s` = `//stack`。

`//stack` 的方向参数省略时等价于 `me`（玩家朝向）。

### 7.8 地皮命令

`/plot info`、`/plot claim`、`/plot auto`、`/plot middle`、`/plot visit`、`/plot tp`、`/plot lock`、`/plot unlock`、`/plot select`（可用 `/p` 前缀与单字母别名）。

---

## 8. 典型工作流

### 8.1 手动测一颗电路的延迟

1. `/tick seq` 进入时序控制模式；
2. 脚本连上协议端口（第一个连上的成为独占脚本）；
3. `POUT` 你要看的输出；
4. `INPUT` 一个测试向量，记下 `OK INPUT` 里的 tick；
5. `RADV 1` 逐 tick 推进，观察 `OUTPUT`；
6. 按 §6.4 换算真实延迟（差值 +1）。

### 8.2 用 `/coord` 布置一组输出点位

1. 木斧右键点一个方块（第二点）；
2. `/coord in io.txt none_out`；
3. `/coord new data 2 8 east` → 写一行 `x, y, z, +2x, 8`；
4. `/io reload` 让 `io.txt` 生效（未编译时）；
5. `/redpiler compile`。

写错了就 `/coord undo`，最多退回 8 步。

---

## 9. 报错排查

| 现象 | 原因与处理 |
| --- | --- |
| `io.txt: 0 group(s), N format error(s)` | 按 `/io list` 的行号看错误；常见是 `width` 与坐标数不符、组名重复、`clk` 指向非按钮 |
| `the clock (x,y,z) is not a button` | `clk` 必须是石按钮，或用 `clk = 0` |
| `(x,y,z) is not a lever, pressure plate or button` | 输入组用了不支持的元件 |
| `ERR hex` | `INPUT` 的 hex 位数不对（每 4 位一个十六进制字符） |
| `ERR nocompile` | 还没编译就发了协议请求 |
| `ERR mode` | 普通模式下用了带偏移的 `INPUT`，先 `/tick seq` |
| `Refused: that would need N points but width = M` | `/coord` 写入超过 `width`，先改文件里的 `width` |
| 灯/按钮时序与预期差 1~2 tick | 检查 `betterio` 配置，以及 §6.4 的 tick 换算 |

---

## 10. 相关文档

- **[protocol_zh.md](protocol_zh.md)** — 自动化协议完整报文表、模式与错误码
- **[file-formats_zh.md](file-formats_zh.md)** — `io.txt` 与存储器文件的完整语法与校验
- **[Redpiler.md](Redpiler.md)** — 编译器内部原理（英文）
- **[RIL.md](RIL.md)** — 中间表示语法（英文）
- **[Adding_a_block.md](Adding_a_block.md)** — 如何加新方块（英文）

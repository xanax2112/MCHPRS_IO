[English](README.md) | **中文**

# MCHPRS_IO

> **0.6.2-beta 已发布 · 正式公测** —— 到 [Releases](https://github.com/xanax2112/MCHPRS_IO/releases)
> 下载最新版：两个 APK（带基岩 / 不带基岩）与 Linux、Windows、macOS 各平台二进制，压缩包里附带使用手册与 Python 工具。
>
> 本版更新：
>
> - 协议基本稳定，正式公测
> - 性能大提升：自动化测试吞吐从 80 组/秒 到 25 万组/秒
> - Python 测试代码更加完善
> - 基岩版作为可选功能，默认不编译；APK 分为带基岩版和不带基岩版两个版本
> - 修复若干 bug

MCHPRS_IO 是 [MCHPRS](https://github.com/MCHPR/MCHPRS) 的一个分支，用来解决红石的三大痛点：**电路无法快速调试与验证**、**电路无法与外部程序沟通**，以及**红石电路没办法高效存储、读写大量数据**。

为此它内置了 **ROM 与 RAM**（在红石里直接存取大量数据）、用**接近 HDL 的语法**声明方块 IO 坐标的 **`io.txt`**，以及一套纯文本的**自动化协议**，让外部程序直接驱动并观察电路。

> 新手教程视频正在制作中，敬请期待。

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## 仓库结构

| 路径 | 是什么 |
| --- | --- |
| `crates/`、`src/` | Rust 服务端本体 |
| `docs/` | 协议、文件格式、编译器内部文档 |
| `vendor/` | vendored 的基岩版协议代码（`bedrock-rs`，协议 766） |
| `python/` | 自动化协议测试脚本 —— [`python/README.md`](python/README.md) |
| `app/` | 把服务端打包成独立 Android APK —— [`app/README.md`](app/README.md) |
| `run/` | 示例运行目录（`Config.toml`、`io.txt`、`memory/`、`world/`） |
| `tests/` | 集成测试，`cargo test` 运行 |
| `docs/backlog/BACKLOG.md` | 项目台账：设计决策、未结项、已知问题 |

## 快速开始

### 1. 下载即玩（推荐）

到 **[Releases](https://github.com/xanax2112/MCHPRS_IO/releases)** 下载最新版即可，**不需要 Rust 工具链，也不用自己编译**。

**Android**：装 `mchprs-0.6.2-beta-java.apk`（不带基岩）或 `mchprs-0.6.2-beta-bedrock.apk`（带基岩）。
打开 App 先给存储权限，首次启动会把使用手册与 Python 工具释放到工作目录
`/storage/emulated/0/termux/MCHPRS_IO`（只解压一次，之后不再覆盖你的改动）；回到主界面点「启动」起服务，
控制台就在同一个界面里，`/` 开头的命令可以直接敲。`Config.toml`、`io.txt`、`world/`、`logs/` 都在那个工作目录下。

**Linux / Windows / macOS**：下载对应平台的压缩包，解压后直接运行 `mchprs`（Windows 是 `mchprs.exe`）。
它在**当前工作目录**读写 `Config.toml`、`world/`、`logs/`，所以先建一个空目录再跑：

```bash
mkdir -p ~/mchprs-server && cd ~/mchprs-server
/path/to/mchprs
```

客户端用 Minecraft **Java 版 1.20.4**（协议 765），连 `<主机>:25565`；带基岩的构建另外支持基岩版 1.21.51（协议 766）。

### 2. 自己编译（可选）

只有要改代码、或者要自己开基岩版时才用得上。

```bash
# 只要 Java 版
cargo build --release

# 需要基岩版：feature 默认关闭，必须显式打开（会把 vendor/bedrock-rs 一起编进来）
cargo build --release --features bedrock
```

产物在 `target/release/mchprs`。同样建议在独立目录里跑，否则 `world/`、`logs/` 会写进源码树。

首次启动会在工作目录生成默认文件：

```text
.
├── Config.toml      缺失时用默认值生成
├── io.txt           IO 组定义（可选）
├── memory/          *.rom.txt 与 *.ram.txt（可选）
├── logs/            自动创建
└── world/           存档
```

日常基本只需要改 `Config.toml`，头一次跑最要紧的是 `bind_address`、`io_port`、`view_distance` 三项。

### 3. 进服与搭电路

1. 用 **1.20.4** 客户端连 `<主机>:25565`（`Config.toml` 的 `bind_address`，默认 `0.0.0.0:25565`）。
2. 出生在一块 **512×512** 的地皮 **`p0,0`** 上。它有没有玩家都在 tick，而且永远不会 unloading。
3. 搭电路（拉杆、按钮、红石灯、音符盒……）。`/setblock`、worldedit、`//schem` 都可用。
4. 用 **`/redpiler compile`** 编译。电路**永远不会**自动编译 —— 不主动编译就不进编译器，
   但红石本身照常 tick。

### 4. 声明方块 IO（`io.txt`）

`io.txt` 告诉自动化协议哪些方块是输入、哪些是输出：

```toml
input switches = [
origin = 0
clk = 0
width = 2
10, 64, 10
12, 64, 10
]

output lamps = [
origin = 0
clk = 0
width = 2
20, 64, 10, +2x, 2
]
```

`/io reload` 可以不重启直接重读。完整语法与校验规则见
[docs/Instruction-manual-zh.md](docs/Instruction-manual-zh.md)。

### 5. 从外部驱动

自动化端口（`io_port`，默认 `25585`）**只监听回环**，且没有鉴权。一行一条报文，动词全大写：

```text
HELLO MCHPRS_IO 0.6.2-beta <tick>     ← 连上后服务端先发
INPUT switches 1                       → OK INPUT switches <tick>
POUT lamps                             → OUTPUT lamps <tick> 0a
TICK NOW                               → NOW <tick>
```

完整参考：**[docs/Instruction-manual-zh.md](docs/Instruction-manual-zh.md)**。

### 6. 自动验证电路（可选）

[`python/`](python/README.md) 里有三个脚本，直接走自动化协议，用你写的一行 Python 函数当"标准答案"验电路：

| 模式 | 判什么 |
| --- | --- |
| `MODE = "correctness"` | 只判结果对不对，顺带打印延迟统计 |
| `MODE = "pipeline"` | 流水输入（需 `/tick seq`）间隔恒定、不阻塞 |

```bash
cd python
python3 main.py             # 先改 MODE 和文件顶部的配置区
```

### 7. 常见问题

| 现象 | 原因 |
| --- | --- |
| `HELLO` 里没有 `<tick>` | 电路没编译 —— 执行 `/redpiler compile` |
| 电路看着没错，结果却全错 | plot 被冻结了（`/tick seq`、`rtps 0`）；解除冻结或回普通模式 |
| 自动化客户端被断开 | 未读输出积压超过 1 MiB，或单行超过 64 KiB |
| 首次 `cargo build --release --features bedrock` 要 10 分钟以上 | 开了 `lto = "fat"`；之后只重编改动过的 crate。去掉 feature 会快很多 |

## 配置

| 键 | 默认 | 说明 |
| --- | --- | --- |
| `bind_address` | `"0.0.0.0:25565"` | MC 监听地址 |
| `motd`、`chat_format`、`max_players`、`view_distance` | — | 常规服务器设置 |
| `schemati` | `false` | 启用 `.schem` 放置 |
| `block_in_hitbox` | `true` | 允许方块落在玩家位置 |
| `luckperms`、`velocity` | — | 可选集成 |
| `io_port` | `25585` | 自动化端口（回环） |
| `performance` | `false` | 改为每批采样一次并禁用预约，用响应速度换吞吐 |

缺失的键会补写默认值；旧键（`automation_port`、`mode`、`automation_per_tick_push`、`automation_data_port_events`、`auto_redpiler`）会自动迁移或删除。

## 一分钟看懂 `io.txt`

```toml
input switches = [
origin = 0
clk = 0
width = 2
10, 64, 10
12, 64, 10
]

output lamps = [
origin = 0
clk = 0
width = 2
20, 64, 10, +2x, 2
]
```

- 角色：`input`（拉杆、压力板）、`output`（灯、活板门、音符盒）、`analog_input` / `analog_output`（点状红石粉）。
- 键：`origin`、`clk`（`0` 表示无时钟，否则必须是按钮）、`width`（= 点数）。模电组还要写 `rs_width`（点数），且 `width = rs_width × 4`。
- 坐标：`x, y, z`、`x, y, z, +2x, 4`（元件通常隔一格摆一个，所以坐标步长是 2），或 `matrix: ...` 表示网格与屏幕。

完整语法、校验规则与示例见 **[docs/Instruction-manual-zh.md](docs/Instruction-manual-zh.md)**。

## 存储器

`memory/*.rom.txt`（只读）与 `*.ram.txt`（可写），Logisim 兼容：

```text
v0.5.3-alpha hex words addressed
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

首行版本必须为 0.5.0 或更新；每个端口列表第一项是 `width = N`；RAM 在玩家退出、关服、`/redpiler reset`、编译崩溃和 `/memory reload` 时落盘。细节见 **[docs/Instruction-manual-zh.md](docs/Instruction-manual-zh.md)**。

## 自动化协议

连 `127.0.0.1:25585`，一行一条报文，**动词只接受全大写**，没有 `/` 命令。

```text
HELLO MCHPRS_IO 0.5.3-alpha <tick>
INPUT <name> <hex>            → OK INPUT <name> <tick>
POUT <name>                   → OUTPUT <name> <tick> <hex>
READ <name>:<bit>             → OK READ <name>:<bit> <tick> <power>
TICK NOW                      → NOW <tick>
RADV <x>                      → OK RADV <x> <tick>
```

两种模式：**普通模式**（默认）与**时序控制模式**（`/tick seq`，冻结电路、锁定一个脚本、允许带偏移的 `INPUT`，用于流水线电路）。完整说明见 **[docs/Instruction-manual-zh.md](docs/Instruction-manual-zh.md)**。

## 命令

| 命令 | 作用 |
| --- | --- |
| `/tick` | 当前 tick |
| `/tick seq` | 预约模式：踢脚本、rtps 0、锁定第一个重连 |
| `/tick freeze` / `/tick unfreeze` | 冻结（`rtps 0`）、恢复冻结前的 rtps |
| `/tick step <偶数>` | 手动推进，`step 2` = `/radv 1` |
| `/tick rate [数值]` | 设置速率，不带参数等于设为 20 |
| `/rtps [n\|unlimited]` | 推进速度（不带参数显示实测速率） |
| `/radv <n>` | 手动推进 n 个 tick |
| `/redpiler compile [参数]` | 编译（`-d` 点、`-c` 十字、`-i` 只刷新 IO） |
| `/redpiler inspect` / `reset` | 编译统计 / 退出编译状态 |
| `/io list`、`/io reload` | `io.txt` 汇总、重读 `io.txt` |
| `/memory reload` | 重读存储器文件 |
| `/coord in\|new\|add\|undo\|redo` | 木斧第二点写进文件坐标 |
| `//schem load\|save\|list` | 投影文件 |
| `/version`、`/stop` | 版本信息、优雅关服 |

终端里同样可用；需要玩家目标的命令在终端会被拒绝。

## 文档

- [docs/Instruction-manual-zh.md](docs/Instruction-manual-zh.md) —— 完整手册：编译运行、`io.txt`、存储器文件、自动化协议、测试工具（[English](docs/Instruction-manual.md)）
- [docs/upstream/Redpiler.md](docs/upstream/Redpiler.md)、[docs/upstream/RIL.md](docs/upstream/RIL.md)、[docs/upstream/Adding_a_block.md](docs/upstream/Adding_a_block.md) —— 编译器内部（英文）
- [python/README.md](python/README.md) —— 协议测试脚本（`correctness` / `pipeline`）
- [app/README.md](app/README.md) —— 把服务端打包成独立 Android APK 的工程
- [docs/backlog/BACKLOG.md](docs/backlog/BACKLOG.md) —— 项目台账与设计决策

## 关注我们

- **开发者**：Xanax21 —— 哔哩哔哩 <https://b23.tv/SJ6cg8m>，GitHub [@xanax2112](https://github.com/xanax2112)
- **测试与宣传**：FishYuDuoDuo —— GitHub [@FISHduoduo123](https://github.com/FISHduoduo123)，哔哩哔哩 <https://b23.tv/kZae7Xw>
- **95 的红石喵窝** —— QQ 群 **784478204**
- **先进红石数电技术研讨组（ARS）** —— QQ 群 **1076278498**

## 致谢

- 上游 [MCHPRS](https://github.com/MCHPR/MCHPRS)：红石编译器与模拟器的全部基础
- 基岩版协议栈：[bedrock-rs](https://github.com/bedrock-crustaceans/bedrock-rs)（**vendored 完整副本**，
  带 5 处本地补丁，见 [`docs/backlog/BEDROCK.md`](docs/backlog/BEDROCK.md)）、
  [nbtx](https://github.com/bedrock-crustaceans/nbtx)（NBT 编解码）
- 网络与异步：[tokio](https://github.com/tokio-rs/tokio)、
  [CloudburstMC](https://github.com/CloudburstMC) 的 RakNet 与协议实现（作对照参考）、
  [Geyser](https://github.com/GeyserMC/Geyser)（编解码对照）
- Java 版存档 NBT：[hematite-nbt](https://github.com/StackDoubleFlow/hematite_nbt)
- 还有 [Rust 生态](https://crates.io) 里其余一堆库（`serde`、`flate2`、`tokio`、`uuid` 等）

## 衍生版声明

本项目是 [MCHPRS](https://github.com/MCHPR/MCHPRS) 的**衍生版（fork）**：上游是把红石编译成图、
只模拟这张图的服务器；本仓库在它之上加了 IO / 自动化层、`io.txt` 与存储器文件格式、
基岩版支持（可选 feature）以及 Android APK 打包。

- 原项目地址：<https://github.com/MCHPR/MCHPRS>
- 原项目协议：**MIT** — Copyright (c) 2020 Ojas Landge
- 本衍生版同样采用 **MIT 协议**（见 [LICENSE](./LICENSE)），对上游代码的修改与新增部分也按 MIT 发布。

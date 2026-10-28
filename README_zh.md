[English](README.md) | **中文**

# MCHPRS_IO

MCHPRS_IO 是 [MCHPRS](https://github.com/MCHPR/MCHPRS) 的一个分支，用来解决红石的两大痛点：**电路无法快速调试与验证**，以及**电路无法与外部程序沟通**。

为此它内置了 **ROM 与 RAM**、用**接近 HDL 的语法**声明方块 IO 坐标的 **`io.txt`**，以及一套纯文本的**自动化协议**，让外部程序直接驱动并观察电路。

> 配套的快速开始演示**尚未实现**。

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## 快速开始

```bash
cargo build --release
./target/release/mchprs
```

所有路径都相对工作目录：

```text
.
├── Config.toml      缺失时自动生成
├── io.txt           IO 组声明（可选）
├── memory/          *.rom.txt 与 *.ram.txt（可选）
├── logs/            自动创建
└── world/           存档
```

用 Minecraft 1.20.4 客户端连 `bind_address`（默认 `0.0.0.0:25565`），在地皮 `p0,0`（512×512）上搭电路，然后 `/redpiler compile`。

- 地皮**有没有玩家都照常 tick**，永不卸载；全服只有这一块地皮。
- 电路**从不自动编译**。
- 自动化端口 `io_port`（默认 25585）**只监听 127.0.0.1**，且不做鉴权。

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

完整语法、校验规则与示例见 **[docs/file-formats_zh.md](docs/file-formats_zh.md)**。

## 存储器

`memory/*.rom.txt`（只读）与 `*.ram.txt`（可写），Logisim 兼容：

```text
v0.5.2-alpha hex words addressed
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

首行版本必须为 0.5.0 或更新；每个端口列表第一项是 `width = N`；RAM 在玩家退出、关服、`/redpiler reset`、编译崩溃和 `/memory reload` 时落盘。细节见 **[docs/file-formats_zh.md](docs/file-formats_zh.md)**。

## 自动化协议

连 `127.0.0.1:25585`，一行一条报文，**动词只接受全大写**，没有 `/` 命令。

```text
HELLO MCHPRS_IO 0.5.2-alpha <tick>
INPUT <name> <hex>            → OK INPUT <name> <tick>
POUT <name>                   → OUTPUT <name> <tick> <hex>
READ <name>:<bit>             → OK READ <name>:<bit> <tick> <power>
TICK NOW                      → NOW <tick>
RADV <x>                      → OK RADV <x> <tick>
```

两种模式：**普通模式**（默认）与**时序控制模式**（`/tick seq`，冻结电路、锁定一个脚本、允许带偏移的 `INPUT`，用于流水线电路）。完整说明见 **[docs/protocol_zh.md](docs/protocol_zh.md)**。

## 命令

| 命令 | 作用 |
| --- | --- |
| `/tick` | 当前 tick |
| `/tick seq` | 预约模式：踢脚本、rtps 0、锁定第一个重连 |
| `/rtps [n\|unlimited]` | 推进速度（不带参数显示实测速率） |
| `/radv <n>` | 手动推进 n 个 tick |
| `/redpiler compile [参数]` | 编译（`-d` 点、`-c` 十字、`-i` 只刷新 IO） |
| `/redpiler inspect` / `reset` | 编译统计 / 退出编译状态 |
| `/io list`、`/memory reload` | `io.txt` 汇总、重读存储器 |
| `/version`、`/stop` | 版本信息、优雅关服 |

终端里同样可用；需要玩家目标的命令在终端会被拒绝。

## 文档

- [docs/file-formats_zh.md](docs/file-formats_zh.md) —— `io.txt` 与存储器文件参考
- [docs/protocol_zh.md](docs/protocol_zh.md) —— 自动化协议
- `MANUAL-0.5.2-alpha.md` —— 完整说明书（中文）
- `BACKLOG.md` —— 项目台账与设计决策

## 衍生版声明

本项目是 [MCHPRS](https://github.com/MCHPR/MCHPRS) 的**衍生版（fork）**。

- 原项目地址：<https://github.com/MCHPR/MCHPRS>
- 原项目协议：**MIT** — Copyright (c) 2020 Ojas Landge
- 本衍生版同样采用 **MIT 协议**（见 [LICENSE](./LICENSE)）。

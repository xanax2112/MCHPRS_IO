[English](protocol.md) | **中文**

# 自动化协议

脚本用 TCP 连 `127.0.0.1:25585`，纯文本，一行一条报文（`\n` 结尾）。连上后服务器先发一条 `HELLO`，之后每个请求对应一行应答。

这是简版说明，文件格式见 `file-formats.md`。

## 连接

```text
HELLO MCHPRS_IO 0.5.2-alpha 1234      ← 电路已编译，当前 tick 是 1234
HELLO MCHPRS_IO 0.5.2-alpha           ← 还没编译
```

第三个字段没有，说明电路还没编译，先在游戏里执行 `/redpiler compile`。

要记住的三件事：

- **动词必须全大写**，`input` 会被拒绝，`INPUT` 才行。
- 脚本**没有 `/` 命令**，任何 `/` 开头的行都回 `ERR unknown`。
- 组名区分大小写，必须是 `io.txt` 里声明过的。

## 两种模式

| | 普通模式 | 时序控制模式 |
| --- | --- | --- |
| 怎么进入 | 默认；手动步进时执行 `/rtps 0` | 玩家执行 `/tick seq` |
| 谁能驱动 | 所有已连接的脚本 | **一个**：踢完后第一个重连的脚本 |
| 电路速度 | 自动推进，或被 `/rtps 0` 冻结 | 始终冻结（rtps 0） |
| 带偏移的 `INPUT` | 拒绝（`ERR mode`） | 可以，预约到未来的 tick |
| `RADV` | 冻结时可用 | 可用 |
| 怎么退出 | 不用退出，这就是默认状态 | `/rtps <n>`，或任何一次重新编译 |

`/tick seq` 会**踢掉所有已连接的脚本**。执行后重新连接：第一个连上的拿到锁，后到的直接断开。锁定的脚本断开后，需要玩家再执行一次 `/tick seq`。

大多数测试用普通模式就够了：读输出、写输入、看变化。时序控制模式是为**流水线电路**准备的：流水线只有在某一拍精确灌入测试向量、再在后面几拍精确观察输出时才有意义，这期间电路不能自己往前走。

## 动词表

| 动词 | 作用 | 应答 |
| --- | --- | --- |
| `INPUT <name> <hex>` | 写入整个输入组 | `OK INPUT <name> <tick>` |
| `INPUT <name> <offset> <hex>` | 预约到 `offset` 个 tick 之后（仅时序控制模式） | `OK INPUT <name> <tick>` |
| `POUT <name>` | 订阅一个输出组 | `OUTPUT <name> <tick> <hex>` |
| `OUTPUT <name> <tick> <hex>` | 服务器主动推送的变化 | — |
| `READ <name>:<bit>` | 读组里的一个点 | `OK READ <name>:<bit> <tick> <power>` |
| `READ <x,y,z>` | 按坐标读一个元件 | `OK READ <x,y,z> <tick> <power>` |
| `TICK NOW` | 问当前 tick | `NOW <tick>` |
| `RADV <x>` | 冻结时手动推进 x 个 tick | `OK RADV <x> <tick>` |

### `INPUT`

一次写入整组；`<hex>` 是把整组位按顺序打包成的一个整数（见下文「位序与 hex」）。

- 不带偏移：下一个 tick 生效。
- 带偏移 `k`：在收到报文时的 tick（记为 N）之后的第 k 个 tick 生效，也就是 N+k。`k = 1` 和不带偏移等价，范围 1–65535。
- 组里有时钟时，同一个 tick 会脉冲一次按钮；没有时钟的组只写数据。

### `POUT` / `OUTPUT`

`POUT 组名` 订阅一个输出组，服务器立刻回一条 `OUTPUT` 表示当前值，没有单独的“成功”应答。

之后值变化时服务器会推送 `OUTPUT`。有时钟的组只在按钮上升沿推一次，没有时钟的组任一位变化就推。重新编译会清空所有订阅，需要重新 `POUT`。

### `READ`

`READ 组名:3` 读组里的第 3 位（`bit 0` 是最后一个坐标）。`READ 10,64,10` 按坐标读任意元件：拉杆和灯回 0/1，红石粉回 0–15。

### `TICK NOW` 与 `RADV`

`TICK NOW` 返回最新的已完成 tick。`RADV 10` 手动推进 10 个 tick，只在电路冻结时可用；自动推进时电路自己会走。

## 位序与 hex

整组是一个整数：**第一个坐标是最高位**，hex 文本在前面补零。`matrix:` 组按行分别补零，具体打包规则见 `file-formats.md` §4。

```text
width = 4，点位 A B C D
A=1 B=0 C=1 D=1  →  1011  →  b
```

模电组每个点一个十六进制位，取值 0–15。

## 错误

错误只有一行：`ERR <码>`，码是小写短词：

| 码 | 含义 |
| --- | --- |
| `unknown` | 未知动词，或 `/` 开头的行 |
| `arg` | 参数不对 |
| `name` | 没有这个组 |
| `disabled` | 组存在但校验失败 |
| `nocompile` | 电路还没编译 |
| `range` | 数值越界 |
| `hex` | hex 写得不对 |
| `mode` | 当前模式不允许 |
| `full` | 预约队列满了 |
| `not_output` | 组的方向不对 |
| `no_io` | 该坐标上没有元件 |
| `line` / `encoding` | 行太长 / 不是 UTF-8 |

## 限制

- 单行最多 65536 字节。
- 客户端不读、积压超过 1 MiB 时连接会被关闭。

## 五分钟上手

```text
$ nc 127.0.0.1 25585
HELLO MCHPRS_IO 0.5.2-alpha 0
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

常见流程：先 `POUT` 订阅关心的输出，再 `INPUT` 灌入测试向量，然后等 `OUTPUT` 推送或直接 `READ` 结果。需要逐 tick 检查时用 `/rtps 0` 冻结；需要输入落在精确 tick 上时用 `/tick seq`。

组和存储器怎么声明，见 `file-formats.md`。

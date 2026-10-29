[English](protocol.md) | **中文**

# 自动化协议

脚本用 TCP 连 `127.0.0.1:25585`，纯文本，一行一条报文（`\n` 结尾）。连上后服务器先发一条 `HELLO`，之后每个请求对应一行应答。

这是简版说明，文件格式见 `file-formats.md`。

## 连接

```text
HELLO MCHPRS_IO 0.5.3-alpha 1234      ← 电路已编译，当前 tick 是 1234
HELLO MCHPRS_IO 0.5.3-alpha           ← 还没编译
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

### tick 语义：输入写在哪个 tick 生效、收到的 tick 是哪个时刻

两条报文里的 tick 字段**取自同一个 tick 的两端**，所以两者的差值天生比玩家在游戏里数出来的延迟小 1。

- `OK INPUT <name> <tick>` —— 这个输入**生效**的 tick。写入发生在该 tick 的最开头、红石 tick 计算之前，所以这一 tick 从头到尾都带着新值。发出报文时当前 tick 记为 N：不带偏移 = N+1；带偏移 k = N+k。
- `OUTPUT <name> <tick> <hex>` —— 值被**采样**的 tick，采样发生在该 tick 的末尾：算完这一 tick（并处理完存储器）之后的状态，而不是开头。`OK READ` 同一口径，报告的是最近一个已完成的 tick。

手动测电路时就会碰上这个差 1：

```text
玩家在游戏里数出 3t 延迟
OK INPUT  add_in 1000        ← 生效 tick：从 tick 1000 开头起输入就在场
OUTPUT    add_out 1002 05    ← 在 tick 1002 末尾采样
1002 - 1000 = 2              ← 服务器差值比你数出来的 3t 少 1
```

**玩家观测延迟 = OUTPUT 的 tick − OK INPUT 的 tick + 1**；也就是“相减之后再补上 1”（差值是 2，真实延迟是 3t）。两个数字都没错：输入锚在 tick 起点，输出锚在 tick 终点。不要靠改写法去抵消这个差值，tick 字段永远就是那一 tick 本身，不是“下一 tick”。

收到第一条 `OUTPUT` 也不等于已经稳定：组合电路可能每 tick 推一个中间值，直到稳定（见下面的例子）。

### `INPUT`

一次写入整组；`<hex>` 是把整组位按顺序打包成的一个整数（见下文「位序与 hex」）。

- 不带偏移：下一个 tick 生效。
- 带偏移 `k`：在收到报文时的 tick（记为 N）之后的第 k 个 tick 生效，也就是 N+k。`k = 1` 和不带偏移等价，范围 1–65535。
- 组里有时钟时，同一个 tick 会脉冲一次按钮；没有时钟的组只写数据。

### `POUT` / `OUTPUT`

`POUT 组名` 订阅一个输出组，服务器立刻回一条 `OUTPUT` 表示当前值，没有单独的“成功”应答。

对已经订阅过的组再发一次 `POUT`，服务器会**再推一条当前值**；订阅关系与下面的推送规则都不变。

如果要以**轮询**方式查询 `POUT` 端口（只要取值、不要推送），请给该输出组绑定一个**永不触发的时钟**（`io.txt` 的 `clk` 指向一个没人按的按钮）：带时钟的组只在上升沿推送，这样重复发 `POUT` 就等价于按需读取。

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

`width = 0` 的端口没有数据位，`<hex>` **固定写 `0`**：`INPUT` 写 `0` 表示"按一次按钮"（脉冲一次 `clk`），`OUTPUT` 在 `clk` 上升沿推 `0`。没有 `clk` 的 `width = 0` 组在编译后会被禁用。

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
HELLO MCHPRS_IO 0.5.3-alpha 0
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

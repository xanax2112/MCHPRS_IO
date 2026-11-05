# MCHPRS_IO 项目台账（对照上游的差异清单）

> 本文件记录本仓库相对**上游 MCHPRS** 的全部差异，按文件位置组织，只列"有什么"。
> 上游基线：`~/mchprs/source/MCHPRS`（`master` @ `d492e43`，Minecraft 1.20.4 / 协议 765）。
> 基岩版（Bedrock）不在本文件范围，见 `docs/backlog/BEDROCK.md`。
> 图例：`✚` 新增 · `✎` 修改 · `✖` 移除。

> ⚠ **数据可信度声明（2026-11-04 21:44 用户判定）**
> 本文件在此时间点**之前**记录的所有实测数据与由其推出的结论（含 §9.1 / §9.2 / §9.3 的实测表、§14 里加法器的失败样例、`docs/mchprsio/automation-throughput_zh.md` 全文数字）**一律作废，不得再作依据**。原因不再追究，视同不存在。
> 目前**唯一可信**的实测只有一次：
>
> | 项目 | 值 |
> | --- | --- |
> | 电路 | `mul_in`(32) → `mul_out`(32)，16×16→32 乘法器 |
> | 配置 | `delay = 27`、`interval = 3`、`batch = 16384`、`total = 2^20` |
> | 结果 | **1,048,576 组，通过 1,048,576，失败 0，间隔异常 0** |
> | 吞吐 | 模拟段 139.529 s → **7,515 组/s，22,559 tick/s**（另加收尾巴 3.236 s） |
> | 日志 | `python/log/pipeline_20261104_210432_784.log` |
>
> 其余场景（加法器、其它脚本、其它批大小）都**没有**可信数据，需要重测。

---

## 0. 协作约定

**五条铁律**（用户 2026-10-25 下达，优先级高于一切效率诉求）

1. **目标先行**：写代码前必须完全明确用户目标，不明确就反问，禁止猜测与脑补。
2. **只记录不改码**：功能 / 改进 / 缺陷一律先登记进台账；只有用户明确说"写代码"才动代码。
3. **写码与编译分离**：说"写代码"才写，说"编译"才跑 `cargo`，两步不自动衔接。
4. **全程中文汇报**。
5. 确认语义永远优先于推进速度。

**附加约定**

- **台账按文件位置归属**（用户 2026-11-03 下达，2026-11-05 补充）：`crates/network/src/bedrock_api.rs`、`crates/network/src/bedrock/**`、`vendor/bedrock-rs/**` 记在 `docs/backlog/BEDROCK.md`；**`python/**` 记在 `docs/backlog/PYTHON-BACKLOG.md`（与内核无关，不进本文件）**；其余（`crates/core`、根 `Cargo.toml`、`src/`、`app/`、`docs/`、CI）记在本文件。
- **本文件里的文件格式规则是契约**：`docs/Instruction-manual*.md` 是面向用户的手册，两边内容必须一致；改了格式先改本文件，再同步文档。
- 新条目先判断能否并入现有条目；同一机制就追加，不新开编号。
- 临时文件一律放 `~/dsh/scratch/`。
- 写含单引号的源码必须走 Python 脚本（`chr(39)` 构造）。
- 状态标记：`✅ 已完成` / `📝 已定待写码` / `🚧 进行中` / `📋 待做` / `⛔ 作废` / `⏳ 等用户`。

---

## 1. 环境与产物

| 项 | 值 |
| --- | --- |
| 运行环境 | Android（vivo V2338A / Android 16）上的 Ubuntu 24.04 容器，aarch64 |
| 仓库 | Termux 侧 `~/mchprs/source/MCHPRS_IO`，远程 `https://github.com/xanax2112/MCHPRS_IO.git`，分支 `main` |
| 上游对照 | `~/mchprs/source/MCHPRS`（只读） |
| 线上二进制 | `~/mchprs/mchprs`，0.6.1-alpha |
| 构建 | `cargo build --release --features bedrock`（要基岩版）；`cargo build --release`（纯 Java）。`/tmp` 不可写，日志写 `~/dsh/scratch/`；release 开 `lto = "fat"`，全量约 11–12 分钟 |
| 首次构建 | 需要网络拉 `nbtx`（git 依赖，随基岩版插头走）；纯 Java 构建不需要 |
| 日志 | 环境变量 **`MCHPRS_LOG`**（不是 `RUST_LOG`），默认 INFO |

---

## 2. 仓库结构差异

`✚` **文档布局（2026-11-05，用户定）**：所有说明类文档都在 `docs/` 下，按来源分三个子目录——
`docs/upstream/`（上游自带：`Redpiler.md` / `RIL.md` / `Adding_a_block.md`）、
`docs/mchprsio/`（本仓库新增：协议、文件格式、吞吐；基岩专题已并入 `docs/backlog/BEDROCK.md` §10）、
`docs/backlog/`（三本台账：`BACKLOG.md` / `BEDROCK.md` / `PYTHON-BACKLOG.md`）。
根目录的 `README.md` / `README_zh.md` 与 `python/`、`app/` 下的 `README.md` 属于工程说明，不放进 `docs/`。

`✚` 上游没有、本仓库新增：

| 路径 | 说明 |
| --- | --- |
| `app/` | Android APK 工程（Java，把 `mchprs` 打成独立 App，包名 `com.dsh.mchprs`） |
| `python/` | 协议测试工具（`main.py` + `core/`），台账在 `docs/backlog/PYTHON-BACKLOG.md` |
| `run/` | 服务端运行目录（`Config.toml` / `io.txt` / `logs/` / `world/`） |
| `docs/Instruction-manual.md`、`docs/Instruction-manual-zh.md` | 统一使用手册（中英）：构建运行、io.txt、存储器、自动化协议、测试工具、APK |
| `.github/workflows/mchprs-io-ci.yml` | 本仓库自己的 CI（`test` + `build` 两个 job） |
| `README_zh.md` | 中文 README（上游只有英文） |
| `docs/backlog/BEDROCK.md` | 基岩版台账（本文件不涉及） |

`✖` 上游有、本仓库删除：

| 路径 | 说明 |
| --- | --- |
| `crates/core/src/profile.rs` | Mojang 玩家档案查询（`/whitelist add <name>` 用）；随白名单功能一起删除 |

`✎` 上游有、本仓库改过（按 crate 归类）：`crates/core`（12 个文件）、`crates/redpiler`（10）、`crates/redstone`（2）、`crates/network`（2）、`crates/text`（1）、`crates/rilc`（2）、`tests/`（2）、`Cargo.toml`、`Dockerfile`、`README.md`、`docs/upstream/Redpiler.md`。

---

## 3. 配置项差异（`crates/core/src/config.rs`）

`✚` 新增：

| 键 | 默认值 | 说明 |
| --- | --- | --- |
| `io_port` | `25585` | 自动化桥的 TCP 端口，**只绑 127.0.0.1**；绑定失败只警告、服务端继续跑 |
| `performance` | `false` | `true` = 每 tick 批量采样（性能模式）；`false` = 逐 tick 精确采样 |
| `betterio` | `false` | 打开 `betterio` 时序修正（见 §10） |
| `bedrock` | `19132` | 基岩版 UDP 端口；`0` = 关闭；详见 `docs/backlog/BEDROCK.md` |

`✖` 移除：

| 键 | 原因 |
| --- | --- |
| `whitelist` | 白名单功能整体删除（连同 `/whitelist` 命令与 `profile.rs`） |
| `auto_redpiler` | 自动编译功能删除（连同 `/toggleautorp`），只保留手动 `/redpiler compile` |

`✎` 语义变化：`bind_address` 的 IP 现在也决定基岩版 UDP 绑哪个网卡（见 `docs/backlog/BEDROCK.md` 第 54 条）。

---

## 4. 新增模块（`crates/core/src/`）

| 模块 | 行数 | 作用 | 规则 |
| --- | --- | --- | --- |
| `automation.rs` | 811 | 自动化桥：明文 TCP，让脚本驱动/观测端口 | 报文见 `docs/Instruction-manual.md` |
| `io.rs` | 715 | `io.txt`：数据端口组 | **§6** |
| `memory.rs` | 2102 | 文件式 ROM / RAM | **§7** |
| `plot/coord.rs` | 802 | `/coord`：把选区写进 io / 存储器文件 | 见 §8 |
| `console.rs` | 57 | stdin 控制台：`/` 开头的行当命令执行 | 见 §9 |

---

## 5. 通用文件语法（`io.txt` 与 memory 共用）

### 5.1 编码与注释

- 文件为 UTF-8 文本。
- 注释三种：`// 行注释`、`# 行注释`、`/* 块注释 */`。
- 空行忽略。

### 5.2 键

- 一行一个 `键 = 值`，`=` 两边空格随意。
- 键按名字匹配，**只在所属块内生效**。
- 示例：`origin = 0`、`clk = 12, 64, 12`、`width = 4`、`rs_width = 2`。

### 5.3 坐标列表

列表以 `[` 开始、`]` 结束，每个条目展开成一个或多个坐标。

| 写法 | 例子 | 含义 |
| --- | --- | --- |
| 单点 | `10, 64, 10` | 一个坐标 |
| 步进 | `10, 64, 10, +2x, 4` | 起点、方向、数量 |
| 矩阵 | `matrix: 10, 64, 10, +2x, 3, +4y, 5` | 3×5 方块区域，行优先 |

步进规则：步长 **1–255**，数量 **1–1024**，方向只认 `x` / `y` / `z`，**符号必填**（`+` 或 `-`）。红石元件通常隔一格摆一个，所以坐标步长一般是 2。

### 5.4 `origin`

只接受两种写法：

- `origin = 0` —— 不偏移；
- `origin = x, y, z` —— 绝对偏移，允许负数。

单个非零数字（如 `origin = 5`）报错。`origin` 加到该块的**所有**坐标上，**包括 `clk`**。memory 文件在**文件级**使用同一个键。

`clk` 同样规则：`0`（或相加为 `0, 0, 0` 的三元组）= 无时钟；其它三元组是坐标，也加 `origin`。

### 5.5 `matrix:` 展开规则

```text
matrix: 10, 64, 10, +2x, 3, +4z, 5
```

- 前缀 `matrix:`（单词不分大小写，**冒号必须有**），后面是普通起点 `x, y, z`。
- 起点后必须是 **1–3 段** `方向, 数量`，所以整行按逗号切开后是 5、7 或 9 段。
- **只能写一行**，跨行报错（`a matrix: list must be a single line`）。
- 方向：显式 `+`/`-`，数值 **1–255**，末尾轴字母 `x`/`y`/`z`（大小写不敏感）；数值 0 报错。例：`+2x`、`-3y`、`+16y`。
- 数量：纯数字，**1–1024**。
- 展开是混合进制、**第一段变化最快**：

  ```text
  for i3 in 0..count3:        # 最慢
    for i2 in 0..count2:
      for i1 in 0..count1:    # 最快
        point = origin + step1*i1 + step2*i2 + step3*i3
  ```

- 同一根轴可以出现在多段里，位移叠加。
- 总点数 = `count1 * count2 * count3`，必须等于声明的 `width`（点数）。
- **数电补零**：`count1` 个点构成一行，每行在**前端**补零到整 4 bit（高位在前），`rows = count2 * count3`。
- **matrix 组的 hex 长度与 `width` 不同**：按 `rows * ceil(count1 / 4)` 个十六进制位打包，补出来的位不占位号，`READ <name>:<bit>` 寻址不到。
- 红石屏幕（像素阵列）推荐用 `matrix:`：每行对应一组十六进制位，按扫描线排列。
- memory 的 matrix 端口用同一套展开，数据字先按行补零再打包，读取时去掉。
- `⛔` **模电 `matrix:` 尚未实现**：当前报 `a matrix: list cannot be analog`。

### 5.6 限制与默认值

| 项 | 限制 / 默认 |
| --- | --- |
| 单行步进 | 步长 1–255，数量 1–1024 |
| 矩阵 | 1–3 段，只能单行，数量 1–1024，步长 1–255 |
| 组名 | `[A-Za-z0-9_]+`，全局唯一，区分大小写 |
| 点宽 | 数电 1 bit，模电 4 bit（红石强度 0–15） |
| 地址总线 | 最多 63 位 |
| `width = 0` | 允许（空列表，没有坐标） |
| 存储器目录 | 与 `Config.toml` 同级的 `memory/` |
| 存储器后缀 | `.rom.txt`、`.ram.txt` |

---

## 6. `io.txt` 规则（`crates/core/src/io.rs`）

### 6.1 文件与块

- 单文件 `io.txt`，位于服务端工作目录根；**文件不存在不报错**（只打一行提示）。
- 首行可写版本声明 `v<版本> hex words addressed`，**0.5.0 及以后**接受；低于 0.5.0 禁用整个文件。不写版本行照旧加载。
- 块头：`<角色> <组名> = [`，块以 `]` 结束。

| 角色 | 方向 | 允许的元件 |
| --- | --- | --- |
| `input` | 脚本 → 电路 | 拉杆、压力板 |
| `output` | 电路 → 脚本 | 红石灯、铁活板门、音符盒 |
| `analog_input` | 脚本 → 电路 | 点状红石粉 |
| `analog_output` | 电路 → 脚本 | 点状红石粉，且不得被其它信号驱动 |

### 6.2 必填键

| 键 | 哪类块需要 | 含义 |
| --- | --- | --- |
| `origin` | 所有块 | 加到该块所有坐标上的偏移 |
| `clk` | 所有块 | 时钟元件，**必须是按钮**；`0` = 无时钟；`width = 0` 时必须写真实坐标 |
| `width` | 所有块 | 数电：点数；模电：**总位数** |
| `rs_width` | 仅模电块 | 模电**点数** |

- 数电组：`width` = 展开后的坐标数。
- 模电组：`rs_width` 与 `width` **都必须写**（顺序任意，写在坐标之前）；`rs_width` = 坐标数，`width` = `rs_width × 4`；对不上该组禁用。
- 每个点固定 4 bit；hex 每点一个十六进制位，长度 = 点数。
- 位序：第一个坐标 = 最显著位；一个点内 4 bit 高位在前。
- 数模**不能在同一列表内混写**；同一个 `io.txt` 里可以既有数电组又有模电组。

### 6.3 组名

- `[A-Za-z0-9_]+`，大小写敏感。
- 整个文件内唯一：`input`、`output`、`analog_*` 之间一起查重。

### 6.4 校验（加载时 + 编译成功后各一次）

| 检查 | 时机 | 结果 |
| --- | --- | --- |
| 语法、块未闭合、块头非法 | 加载 | **整个 `io.txt` 禁用** |
| 缺键、`width`/`rs_width` 不符、宽度关系错、组名重复、时钟落在数据点上 | 加载 | 只禁用该块 |
| 坐标上没有元件 | 编译后 | 只禁用该块 |
| 元件类型不符（输入要拉杆/压力板，输出要灯/活板门/音符盒，时钟要按钮） | 编译后 | 只禁用该块 |
| `clk` 与别的 io 组相同 | 加载 | **后出现的块**禁用 |
| `clk` 与 memory 的时钟相同 | 编译后 | **只禁 io 块**，memory 不受影响 |
| `width = 0` 且没有 `clk` | 编译后 | 只禁用该块 |

- 报错信息里的行号一律按**整个文件**计。
- 点数只受"每段 ≤1024 + 坐标范围"限制，**不设旧协议的 16000 上限**。

### 6.5 时钟与运行行为

| 情形 | 行为 |
| --- | --- |
| 有 `clk` 的组收到 `INPUT` | 写入数据，并在**同一 tick** 脉冲按钮一次 |
| 无 `clk` 的组收到 `INPUT` | 只写数据、不脉冲 |
| 无 `clk` 的输出组 | 任一位变化就推 `OUTPUT` |
| 有 `clk` 的输出组 | 只在按钮**上升沿**推一次 |
| 重新编译 | 清空全部 `POUT` 绑定与预约 |

**`width = 0` 的端口**（没有数据位，只有时钟有意义；`clk` 必须是真实坐标，否则按 6.4 禁用）：

| 情形 | 行为 |
| --- | --- |
| `INPUT <组名> 0` | `<hex>` 固定 `0`，含义是"按一次按钮"：只脉冲一次 `clk`，不写数据位 |
| `OUTPUT` / `POUT` | `<hex>` 固定为 `0`，只在 `clk` 上升沿推送（绑定确认帧也是 `0`） |
| 无 `clk` | 编译后整组禁用 |

### 6.6 时钟约束（补充）

- 时钟元件不得与任何端口坐标重合（撞了报错）。
- 时钟点上的元件类型不对，或该点不在编译后的电路里 → 编译后校验失败。
- 模电点只有让编译器保留下来才会出现在图里：**点需要 `-d`（`--wire-dot-out`），十字需要 `-c`（`--wire-cross-out`）**；缺了对应参数，该点会被报成缺元件，整组禁用。
- 红石粉读写：只识别孤点；读不限，写只允许入边为 0 的孤点；写入只能整组 `INPUT`。

---

## 7. 存储器规则（`crates/core/src/memory.rs`）

### 7.1 文件与首行

- 只读：`memory/<名字>.rom.txt`；可写：`memory/<名字>.ram.txt`。
- 首行版本声明 `v<版本> hex words addressed`：**0.5.0 及以后**接受；**只写版本号**（如 `v0.5.3-alpha`）也可加载。
- 版本不符 → 加载时**整文件拒绝**，只写日志说明原因。
- 以点号开头的文件（`memory/.rom.txt`、`.ram.txt`）是首次启动写下的**骨架模板**，加载器跳过。
- 服务器写盘时首行版本号 = **当前内核版本**（随 crate 版本走，0.6.1-alpha 就写 `v0.6.1-alpha`）。

### 7.2 结构

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

00: 0f 3c 69 00
```

- `origin` 与 `clk` 是**文件级键**；`origin` 加到所有端口坐标上，包括 `clk`。
- 每个端口列表**第一项必须是 `width = N`**，展开后正好 N 个点。

### 7.3 时钟 `clk`

| 写法 | 含义 |
| --- | --- |
| `clk = x, y, z` | 真实时钟：读这个方块的电平，只在**上升沿**（上一 tick 低、这一 tick 高）动作 |
| `clk = 0`、`clk = 0, 0, 0` | 无时钟 |
| 不写 `clk` | 等同于无时钟 |

**ROM：时钟可选**

- 有时钟：只有上升沿才把"当前地址对应的字"写到 `data` 总线；两次上升沿之间地址变了也不写。
- 无时钟：**跟随地址变化** —— 采样地址与上一次不同就写出新字。
- 两种情况都**只改电平确实变化的位**。

**RAM：时钟必填**

- 写 `clk = 0` 直接拒绝：`a RAM requires a clock element: write clk = x, y, z`，整个文件不加载。
- 一次上升沿里顺序固定：
  1. 汇编当前 `address`；
  2. `read enable` 为高 → 把**旧字**送到 `read data`（**读优先**：读到的永远是本次写入之前的值）；
  3. `write enable` 为高 → 从 `write data` 取新字，按 `byte write enable` **逐字节合并**后写回；
  4. 没有上升沿 → 既不读也不写。

**时钟元件必须是「输出类」方块**：红石灯 / 铁活板门 / 音符盒（服务器**读**它）。
与 `io.txt` 相反：`io.txt` 的 `clk` 必须是**按钮**（服务器**按**它）。

**时钟的其它约束**

- 时钟不能落在数据点上，也不能与数据点**正交相邻**（写给数据总线的信号会驱动它本该读取的那条线）。
- 两个存储器**不能共用**同一个时钟方块；共用的全部禁用，日志说明原因。
- `io.txt` 的组与存储器共用时钟时，**只禁 io 组**，存储器照常工作。
- 边沿**按 tick 采样**：`run_memories()` 每个 tick 都跑，`performance` 模式也不合并边沿（与 io 组的批量采样不同）。

### 7.4 端口键

| 角色 | 数电键 | 模电键 | 方向 |
| --- | --- | --- | --- |
| 地址 | `address coordinate` | `analog address coordinate` | 输入 |
| 写数据 | `write data coordinate` | `analog write data coordinate` | 输入 |
| 读数据 | `read data coordinate` | `analog read data coordinate` | 输出 |
| ROM 数据 | `data coordinate` | `analog data coordinate` | 输出 |
| 写使能 | `write enable` | — | 输入 |
| 读使能 | `read enable` | — | 输入 |
| 字节写使能 | `byte write enable` | — | 输入 |

- `write enable` / `read enable` 是**单坐标**（`write enable = x, y, z`），**必须写**，不接受 `0`。
- `byte write enable` 是**列表**：写 `byte write enable = [ width = 0 ]` 表示**不使用**；带坐标时条数必须等于 `ceil(数据位宽 / 8)`。
- `byte write enable` **只在 `write enable = 1` 时生效**。
- 模电键名**用空格分隔，不带下划线**。
- 每个角色只能有一个列表；同一角色的数电键与模电键**互斥**。
- 数电/模电**按每条线选**，可混搭。
- 模电列表与 `io.txt` 一样要同时写 `rs_width`（点数）与 `width`（总位数 = 点数 × 4）。
- 地址位宽最大 **63 位**（模电按 `address coordinate` 点数 × 4 计）。
- 输入点只要是点状粉即可，不要求入边为 0；输出点由 memory 驱动，要求无其它输入，允许比较器读。
- 模电点只认红石粉；编译参数 `-d` / `-c` 同样要加。

### 7.5 数据段

```text
00: 0f 3c 69 00 00 00 00 00
1f: 00 0a
```

- `<地址>:` 设定当前地址，后面的字依次写入；**没有冒号的行**表示接着上一行继续写。
- 地址与字都是十六进制（可带 `0x` 前缀）。
- 字太短**前端补零**，太长只保留低位。
- `matrix` 端口的字先按行补零再打包。
- 没有数据的地址读作 0。
- 数据段从第一行"看起来像数据"的行开始（`地址:` 或纯十六进制字）。

### 7.6 RAM 回写

运行期不写盘，只在下列时机落盘：

1. 任意玩家退出服务器；
2. 服务器退出（SIGINT / SIGTERM / SIGHUP；SIGKILL 不算）；
3. `/redpiler reset`；
4. 编译崩溃；
5. `/memory reload` 之前。

回写格式（Logisim 兼容）：

- 每行固定 **8 个 word**，行首地址按 8 对齐；
- 整块没有数据的块整块省略；
- 空格子按数据位宽补零；
- 地址按地址总线宽度补足位数；
- 首行版本号重写为当前内核版本；
- 文件**原子替换**（先写临时文件再 rename）。

### 7.7 校验

| 检查 | 时机 | 结果 |
| --- | --- | --- |
| 首行版本、语法、`width` 与坐标数、模电双键 | 加载 | **整文件禁用** |
| 端口坐标上没有元件、元件类型错 | 编译后 | 整文件禁用 |
| 同一存储器的两位落在同一坐标 | 加载 | 整文件禁用 |
| 数据点与地址 / 时钟 / 使能点相邻 | 加载 | 整文件禁用 |
| 读数据与写数据共用坐标 | 加载 | 整文件禁用 |

- 相邻判冲突的原因：写给数据总线的信号会直接驱动它本该读取的那条线。
- ROM 检查数据点对地址与时钟；RAM 检查读数据点对写数据、使能、时钟与地址。
- 编译后校验失败时，RAM 内存里**未落盘的数据直接丢弃**；被禁用的存储器永不回写。

---

## 8. 命令集差异（`crates/core/src/plot/commands.rs`）

`✚` 本仓库新增：

| 命令 | 子命令 | 说明 |
| --- | --- | --- |
| `/coord` | `in` / `new` / `add` / `undo` / `redo` | `in` 选择目标（`io.txt <组>` 或 `xx.rom`/`xx.ram`），`new`/`add` 把选区写入，8 步 undo/redo |
| `/io` | `list` / `reload` | 查看或热重载 `io.txt`（`-i` 编译运行中不可用） |
| `/memory` | `reload` | 热重载 `memory/` 下的存储器定义 |
| `/tick` | 无参 / `seq` / `freeze` / `unfreeze` / `step <偶数>` | 看当前 tick、进入预约模式、冻结/恢复 rtps、单步推进 |

`✖` 本仓库删除：

| 命令 | 说明 |
| --- | --- |
| `/whitelist` | 连同 `profile.rs`、`whitelist` 配置一起删 |
| `/toggleautorp` | 自动编译已删，只保留 `/redpiler compile` |

`✎` 行为变化：

- `/stop`：支持从控制台执行；停机时先落盘。
- `/rtps`：设成自动速度（正数 / `unlimited`）会退出预约模式；`/rtps 0` 只改速度，既不踢脚本也不授权预约。
- `/gamemode`：命令树新增 `GameMode` 参数类型（`crates/network/src/packets/clientbound.rs`），补全可用。
- `/radv`、`/compile`、`/reset`、`/sel`、`/plot`、`/visit`、`/info` 等其余命令保持上游行为。

---

## 9. 运行模型与自动化

- `✚` **tick 计数**：`tick: u32` 记录编译完成后的 tick 数，供自动化 `HELLO` / `INPUT` / `READ` 使用（`advance_ticks` / `step_tick` / `current_tick`）。
- `✚` **预约模式** `/tick seq`：单一脚本独占，按绝对 tick 预约 `INPUT`（最多 255 条，`apply_reservations`）；与 `performance` 模式互斥。
- `✚` `rtps` 冻结/恢复：`freeze_rtps` / `unfreeze_rtps`，配合 `/tick step` 做单步调试。
- `✚` **崩溃保存**：编译 panic 时 `crash_save` 落盘并退出（`panic_payload_message`）。
- `✚` **控制台线程**：`server.rs` 启动时 `console::spawn`，stdin 与游戏内命令共用一套逻辑；需要玩家目标的命令会被拒绝并红字提示。
- `✚` **自动化桥**（`automation.rs`）：纯文本 TCP，端口 `io_port`（默认 25585，只绑 `127.0.0.1`）；报文 `HELLO` / `INPUT` / `POUT` / `OUTPUT` / `READ` / `TICK` / `NOW` / `RADV`；版本串 `MCHPRS_IO 0.5.2-alpha`；规格见 `docs/Instruction-manual.md`。
- `✚` `Message` 扩展：`IoResponse`、`IoDataPortOutput`、`SeqModeChanged`、`ConsoleCommand`、`AutomationRequest`、`AutomationDisconnect`。

### 9.1 自动化协议下沉到 plot 线程（✅ 已完成，2026-11-04）

目标：消灭「主循环窗口 + 两跳 mpsc」的往返延迟，并让全速批测的输出能边跑边推。

**两种模式（行为不变）**

- **时序精确模式**：`/tick seq` 进入，条件不动——`rtps 0`、单脚本独占、`io_mode = exact`。`INPUT` 按 tick 单调递增预约，再发 `RADV N` 全速跑。
- **普通模式**：不接受预约，带 `offset` 的 `INPUT` 仍返回 `ERR mode`；不带 `offset` 的 `INPUT` 下一 tick 生效。

**改动**

- `✚` `AutomationServer` 由 `server.rs` 绑定 `io_port` 后整体移交 p0,0 的 plot 线程；accept、读 socket、解析、执行、编码、写回全在该线程完成，主线程不再引用 automation。
- `✎` 主循环 `sleep(2ms)` → **`sleep(5ms)`**。
- `✚` **flush 策略**（由 plot 自己控制，不依赖主循环）：
  - `RADV` 同步跑批期间：每 **5 ms** 冲一次输出。
  - 其余时刻：不等，产生一条就写一条。
  - `io_mode = performance`：每 **5 ms** 采样一次并推送（不再按批末尾采样；`tick seq` 与普通模式一致）。
- `✚` **独占权自动续接（用户 2026-11-04 定，已写码）**：`/tick seq` 只需执行**一次**。先连上的脚本取得独占；脚本退出后服务器**保持武装**，下一个连上的脚本自动成为独占，不需要重新输入 `/tick seq`。
  - 现状（要改的）：`AutomationServer::drop_connection` 在独占脚本断开时把 `waiting` 置回 `false`，注释写的是"玩家必须重新 `/tick seq`"——这导致每跑一次脚本都要重敲一次命令，非常容易忘，是 `pipeline.py` 反复报"当前不是 /tick seq 模式"的直接原因。
  - 改法：断开时保留 `waiting = true`（`exclusive = None`），`accept_new` 已有"武装状态下第一个连接成为独占"的逻辑，无需新增。
  - `docs/Instruction-manual.md` 需同步：`/tick seq` 的执行次数从"每次会话一次"改为"进入该模式一次，直到 `/rtps > 0`、重新编译或关服才解除"。
- `✚` **poll 周期 200 µs**：plot 在 tick 循环里按墙钟时间片非阻塞读 socket，保证普通模式往返延迟不回退。
- `✚` **调度开销收敛**：用计数器驱动，每 64 个 tick 才取一次 `Instant::now()`——全速下每 tick 取表约 20~25 ns，270 万 tick/s 会吃掉约 5% CPU；无 automation 连接时直接跳过 poll（否则每秒 5000 次空问）。收敛后整套调度（时间检查 + poll）在 0.1% 以下。
- `✎` **反压**：单连接 `MAX_PENDING` 1 MB → **16 MB**，另设全局 **32 MB** 上限，超限断连；`/tick seq` 只接受一个脚本。
- `✎` **预约错误**：坏条目跳过并继续处理后面的；错误码带信息（`ERR order <tick>`、`ERR hex <tick>`）；单批最多回 10 条详细错误 + 一条汇总 `ERR skipped <n>`，避免错误风暴灌爆 socket。
- `✖` 删除 `Message::IoResponse`、`Message::IoDataPortOutput`、`AutomationAction::Route`、`AutomationAction::Disconnect`、`PrivMessage::AutomationRequest`、`PrivMessage::AutomationDisconnect`。
- `✎` `/tick seq` 改为主线程发 `PrivMessage` 通知 plot，不再直接调用 `automation.set_seq_mode`。
- `✎` `READ x,y,z` 收窄：只认 p0,0 范围，范围外 `ERR nocompile`。
- `✎` `RADV` 保持**同步**（跑完才回 `OK RADV`），脚本须边读 `OUTPUT` 边等 `OK`；输出保证先于 `OK` 到达（同一条 mpsc，顺序不变）。

**不做**：输出合并与抽样（电路特性变化太快，由脚本少发预约解决）。

**实施注意（改码时必须处理）**

- `✚` 节点 id 缓存在 `clear_io_state()`（`mod.rs:1813`）里一并失效；`poll` / `write` 发现缓存为空就重新解析，解析失败标 `active = false`，不得拿旧 id 读。
- `✚` 预约队头 `tick < now` 时丢弃并回 `ERR stale <tick>`，否则后面的条目会被永久堵住。
- `✚` `pending` 建连时预留容量（约 256 KB），增长按 1 MB 步进，避免 16 MB 反复翻倍拷贝。
- `✚` `io_port` 仍在主线程 bind，失败则禁用 automation 且不 panic；成功才移交 plot。
- `✚` 重新编译后连接保留但绑定被清（`clear_io_state`），需主动推送 `ERR reset` 让脚本重新 `POUT`。
- `✚` `READ x,y,z` 收窄属于行为变更，写进 `docs/Instruction-manual.md`。
- `✚` 主循环 5 ms 把玩家包刷新降到 200 Hz（有意取舍）。
- `✚` 清理 `server.rs` 中 `IoResponse` / `IoDataPortOutput` / `SeqModeChanged` 与 `AutomationAction::Route` / `Disconnect` 的残留分支。

预期：往返由约 12.5 ms 降到「1 tick + loopback」；655360 tick 的批测输出按 5 ms 分批发，脚本持续读取即可，不会撞断连。

⛔ **以下数据已作废（2026-11-04，见文首声明），只留作历史记录**

**实测（2026-11-03，加法器 17→9 位）**

| 场景 | 结果 |
| --- | --- |
| 全组合 131072 组（预约 + RADV，间隔 2t） | **1.37 s，0 错误，95,719 组/s** |
| 批量 2000 组（同上） | RADV 0.062 s；整脚本 0.8 s |
| 串行一问一答 | 4,527 组/s |
| 采样开销（满负荷，绑/不绑 POUT 对比） | 噪声内，测不出 |
| `correctness.py` / `timing.py` / `pipeline.py` | 500/500 · 10/10 · 2000/2000 |

`pipeline.py` 原来逐条等 `OK` 再发下一条（N 次往返），已改为一次发完再收 `OK`；`full_sweep.py` 用同一做法扫全组合。

### 9.2 IO 采样与写入：去掉每 tick 的分配与哈希查找（✅ 已完成，2026-11-04）

⛔ **以下数据已作废（2026-11-04，见文首声明），只留作历史记录**

实测基线（64 位、无时钟、绑了 `POUT` 的输出组，每 tick）：

| 动作 | 成本 |
| --- | --- |
| `Vec::with_capacity` 堆分配 | 40~80 ns |
| 64 次 `query_io` → `pos_map` 的 `FxHashMap` 查找 | 500~750 ns |
| `prev_bits != bits`（memcmp 64 字节） | 5~10 ns |
| `bits.clone()` 分配 + 拷贝 | 50~100 ns |
| 合计 | 0.65~1.0 µs |

- `✚` **缓冲复用**：`IoGroup` 增 `scratch: Vec<bool>`，新增 `read_into(&mut scratch)`；`poll` 用 `prev_bits != scratch` 判定，命中后 `std::mem::swap(&mut prev_bits, &mut scratch)`。每 tick 零分配、零拷贝。约省 15%。
- `✚` **节点索引缓存**：绑定时把每个坐标的 `NodeId` 解析一次存进 `IoGroup`，之后 `poll` / `write` 直接下标取值，不再每个点查一次 `pos_map`。省 60~70%，是三条里的大头。
  - 需要后端新增按 `NodeId` 取 `powered` / `output_power` 与按 `NodeId` 写入的接口；模拟量的 `query_wire_power` 同样要覆盖。
  - 写入路径（`IoGroup::write` 的 `try_set_powered` / `try_set_wire_power`）有同样的每点哈希查找，一并改成用缓存 id。
- `✖` **只在变化时克隆**：被缓冲复用的 `swap` 覆盖，不单独实现（单做能省 50~100 ns，但做了缓冲复用就白做）。
- `✎` **采样节奏**：`performance` 模式原本在每批末尾调一次 `sample_io_groups`，改为按墙钟**每 5 ms** 一次（与 flush 同拍，`RADV` 期间也照此）；`exact` 模式保持每 tick 采样。三件事共用一个时间片：poll 200 µs、采样 5 ms（仅 `performance`）、flush 5 ms。
- `✚` **`run_memories` 同源优化**（`mod.rs:1500`）：`MemoryState::sample` 每 tick 走 `query_io` / `query_wire_power` / `try_set_*`，与 `IoGroup` 是同一套按坐标查哈希的写法。同样缓存 `NodeId` 并复用缓冲，避免每 tick 每点一次哈希查找（16 位地址的 ROM 每 tick 约 30~60 次查询）。

收益：每 tick 由 0.65~1.0 µs 降到 0.15~0.25 µs，约 -75%。

适用性：`rtps 1000` 时采样只占约 0.1% CPU，做不做无差别；跑全速（`unlimited`）时采样成本是红石模拟本身的 2~3 倍，必须做。用户 2026-11-04 决定三条都做。

风险：

- **节点 id 只在一次编译内有效**。重新编译、`/redpiler reset`、`crash_save` 重载之后必须重新解析；坐标已不存在时要把该组标成 `active = false`，绝不能拿旧 id 去读，否则是静默错数据。
- `poll` 里 `swap` 之后 `scratch` 持有的是旧的 `prev_bits`，后续不得再当本 tick 的值使用。
- 输出若每 tick 都变，省下的时间会被 `encode_hex` + `Message` 分配（0.5~1 µs）吃回一半，收益只在“输出很少变”时体现。

### 9.3 预约表：单调化 + 上限 65536（🚧 进行中：批次回复 2026-11-04 已写码，待编译实测）

- `✎` **上限**：`MAX_RESERVATIONS` 由 255 提到 **65536**（用户 2026-11-04 定）。每条约 170 字节（64 位组：结构体 56 + 组名堆约 32 + `bits` 堆约 80），65536 条约 **11 MB**。30 万条以上才需要驻留组名 + 位打包（可降到约 24 字节/条），本次不做。
- `✚` **单调约束**：预约必须按 tick 单调不减发送，违反返回新错误码 `ERR order`。理由：单调后表天然有序，出队是游标 O(1)，查重只需从尾部扫当前 tick 的那几条——替掉现在每 tick 全表 `retain` 加每条 `clone` 的写法（655360 tick × 65536 条约 2×10^10 次遍历）。
- `✎` **结构**：`reservations: Vec<Reservation>` 改为 `VecDeque<Reservation>` 加 `last_tick: u32`。
  - `reserve_input`：`tick < last_tick` 拒绝；`tick == last_tick` 从尾部往前扫同 tick 条目，同组覆盖（保住“后到覆盖”语义）；否则 `push_back`。
  - `apply_reservations`：`while front.tick == now { pop_front(); group.write(...) }`。
  - `VecDeque` 出队不自动缩容，需在清空或长期低水位时 `shrink_to_fit`，否则峰值决定常驻内存。

插入摊还 O(1)，应用 O(k)（k 为该 tick 的条目数）。配合流式灌入时表里始终保持一个批次（65536 条 / 11 MB）。

- `📋` **批量预约的 `OK` 取舍（待确认）**：实测 65536 条预约，`OK INPUT` 回复本身要 **0.155 s**（43 万条/秒），而 RADV 13 万 tick 只要 0.262 s——**占整轮时间的 37%**，因为 65536 条 `OK INPUT <name> <tick>` 约 1.7 MB，客户端必须逐行解析。
  - 建议：新增静默动词 `INPUTQ <name> <offset> <hex>`，语义同 `INPUT`，成功不回、出错仍回 `ERR`。服务器按行顺序处理，随后的 `OK RADV` 天然是同步点；绝对 tick 由脚本用 `HELLO` 基准 + offset 自算（`rtps 0` 下确定）。
  - 收益：65536 条省 1.7 MB 与 0.155 s；老脚本用 `INPUT` 不受影响。
  - 已否决的备选：把回复缩短成 `OK <tick>`（省字节不省解析次数，且破坏兼容）；批量帧 `LOAD <n>` + n 行 + 一次 `OK`（收益相同但协议改动大）。
- `✚` **预约批次按 5 ms 静默判定（用户 2026-11-04 定，已写码，待编译实测）**：预约模式下服务器**不再逐条回 `OK INPUT`**。
  - 收到 `INPUT <name> <offset> <hex>` 只入队并计数，不回任何东西；
  - **连续 5 ms 没再收到新的预约**，视为这一批结束，回**一条** `OK INPUT <name> <条数>`（条数 = 本批被接受的预约数）；
  - 条数回显用于核对：脚本发出的行数应等于所有 `OK INPUT` 的条数之和；对不上说明有丢失，或发送中途停顿被切成了多批；
  - 出错仍然**立即**回 `ERR <code>`，不受 5 ms 影响；
  - 收益：65536 条从 **1.7 MB / 0.155 s** 降到 **1 条回复**，基本归零；
  - 代价：每批固定多 ~5 ms 延迟；下一条预约晚于 5 ms 到达会被算作下一批（多一条 OK），脚本要能接受任意次数的 `OK INPUT`；
  - 实现要点：plot 侧记 `pending_inputs: u32` 与 `last_input_at: Instant`，在已有的 5 ms 调度片里检查并回 OK；任何**非 INPUT** 请求（`RADV` / `TICK NOW` / `READ` / `POUT`）到达时先结算未回的批次，保证回复顺序；
  - 脚本流程：**灌完 → 等 `OK INPUT <name> <n>`（≤5 ms）→ 核对条数 → 再发 `RADV`**。用户 2026-11-04 明确：先校验条数再推进，不能把 `RADV` 和预约一起发出去；
  - 因此原先设想的“`RADV` 提前发”不再需要：那是为了掩盖逐条读 65536 个 `OK` 的等待，改成一条批次回复后没有可重叠的东西了；
  - 普通模式（`rtps > 0`）**不变**，仍逐条回 `OK INPUT <name> <tick>`；`docs/Instruction-manual.md` 需写清“批次”概念与两种模式的回复差异。
  - `✚` **冻结时钟的响应性（写码时发现，一并改）**：`rtps 0` 时 `sleep_time_for_tps` 返回 50 ms，主循环每轮睡 50 ms，`TICK NOW` 和预约批的 5 ms 窗口全被这 50 ms 包住——实测“65536 条 `OK` 要 0.155 s”主要是这个，不是客户端解析慢。改法：有 automation 连接时把睡眠截到 **1 ms**（不推进 tick，只多花唤醒）。
  - `✚` **独占脚本断开时清预约表（写码时发现，一并改）**：自动续接后新脚本从 `HELLO` 给的 tick 重新计数，旧预约若留着会在新会话里凭空触发，所以断开时 `reservations.clear()` + `last_reservation_tick = 0`。
  - `✚` **断开后立刻重连会被拒（实测抓到，已修）**：日志里出现 `Refusing automation client …: a /tick seq exclusive session is active`——上一脚本的 EOF 要等下一次 poll 才被发现，这段时间里独占名额还占着，新脚本一连就被踢。改法：`accept_new` 收到新连接时先 `peek` 独占连接，对端已关就先 `drop_connection` 再接纳。背靠背两次连接已验证通过。

⛔ **以下数据已作废（2026-11-04，见文首声明），只留作历史记录**

**实测（2026-11-04 18:4x，新二进制 8,680,024 B，安卓本机）**

| 场景 | 结果 | 改前 |
| --- | --- | --- |
| `full_sweep` 131072 组（2 批） | **1.00 s，0 错误，130,991 组/s** | 1.37 s / 95,719 组/s |
| `pipeline` 20000 组单批 | 20000/20000，间隔异常 0 | — |
| `pipeline` 65536 组单批 | 65536/65536，0 错误，间隔异常 0 | — |
| 灌 65536 条预约到确认 | **0.092 s**（sendall 0.003 + 等 0.089） | 0.155 s |
| 协议探测（原始 socket） | 3 条一次发 = 一条 `OK INPUT add_in 3`；单条再发 = 新批 `… 1`；中途 `TICK NOW` 先结算再回 `NOW`；`OUTPUT` 全部先于 `OK RADV` | — |
| 纯模拟（无 POUT，含预约应用） | 351,393 tick/s（131080 tick） | — |
| 带 POUT 采样 | 288,473 tick/s，65536 行推送 | — |
| `correctness.py`（rtps 5000） | 497/500，失败 3 组 | 500/500 |
| `timing.py`（rtps 5000） | 10/10 | 10/10 |

注意：`correctness.py` / `timing.py` 必须先把服务器 `/rtps 5000`（脚本自带 `rtps: 5000` 检查但改不了），
在默认 20 tps 下 5 ms 的等待连一个 tick 都不够，会整片报错（实测 1/500），不是回归。
：自动续接后新脚本从 `HELLO` 给的 tick 重新计数，旧预约若留着会在新会话里凭空触发，所以断开时 `reservations.clear()` + `last_reservation_tick = 0`。
- `✚` 组名在加载 `io.txt` 时驻留成 `u16` id；`RequestKind` / `Reservation` / `pout_bindings` 只存 id。
- `✚` `decode_hex` 直接写进复用的 `Vec<bool>`（`decode_hex_into`），不再返回新 Vec。
- `✚` `encode_hex` 写进复用的 `String`；`format!` 改成手写 `push_str`；输出行不 clone `hex`。

**暂缓**：Python 侧逐行解析不优化（用户 2026-11-04 决定）。若它成为硬上限，改用 Go 客户端替代，不改服务端协议。

---

## 10. 编译器与红石（`crates/redpiler`、`crates/redstone`）

- `✚` `CompileError { Override(OverrideError), Cancelled }`：编译失败改为返回 `Result`，不再 panic。
- `✚` `comparator::try_get_override` / `OverrideError`：比较器读到坏容器或越界时把**具体坐标和方块**报给调用方。
- `✎` 比较器：`✚` 支持 `WaterCauldron`；空的容器方块实体不再视为错误，改由 plot 侧统一警告（`format_empty_container_warning`），并按 0 参与编译。
- `✚` `betterio` 开关（`mchprs_redstone::set_betterio`）：未编译模拟器与编译后端共用同一标志。
- `✚` 自动化查询接口：`query_io`、`has_inputs`、`query_wire_power`、`take_io_events`、`try_set_powered`、`try_set_wire_power`、`try_use_block`。
- `✎` `TaskMonitor`：编译可取消（`Cancelled`），取消时不留半成品后端。
- `✎` `rilc` / `redpiler` 若干测试与 pass 细节调整（`constant_fold`、`coalesce`、`identify_nodes`）。

---

## 11. 网络层（`crates/network`）

除基岩版外（见 `docs/backlog/BEDROCK.md`），与上游的差异只有三处：

- `✚` **插座 / 插头**（2026-11-03）：基岩协议栈拆成接口层 `crates/network/src/bedrock_api.rs`
  （12 个数据类型 + `BedrockStack` / `ChunkEncoder` 两个 trait + 全局注册，常驻编译、只依赖 std）
  与实现层 `crates/network/src/bedrock/**`（`--features bedrock` 才编译，整个文件夹可删）。
  core 侧只通过插座拿编码器/换算接口，不再引用实现模块。完整记录见 `docs/backlog/BEDROCK.md` §9.41。
- `✚` `clientbound.rs`：命令树参数类型新增 `GameMode`（`minecraft:gamemode`）。
- `✚` `crates/text`：`ColorCode::code_char()` + `to_legacy_text()`，把文本组件降级成 `§` 颜色码。

---

## 12. 测试与 CI

- `✎` `tests/common/mod.rs`：测试执行器简化为"红石 vs redpiler"两种后端（后端变体参数合并）。
- `✎` `tests/components.rs`：删除 `pending_repeater_pulses_without_input`（上游该用例与当前红石时序实现不符）。
- `✚` `.github/workflows/mchprs-io-ci.yml`：`test` 与 `build` 两个 job；`build` 同时编"带基岩版"和"纯 Java"两种配置。
- `✎` 上游自带的 `build.yml` / `ci.yml` / `docker.yml` 仍在仓库里，监听 `master` 与 `v*`。

---

## 13. 文档与工具

- `✚` `docs/Instruction-manual.md` / `Instruction-manual-zh.md`（2026-11-05）：统一使用手册，
  取代原来的 `protocol.md` / `protocol_zh.md` 与 `file-formats.md` / `file-formats_zh.md`；
  内容覆盖构建运行、Config、游戏内命令、`io.txt`、存储器文件、自动化协议（含批量预约回复与空闲超时）、
  Python 测试工具、APK 与常见问题。
- `✚` README（中英）"快速开始"：依赖、构建运行、进服编译、io.txt、协议、python 工具、故障排查。
- `✚` `python/`：协议测试工具（正确性、时序、流水线），自带台账。
- `✚` **`python/` 目录结构（2026-11-05 合并）**：入口只有 `main.py`，用 `mode` 选 `correctness` / `pipeline` 两种模式；它只放配置区 + `test()`，逻辑全在 `core/` 里（详见 `docs/backlog/PYTHON-BACKLOG.md` 的 PY-011/PY-016）：
  - `core/__init__.py`：包入口，只 re-export `run_correctness` / `run_timing` / `run_pipeline`；
  - `core/mchprs_io.py`：协议客户端，唯一跟服务器说话的地方（`Client`：HELLO / POUT / INPUT / READ / TICK NOW / RADV，hex 编解码，以及批次预约的 `reserve_batch` / `expect_batch_ok`）；
  - `core/runner.py`：三个测试的执行主体（日志 Tee、进度、失败明细、向量生成、判定逻辑）；
  - `core/svg.py` + `core/wavedrom/`：把记录渲染成 WaveDrom 时序图，`svg` 配置为 `True` / `"error"` 时才用；`wavedrom` 是第三方库的本地副本，不装 pip 也能出图。
  - `✖` **已删除 `python/full_sweep.py` 与 `python/bench_batch.py`（用户 2026-11-04 定：没用）**：两个临时批测脚本（全组合扫描 / 单批基准）功能与 `pipeline.py` 重叠，只是省了日志和图；测速需求统一走 `pipeline.py`，不再维护这两份。`build.sh` 里对应的拷入也一并去掉。
  - `✖` **已删除工作目录里的 `python/runner.py`**：旧布局残留（11-04 00:05，19,422 B），没有任何文件 import 它，里面还是旧逻辑（没有 `reserve_batch`、`collect_seconds` 仍是 0.5），留着只会让人改错文件。仓库里本来就没有这份。
- `✚` `app/`：APK 工程（`build.sh` 一键出包，可内置文档与 python 工具）。
- `✚` **版本未稳定期间，打包 APK 不带任何附带文件（用户 2026-11-04 定，已落实到 `build.sh`）**：
  - APK 里只放 `classes.dex` + `lib/arm64-v8a/libmchprs.so`，**不打包 `docs/`、不打包 `python/`**。
  - `build.sh` 新增 `WITH_ASSETS` 开关，**默认 `0`（不带）**；带的时候才 `cp ../docs/…`、`cp ../python/…` 并把 `-A "$ASS"` 传给 `aapt2 link`（空数组则整段展开为空，不影响命令行）。
  - 实测：默认打包 9 个条目（3,654,503 B），`unzip -l mchprs.apk | grep -c assets/` = 0；带 assets 时 35 个条目、3,734,307 B。
  - 原因：`Assets.java` 是**只补缺、不覆盖**（为了让用户的改动不被冲掉），所以一旦工作目录里已经有旧脚本，
    App 启动根本不会更新它们。双方都以为在测新代码，实际跑的是旧文件——2026-11-04 就是这样踩的：
    用户跑 `pipeline.py` 拿到的仍是旧的 `collect_seconds = 0.5` 与旧 `runner.py`，吞吐被 2 s 固定等待拖成 4.6 万组/秒。
  - 附带文件一律由仓库/手动同步，装 APK 只为了换二进制，避免「装的包和跑的脚本不是同一版」。
  - 恢复条件：版本稳定、脚本接口冻结之后，再把文档与 python 打进包；恢复时应当给脚本加版本号或校验（输出里打印版本、启动时比对），
    否则同样的坑还会再踩一次。
- `✚` **`pipeline.py` 计时口径修正（2026-11-04）**：原来每批把 `collect_seconds`（默认 0.5 s）也算进「模拟段」，
  131072 组切 4 批就是 2.0 s 纯等待，吞吐被算成 4.6 万组/s（真实约 15 万）。现在 `runner.py` 把 RADV 与收尾巴分开计时，
  吞吐只按 RADV 算，尾巴单独显示；`collect_seconds` 默认降到 **0.02**（输出保证先于 `OK RADV` 到达）。
- `✚` `docs/mchprsio/automation-throughput_zh.md`：批测速度从 80 组/秒 到 25 万组/秒 的完整推导（调度往返 → 批量预约 → 每 tick 采样 → 回复量，四层瓶颈逐层拆解）。

---

## 14. 未结项（待办清单）

- [ ] **BUG-037：batch 提到 32768 就报 `RADV 失败：`（2026-11-04 用户实测）**
  - 现象：`batch = 16384` 正常（1,048,576 组零错）；改成 `32768` 后，第一批发出去就停在 `RADV 失败：`，错误串是**空的**：
    ```
    已连接：协议 0.6.1-alpha，当前 tick 0
    RADV 失败：
    ```
  - 日志：`python/log/pipeline_20261104_214756_400.log`；配置 `delay = 27`、`interval = 3`、`timeout = 3.0`。
  - **根因（算得出来，不用猜）**：一批要跑 `batch × interval + delay + 2` 个 tick：
    - `16384 × 3 + 27 + 2 = 49,181` tick，按 22,559 tick/s ≈ **2.18 s** < 3.0 s → 过；
    - `32768 × 3 + 27 + 2 = 98,333` tick，≈ **4.36 s** > 3.0 s → 客户端 `radv_collect` 到点抛 `TimeoutError`。
  - 也就是说**不是服务端算错，是客户端超时没有跟着 batch 放大**：`Client.radv_collect` 用的是**一次性绝对 deadline**（`deadline = now + timeout`），
    中途持续收到 `OUTPUT` 也不会续期，所以只要一批的总时长超过 `timeout` 就必失败；batch 越大越容易踩。
  - 建议修法（待写码）：
    1. `radv_collect` / `collect_outputs` 改成**静默超时**——每收到一行就把 deadline 重置成 `now + timeout`，只有真正卡住（连续 timeout 秒无数据）才报错；
    2. 顺手把超时信息写清楚：带上 `batch / span / 已收行数 / 期望 tick 区间`，不要再只打印一个空错误串；
    3. 可选：`pipeline` 起跑前按 `span / 实测 tick/s` 估一个下限，`timeout = max(timeout, 估算 × 1.5)`。
  - 临时绕过：`timeout` 手动调到 `span / 10000 + 5`（32768 批约 15 s；65536 批约 25 s）。
  - **2026-11-04 22:05 翻日志复核（机制确认，不是估算）**：
    - `pipeline_20261104_214756_400.log` 就是这条：`当前 tick 0` 之后直接 `RADV 失败：`，空错误串，配置 `timeout = 3.0`。
    - 用户把运行目录 `/storage/emulated/0/termux/MCHPRS_IO/python/pipeline.py` 的 `timeout` 手改成 `5` 后，32768 批**不再超时**：
      `pipeline_20261104_215024_712.log` 第一批 RADV **4.062 s** 正常返回。→ 3.0 s 卡死、5 s 通过，`timeout` 是唯一变量。
    - 边界重算：98,333 tick / 4.062 s ≈ **24,200 tick/s**；`timeout = 5` 最多撑 `(5 × 24200 − 29) / 3 ≈ 40,300` 组/批。
      65536 批 span = 196,661 tick ≈ **8.1 s**，`timeout = 5` 照样超时。所以"改大 timeout"只能应急，正式修法仍是静默超时。

⛔ **本条证据已作废（2026-11-04，见文首声明）；结论保留仅为历史线索**

- [ ] **输出偶发错误（用户判断：更深的 bug，来自上游）**：`add_in → add_out` 加法器（延迟 4t），**同一输入有时对有时错**，而输出的 tick 间隔始终正确（间隔异常 = 0）。
  - 2026-11-03 `interval = 3`：2000 组固定失败 5 组，`0x1f197` 期望 `0x189` 实得 `0x1c9`、`0x10921` 期望 `0x02b` 实得 `0x06b`——都是 bit6 多亮。
  - 2026-11-04 `interval = 2`：一轮 `20000` 组失败 3 组（`0x02974` 期望 `0x09d` 实得 `0x0bf`、`0x1aeaa`、`0x10921`），紧接着同配置 `40000` 组**零失败**；此前 `131072` 全组合也曾零失败。
  - 反复出现的输入：`0x02974`、`0x1aeaa`、`0x10921`、`0x1f197`。
  - **2026-11-04 新观察**：`correctness.py` 与 `pipeline.py` 都用 `coverage=random, seed=0`，所以开头三个向量固定是 `0x18a7d`、`0x1aeaa`、`0x02974`；这两次跑出来的失败组正好就是这三条（`correctness` 497/500、`pipeline` 19997/20000，失败组号都是 1/2/3）。但同一批向量在另一次跑里全过（`pipeline` 65536/65536、`full_sweep` 131072 全零错），所以"失败组号总在前三"只是**固定种子**的假象，不是冷启动——真正稳定的只有**失败的值**。这就把范围收窄成：同一输入、同一编译，结果**随跑次变化**，且波形上表现为某一位多亮。
  - **判断**：不是测试脚本的问题，是更深一层的 bug，根源在**上游**。
  - **关键线索**：`CompilerOptions.update`（原义：*Update all blocks in the input region after reset*）本来就是为这类情况存在的——编译完成后输入区域的方块状态可能没有全部刷新。**建议 `/redpiler compile` 默认自动带上 `update` 参数**；"需要这个参数才能稳定"本身就证明了问题在上游的编译/初始化路径，而不在采样。
  - 待查：用未编译后端（纯红石模拟）跑同一组向量对照，分辨是编译图初始状态、采样时机，还是 `INPUT` 应用顺序。
- [ ] **BUG-038：冷启动（tick 0）后第一批前 32 组出错 —— 用户定性为上游 bug，已搁置（2026-11-04）**
  - **用户 2026-11-04 22:10 定性**：不是采样采早了，就是**上游带来的 bug**，同类证据已经够多，先搁置，不再往下查。
    台账只留现象与证据，避免以后又从头推一遍。
  - 证据：`pipeline_20261104_215024_712.log`（服务器 21:50:15 刚启动，`当前 tick 0`，`batch = 32768`、`timeout = 5`）
    第 1/32 批：**前 32 组全错，第 33 组起全对**（`累计通过 32736 失败 32`），`RADV 4.062 s`、8067 组/s。
  - 错值形态（只记事实）：**低 16 位逐渐对上、高 16 位始终不对**（第 15 组起低半字完全一致，如 `期望 0x15ab7254 / 实得 0x186d7254`），
    且前几组的错值几乎不变（`0x59455434`、`0x5945543a`、`0x59455438`…）。
  - 我的推测（已作废）：曾以为是"传播中途采样"；用户指出不是采样问题，是上游 bug，此推测撤回。
  - 反证：随后同配置重跑（`pipeline_20261104_215035_836.log`，`当前 tick 196666`，同一批向量）**32768/32768 全过**；
    此前唯一可信的 1M 组跑（`pipeline_20261104_210432_784.log`，`当前 tick 729324`）也是零错。
    → 失败与向量、与电路都无关，**只和"服务器重启后第一批从 tick 0 起跑"有关**（暖机后消失）。
  - 环境事实：加载的 `world/plots.db` 是 20:59 保存的旧编译结果，21:47、21:50 两次启动都**没有**重新 `redpiler compile`。
  - **状态：搁置（用户 2026-11-04）**。以上证据保留备查；重新捡起来时先看 §14 旧条目和 `--update` 那条线索。
  - **2026-11-05 用户给出规避办法**：编译前先执行一次 `//update -p`（全 plot 刷一遍），
    再 `/redpiler compile`；不刷就会踩这个冷启动 bug。已写进手册 §2 第 15 条、
    §3.4、§5（编译选项后）与 §11.4 症状表。BUG-038 本身保持搁置。
- [x] **BUG-039：`/coord in` 的 `io.txt` 补全在"第一次进入世界"时出不来（2026-11-05 修复，真机验证通过）**
  - 现象：刚进世界时输入 `/coord in` 按 Tab，`io.txt` 没有候选。
  - 涉及代码：`crates/core/src/plot/packet_handlers.rs:74` `handle_command_suggestions_request`
    → `crates/core/src/plot/coord.rs:242` `coord_suggestions`
    → `crates/core/src/plot/coord.rs:169` `file_candidates()`（**每次 Tab 都重新读 `io.txt`**，只有 `groups` 非空才把 `io.txt` 放进候选）；
    命令树节点见 `crates/core/src/plot/commands.rs:1641`（85 号节点，`suggestions_type = minecraft:ask_server`）。
  - 待确认（按排查成本排序）：
    1. 是否只是没打尾空格——文本是 `/coord in` 时当前词就是 `in`，`io.txt` 被前缀过滤掉；打成 `/coord in ` 才有候选。
       这不算 bug，但用户视角就是"补不出来"，值得改：以字面量参数结尾时也能补（把当前词当空）。
    2. 首次进服时命令树/补全请求的时序问题（客户端拿不到带 `ask_server` 的节点就不会发请求，服务端自然没反应）。
    3. `file_candidates()` 用的是**相对路径** `io.txt`，依赖 CWD；进服早期 CWD/线程上下文若不同会读不到。
  - 备注：服务端对不认识的补全请求**完全不回包**（见 BUG-040），客户端表现同样是"没反应"，
    所以 039 和 040 的症状很难区分。建议一起修：**凡是 `ask_server` 的请求，匹配不上也要回一个空响应包**。
  - **2026-11-05 已写码（部分，未编译、未测试）**：
    - `crates/core/src/plot/coord.rs` `coord_suggestions`：文本正好停在字面量子命令上（`/coord in`、`/coord new`、`/coord add`）
      时，把当前词当成空、`start` 取 `text.len()`，于是按**下一个参数**给候选（`/coord in` 直接出 `io.txt`）。
    - `crates/core/src/plot/packet_handlers.rs`：任何 `ask_server` 请求现在**一定会回包**，匹配不上回空 `CCommandSuggestionsResponse`
      （`start = text.len()`、`length = 0`）。
    - **2026-11-05 真机验证通过**（用户确认"成功了"，同一包 `mchprs-0.6.1-java-tabfix.apk`）：
      `/coord in` 能出 `io.txt`。原先"首次进世界补不出来"在改后已复现不到。
      仍未单独验证：极端时序（进服第一帧就按 Tab）不保证一定命中，但不会再静默无响应。

- [x] **BUG-040：`//schem load` 不能 Tab 补全（2026-11-05 已写码，真机验证通过）**
  - 原因：`crates/core/src/plot/packet_handlers.rs:99` 只认 `packet.text.starts_with("//load ")`，且 `start` 硬编码成 `7`。
    `//schem` 的 `load` / `save` 节点只是 **redirect** 到 `//load` / `//save`（`crates/core/src/plot/worldedit/mod.rs:1180` 起），
    客户端发来的补全文本仍是 `//schem load xxx`，既不匹配 `/coord `，也不匹配 `//load `，handler 直接 `return`，**一个包都不回**。
  - 修法（待写码）：前缀判断同时支持 `//load ` / `//schem load `（`save` 同理），`start` 用前缀长度算而不是写死 7；
    `schemati = true` 时按 UUID 分子目录那段逻辑照旧复用。
  - 顺带：`//schem list` 不需要补全，但 `//schem save` 也要一起补上，否则两个 redirect 一好一坏更迷惑。
  - **2026-11-05 已写码并真机验证通过**：`packet_handlers.rs` 改成前缀表
    `["//load ", "//save ", "//schem load ", "//schem save "]`，`start` 由前缀长度算，不再写死 `7`；
    `schems` 目录读不到（含 NotFound）时也回空响应，不再静默返回。
    验证包：`app/mchprs.apk` / `Download/mchprs-0.6.1-java-tabfix.apk`（0.6.1-alpha，md5 `e821735f…`）。

- [ ] **BUG-041：`--update` 现在是个空开关，而且"编译前自动 update"在首次编译时不生效（读码得出，2026-11-04）**
  - 事实 1：`CompilerOptions.update` 全仓只有两个读者——`crates/redpiler/src/lib.rs:212`（`Compiler::reset()` 里对
    **整个 plot bounds** 逐块跑 `mchprs_redstone::update`，上游同款）和 `crates/core/src/plot/scoreboard.rs:155`（记分板显示）。
    **编译流程本身完全不读它**，它不是编译选项，是"拆掉旧电路之后把世界刷一遍原版红石"的开关。
  - 事实 2：`reset()` 结尾 `self.options = Default::default()`，所以是一次性的；`compile()` 结尾把本次 options 存进 `self.options`，
    即"这次编译给下次拆电路用"。
  - 事实 3：`/redpiler compile` 两条路径（`commands.rs:185`、`commands.rs:378`）都是**先 `reset_redpiler()` 再 `start_redpiler(options)`**，
    所以 reset 消费的是**上一次**的 options。手打 `--update` 对本次编译无效，只对下一次拆电路有效——重编译时那正好就是"编译前"。
  - 事实 4：本 fork 在 `crates/core/src/plot/mod.rs:1935`（`start_redpiler`）**强制 `options.update = true`**（提交 `3d83f26`「编译前自动 update」）。
    结论：`/redpiler compile --update` 与 `/redpiler compile` **完全等价**，`--update` 已是空开关；参数可以留（兼容），但别再指望它区分行为。
  - 事实 5（**这是缺陷**）：`reset_redpiler()` 里的 reset 被 `if self.redpiler.is_active()` 包着。
    **刚启动服务器 / 刚加载 plot、还没有活动电路时，第一次 `/redpiler compile` 不会跑 update**，直接拿磁盘上的方块状态建图。
    上一次若是不干净退出（进程被杀、没走 `PlotUnload`），世界就是脏的，这一发编译就带着脏初态——和 BUG-038 的"冷启动前 32 组"同一条线索。
  - 建议修法（待写码）：把 update 从"reset 的副产物"改成 `start_redpiler` 里的**显式前置步骤**：
    `reset_redpiler()` 之后、`self.world.to_be_ticked.drain(..)` 之前，无条件对 bounds 跑一遍
    `for_each_block_mut_optimized(... mchprs_redstone::update ...)`；`CompilerOptions.update` 只保留给记分板/兼容。
    这样首次编译也覆盖，而且不再依赖"上一次编译存的 flag"。
  - 代价记录：这一遍是 O(非空 section 里的所有方块)，每块都要查邻居、还可能 `schedule_tick`（torch/repeater），
    `reset_redpiler` 里那句 `reset_timings()` 的注释就是为此写的（"reseting redpiler could cause a large amount of block updates"）；
    之后 `publish_world()` 还会把方块变化推给客户端。

- [x] **BUG-042：去掉 `/redpiler compile` 的"默认带 update"（2026-11-05 已写码，未编译）**
  - 用户 22:27 的原话：`--update` 这套自动机制绕了一层、还跟 `//update` 重复，"去掉 update 标志，
    只能建议玩家在编译前打一次 `//update -p`"；22:31 澄清：**只改一处**——
    现在 `/redpiler compile` 默认是带 `--update` 的，改成**默认不带**，其他都不动。
  - 唯一改动点：`crates/core/src/plot/mod.rs:1931-1935`（`start_redpiler`）删掉
    `// Every compile updates the whole plot first, silently ...` 那段注释和 `options.update = true;`。
  - **不动**的：`CompilerOptions.update` 字段、`--update` / `-u` 解析、`Compiler::reset()` 里的整图 update 循环、
    记分板显示、Java 命令树 71 号节点、基岩命令表。也就是 `--update` 作为**显式选项仍然可用**，
    只是 `/redpiler compile` 不再偷偷替你打开它。
  - 行为变化：默认编译不再在拆电路时刷全图；`//update -p` 成为"编译前把世界刷干净"的推荐做法
    （它有 `mutates_world` 默认 true，会先拆电路再刷，正好是编译前的顺序；默认不再有 update 之后它只刷一遍）。
  - 仍要写进文档的一条坑：显式打 `--update` 也不是"这次编译前刷"，`reset()` 消费的是**上一次**存的 options，
    真正生效在下一次拆电路（重编译时正好就是紧随其后的那次 reset）。所以要确定刷干净，还是 `//update -p`。
  - **状态：代码已改（2026-11-05 11:50），未编译、未测试**。改动只有 `start_redpiler` 头部：
    删掉强制 `options.update = true` 与 `let mut options = options;`，换成一段说明注释；
    函数里其余代码未动（`options` 已无任何字段赋值，无需 `mut`）。
    编译/测试等用户发话。

- [x] **BUG-043：`/redpiler stats` —— 自动编译 + 全量出入度统计写入 `logs/`（2026-11-05 已写码并编译，未真机测试）**
  - 来源：RCU 群 ElePrC 的需求——元件是有向图节点，统计所有节点入度/出度，输出每个度数的节点数与占比。
  - **形态（用户定稿，不再问）**：
    - 同时提供**控制台命令**和**玩家命令**：`/redpiler stats`（`/rp stats` 同效）。
    - 玩家侧要在命令树里给 `/redpiler` 加 `stats` 子节点（现有只有 compile/inspect/reset），并加进 `/rp` 的 redirect 覆盖范围；
      基岩侧若要同步，用 `generators/gen_commands.py` 重新生成，不手改 `commands_gen.rs`。
    - 控制台侧挂在 `execute_automation_command` 的 `redpiler` match（`commands.rs:366`）里加 `"stats"`，
      `handle_console_command`（`commands.rs:357`）自动覆盖；自动化脚本发的 `/redpiler stats` 也走同一条。
  - **行为**：输入即执行，**不需要先编译**——命令内部先跑一次编译（与 `/redpiler compile` 同一路径：
    `reset_redpiler()` → `start_redpiler(默认 CompilerOptions, source)`），编译成功后再统计、写文件。
    编译失败/取消则报失败，不写文件。
  - **全量统计**：度数分布覆盖 `0..=最大值` 全部取值，不截断、不分档；按节点类型也是全量列出。
    出度 = `forward_links.get(&node.fwd_link_range)` 长度；入度 = 一趟 `O(V+E)` 累积（不逐节点反查）。
  - **输出文件**：写到 `logs/` 目录下（不存在就 `create_dir_all`），文件名带时间戳，例如
    `logs/redpiler_stats_20261105_195500.txt`，不覆盖旧结果。
    内容：表头（版本、时间、节点数/边数）、`[入度分布]`、`[出度分布]`、`[按节点类型]`、出度/入度极值节点明细（含坐标）。
  - **终端只打印是否成功**：成功一行 `Done.`，失败一行错误码（如 `ERR compile`）；**不打印路径、不打印表格**。
  - 与运行无关：只读编译结果，`/tick seq` 冻住 tick、rtps = 0 时照样可执行。
  - **2026-11-05 已写码、已编译（`cargo build --release`，不带 bedrock），未真机测试。**
  - 改动清单：
    - 新增 `crates/redpiler/src/backend/direct/stats.rs`：`DirectBackend::degree_report()`，
      一趟 `O(V+E)` 求出度/去重出度/入度，输出 `[node types]` / `[in-degree distribution]` / `[out-degree distribution]` /
      `[nodes with out-degree >= 2]` / `[nodes with in-degree >= 2]`（含坐标），分布表覆盖 `0..=最大值` 全部度数。
    - `crates/redpiler/src/backend/direct/mod.rs`：`mod stats;`。
    - `crates/redpiler/src/lib.rs`：`Compiler::degree_report() -> Option<String>`（未编译返回 `None`）。
    - `crates/core/Cargo.toml`：新增 `chrono = { workspace = true }`，只用于文件名时间戳。
    - `crates/core/src/plot/mod.rs`：`Plot::compile_and_write_stats()`——先 `reset_redpiler()` + `start_redpiler(默认 options, source)`，
      成功才写 `logs/redpiler_stats_<YYYYmmdd_HHMMSS>.txt`；失败返回 `ERR compile` / `ERR io`，不写文件。
    - `crates/core/src/plot/commands.rs`：玩家命令 `"stats"` 分支（聊天栏 `Done.` / `ERR <code>`）、
      控制台/自动化 `"stats"` 分支（控制台只打 `Done.` / `ERR <code>`）、命令树新增 90 号节点 `stats`，
      44 号 `/redpiler` 的 children 改为 `[46, 47, 48, 90]`（`/rp` 靠 redirect 自动覆盖）。
  - 编译结果：`Finished` in 4m 19s（新增 chrono 依赖），二进制 8,793,368 字节；
    警告仍是存量（redpiler 1 条 `AnalysisUsage::reset`、core 3 条），无新增。
  - 待办：真机验证；**基岩命令表 `commands_gen.rs` 未重新生成**（本次不编 bedrock），
    以后要开 bedrock 时先跑 `python crates/network/generators/gen_commands.py`。

- [x] **BUG-044：`/redpiler stats` 的输出改成 Markdown（2026-11-05 已写码、用户编译、已打包）**
  - 需求（用户 2026-11-05 20:10）：统计文件从纯文本 `logs/redpiler_stats_<时间戳>.txt` 改成 Markdown `logs/redpiler_stats_<时间戳>.md`。
  - 改动：
    - `crates/redpiler/src/backend/direct/stats.rs`：报告改写为 Markdown——`## Node types` / `## In-degree distribution` /
      `## Out-degree distribution` / `## Nodes with out-degree >= 2` / `## Nodes with in-degree >= 2` 全部用表格；
      末尾加 `## Notes` 写明四条读法：统计的是调度图、入度按 link 计不去重、wire 在图里是汇、选项是本次编译用的。
    - `DirectBackend::degree_report(&self, timestamp: &str)`：时间戳由调用方传入（redpiler 不引 chrono）。
    - `Compiler::degree_report(&self, timestamp: &str)` 同步改签名。
    - `crates/core/src/plot/mod.rs::compile_and_write_stats`：把 `%Y-%m-%d %H:%M:%S` 传进报告，文件名改 `.md`。
  - 状态：代码已改，用户 20:11 自行编译（二进制 8,795,592 字节），20:13 打包为
    `Download/mchprs-0.6.1-java-statsmd.apk`（md5 `3fe6f1f68a6cc21d122301c49f908eca`，3,707,751 字节，不带 docs）。

- [ ] **BUG-045：控制台支持 `//update -p`（2026-11-05 已写码、已编译、已打包，待真机验证）**
  - 现象（用户 2026-11-05 23:18）：在服务器终端敲 `//update -p`，得到
    `[console] This command needs a player and cannot be used from the console`。
  - 原因：`handle_console_command` 没有世界编辑分支，所有 `//` 命令落到
    `_ => NEEDS_PLAYER`；而 `execute_update` 的 `-p` 分支用的是 `ctx.plot.get_corners()`，
    本来就不依赖玩家选区。
  - 改动：
    - `crates/core/src/plot/worldedit/mod.rs`：`update()` 改为 `pub(super)`。
    - `crates/core/src/plot/commands.rs`：控制台新增 `"/update" | "update"` 分支，
      只接受 `-p`（其它参数回 `Usage: //update -p (updates the entire plot)`），
      成功后打印 `Updated the entire plot. (<耗时>)`。
  - APK：`app/src/com/dsh/mchprs/MainActivity.java` 的快捷指令栏（`QUICK_CMDS` / `QUICK_FILL`）
    加了一个 `//update -p` 小标签，点一下只填充输入框、不发送。
  - 未做：不带 `-p` 的 `//update` 仍需要玩家选区，控制台依旧报 NEEDS_PLAYER。
  - 编译：`cargo build --release`，2m 06s，二进制 8,795,600 字节（警告仍是存量 4 条）；
    APK 重新打包（WITH_ASSETS=0）为 `Download/mchprs-0.6.1-java-joinmsg.apk`，
    md5 `ddb8e909f721cba0483d4d4c8a21bb7e`，3,707,751 字节。

- [ ] **BUG-046：玩家进入/离开没有全服提示（用户 2026-11-05 提出，当天已写码、已编译、已打包，待真机验证）**
  - 现象：一个玩家进服时，已在线的其他玩家地上没有任何聊天栏提示；退出时同样没有。
  - 对照结果：**上游 MCHPRS 也是这样**（`crates/core/src/server.rs` 上游 `Message::PlayerJoined` 只有
    `info!("{}" joined the game")` 写日志 + 发 `PlayerJoinedInfo` 刷新 Tab 列表，
    没有任何聊天广播），不是 fork 引入的回归。
  - 实现（2026-11-05，用户选择“现在就改”）：
    - `crates/core/src/server.rs::handle_message`：`Message::PlayerJoined` 在 `info!` 之后广播
      `BroadcastMessage::Chat(uuid, &e<name> joined the game)`；`Message::PlayerLeft` 在取出玩家后
      广播 `&e<name> left the game`。措辞与原版一致（黄色）。
    - 已知差异：进服那个玩家自己看不到自己的“joined”（广播时他还没进 plot），原版是能看到的。

- [ ] **BUG-047：版本升到 0.6.2-beta、双二进制 / 双 APK、内置 docs+python（用户 2026-11-05 23:43）**
  - 需求：版本号升到 `0.6.2-beta`；这次编译**两份二进制**（带基岩 / 不带基岩）；
    APK 内置 python 代码与文档（**不含台账与上游文档**）；并且内置文件
    **只在第一次启动时解压一次**，之后不再碰；另外要打**两个 APK**。
  - 改动：
    - `Cargo.toml`、`app/AndroidManifest.xml`：`0.6.1-alpha` → `0.6.2-beta`，versionCode 609 → 610。
    - `app/build.sh`：`BEDROCK=1` 时改用 `~/mchprs/mchprs-bedrock`、产出 `mchprs-bedrock.apk`、
      `--version-code 611`；默认产出 `mchprs.apk`、`--version-code 610`。
      `VERSION_NAME=0.6.2-beta` 由脚本传给 `aapt2 link`。
    - `WITH_ASSETS` 默认从 0 改为 1：打包 `docs/` 顶层两份手册、`python/`、根 `README*.md`；
      不打包 `docs/backlog/`、`docs/upstream/`、`docs/mchprsio/`。
    - `app/src/com/dsh/mchprs/Assets.java`：改成标记文件 `工作目录/.apk-assets-done`，
      没有标记时解压一次并写下标记，之后启动直接跳过；解压本身仍只补缺、不覆盖。
      想重新解压：删掉标记后再启动。
    - `python crates/network/generators/gen_commands.py` 重跑一次（本次要编基岩版），
      `commands_gen.rs` 没有实质变化（24 命令）。
    - 两份手册与 `README*` 里的版本字符串改为 `0.6.2-beta`；
      "since 0.6.1-alpha" 这类历史陈述保留。
    - 手册 §10 重写：两个 APK 的构建方式、内置范围、首启解压与标记文件。
  - 产物/结果（2026-11-05 23:54）：
    - `cargo build --release` 3m 30s → `~/mchprs/mchprs`，8,795,640 字节，`--version` 回 `0.6.2-beta`。
    - `cargo build --release --features bedrock` 3m 59s → `~/mchprs/mchprs-bedrock`，12,849,016 字节。
    - APK：`app/mchprs.apk`（versionCode 610，3,787,132 B，内置 java-only 二进制）；
      `app/mchprs-bedrock.apk`（versionCode 611，5,155,196 B，内置基岩版二进制）。
      两份 APK 都含 assets：`docs/Instruction-manual*.md`、`python/`、`README*.md`
      （无 docs/backlog、docs/upstream、docs/mchprsio）。
    - 副本到 `Download/`：`mchprs-0.6.2-beta-java.apk`（md5 e983190e…）、
      `mchprs-0.6.2-beta-bedrock.apk`（md5 bab7b709…）、`mchprs-0.6.2-beta-aarch64-java`（md5 1bd13a83…）、
      `mchprs-0.6.2-beta-aarch64-bedrock`（md5 3e358593…）。
  - 已知：`BEDROCK` 版与普通版**包名相同**，手机上只能同时存在一个；安装另一个会覆盖。
  - **2026-11-05 23:57 已发布 GitHub Release**：`v0.6.2-beta`（prerelease），
    标题 `v0.6.2-beta — 正式公测`，上传了 `mchprs-0.6.2-beta-java.apk`（§3,787,132 B）与
    `mchprs-0.6.2-beta-bedrock.apk`（§5,155,196 B）；aarch64 二进制交给 tag 触发的 CI 编译。
    README（中英）标题下方已加发布公告 + 本版更新说明；提交 `748c2e6` 已推送。

- [ ] **Java 侧区块推送**：客户端视距 6 / 服务端按 8 推，空洞随机分布，发过不再补（用户 2026-11-02 决定暂不修）。
- [ ] **模电 `matrix:`**：报 `a matrix: list cannot be analog`。
- [ ] **上游 workflow 在 tag 上误报失败**：`build.yml` / `ci.yml` / `docker.yml` 监听 `v*`。
- [ ] **APK 609 的自动释放资源**需真机确认（`assets` 解包、覆盖策略）。
- [ ] **基岩监听地址（第 54 条）**需真机复测重连行为。
- [ ] **存量警告**（`is_running_behind`、`fetch_update`、`info` 导入等），用户 2026-11-03 决定不处理。

---

## 15. 版本摘要

| 版本 | 状态 | 要点 |
| --- | --- | --- |
| `0.6.2-beta` | ✅ 当前 | 双二进制（含基岩）、双 APK、内置 docs/python 仅首启解压 |
| `0.6.1-alpha` | ✅ | 日志英文化、基岩日志收口、监听地址跟随 `bind_address`、插座/插头拆分 |
| `0.5.4-alpha` | ✅ | `width = 0` 时钟端口；删 `manual.md`，`file-formats` 增时钟章节 |
| `0.5.2-alpha` | ✅ | 模电 `matrix:`、日志去重、文件版本兼容；自动化协议定型 |
| `0.5.1-alpha` | ✅ | 模电双键（`rs_width` + `width`）、动词大写 |
| `0.5.0-alpha` | ✅ | 首个带「自动化 + io.txt + 存储器」的版本 |
| `0.4.x` / `0.3.0` / `0.2.x` | ✅ | 早期迭代，详见归档 |

- 归档：旧台账 `~/.ssset/backup/BACKLOG.pre-rewrite-2026-10-28.md`、`BACKLOG.archive-2026-10-27.md`。

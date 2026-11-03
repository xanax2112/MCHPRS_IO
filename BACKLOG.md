# MCHPRS_IO 项目台账（对照上游的差异清单）

> 本文件记录本仓库相对**上游 MCHPRS** 的全部差异，按文件位置组织，只列"有什么"。
> 上游基线：`~/mchprs/source/MCHPRS`（`master` @ `d492e43`，Minecraft 1.20.4 / 协议 765）。
> 基岩版（Bedrock）不在本文件范围，见 `BEDROCK.md`。
> 图例：`✚` 新增 · `✎` 修改 · `✖` 移除。

---

## 0. 协作约定

**五条铁律**（用户 2026-10-25 下达，优先级高于一切效率诉求）

1. **目标先行**：写代码前必须完全明确用户目标，不明确就反问，禁止猜测与脑补。
2. **只记录不改码**：功能 / 改进 / 缺陷一律先登记进台账；只有用户明确说"写代码"才动代码。
3. **写码与编译分离**：说"写代码"才写，说"编译"才跑 `cargo`，两步不自动衔接。
4. **全程中文汇报**。
5. 确认语义永远优先于推进速度。

**附加约定**

- **台账按文件位置归属**（用户 2026-11-03 下达）：`crates/network/src/bedrock_api.rs`、`crates/network/src/bedrock/**`、`vendor/bedrock-rs/**` 记在 `BEDROCK.md`；其余（`crates/core`、根 `Cargo.toml`、`src/`、`app/`、`python/`、`docs/`、CI）记在本文件。
- **本文件里的文件格式规则是契约**：`docs/file-formats*.md` 是面向用户的手册，两边内容必须一致；改了格式先改本文件，再同步文档。
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

`✚` 上游没有、本仓库新增：

| 路径 | 说明 |
| --- | --- |
| `app/` | Android APK 工程（Java，把 `mchprs` 打成独立 App，包名 `com.dsh.mchprs`） |
| `python/` | 协议测试工具（`correctness.py` / `timing.py` / `pipeline.py` + `core/`），自带领 `PYTHON-BACKLOG.md` |
| `run/` | 服务端运行目录（`Config.toml` / `io.txt` / `logs/` / `world/`） |
| `docs/protocol.md`、`docs/protocol_zh.md` | 自动化协议规格（中英） |
| `docs/file-formats.md`、`docs/file-formats_zh.md` | `io.txt` 与存储器文件格式（中英） |
| `.github/workflows/mchprs-io-ci.yml` | 本仓库自己的 CI（`test` + `build` 两个 job） |
| `README_zh.md` | 中文 README（上游只有英文） |
| `BEDROCK.md` | 基岩版台账（本文件不涉及） |

`✖` 上游有、本仓库删除：

| 路径 | 说明 |
| --- | --- |
| `crates/core/src/profile.rs` | Mojang 玩家档案查询（`/whitelist add <name>` 用）；随白名单功能一起删除 |

`✎` 上游有、本仓库改过（按 crate 归类）：`crates/core`（12 个文件）、`crates/redpiler`（10）、`crates/redstone`（2）、`crates/network`（2）、`crates/text`（1）、`crates/rilc`（2）、`tests/`（2）、`Cargo.toml`、`Dockerfile`、`README.md`、`docs/Redpiler.md`。

---

## 3. 配置项差异（`crates/core/src/config.rs`）

`✚` 新增：

| 键 | 默认值 | 说明 |
| --- | --- | --- |
| `io_port` | `25585` | 自动化桥的 TCP 端口，**只绑 127.0.0.1**；绑定失败只警告、服务端继续跑 |
| `performance` | `false` | `true` = 每 tick 批量采样（性能模式）；`false` = 逐 tick 精确采样 |
| `betterio` | `false` | 打开 `betterio` 时序修正（见 §10） |
| `bedrock` | `19132` | 基岩版 UDP 端口；`0` = 关闭；详见 `BEDROCK.md` |

`✖` 移除：

| 键 | 原因 |
| --- | --- |
| `whitelist` | 白名单功能整体删除（连同 `/whitelist` 命令与 `profile.rs`） |
| `auto_redpiler` | 自动编译功能删除（连同 `/toggleautorp`），只保留手动 `/redpiler compile` |

`✎` 语义变化：`bind_address` 的 IP 现在也决定基岩版 UDP 绑哪个网卡（见 `BEDROCK.md` 第 54 条）。

---

## 4. 新增模块（`crates/core/src/`）

| 模块 | 行数 | 作用 | 规则 |
| --- | --- | --- | --- |
| `automation.rs` | 811 | 自动化桥：明文 TCP，让脚本驱动/观测端口 | 报文见 `docs/protocol.md` |
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
- `✚` **自动化桥**（`automation.rs`）：纯文本 TCP，端口 `io_port`（默认 25585，只绑 `127.0.0.1`）；报文 `HELLO` / `INPUT` / `POUT` / `OUTPUT` / `READ` / `TICK` / `NOW` / `RADV`；版本串 `MCHPRS_IO 0.5.2-alpha`；规格见 `docs/protocol.md`。
- `✚` `Message` 扩展：`IoResponse`、`IoDataPortOutput`、`SeqModeChanged`、`ConsoleCommand`、`AutomationRequest`、`AutomationDisconnect`。

### 9.1 自动化协议下沉到 plot 线程（📝 已定待写码，2026-11-04）

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
- `✚` `READ x,y,z` 收窄属于行为变更，写进 `docs/protocol.md`。
- `✚` 主循环 5 ms 把玩家包刷新降到 200 Hz（有意取舍）。
- `✚` 清理 `server.rs` 中 `IoResponse` / `IoDataPortOutput` / `SeqModeChanged` 与 `AutomationAction::Route` / `Disconnect` 的残留分支。

预期：往返由约 12.5 ms 降到「1 tick + loopback」；655360 tick 的批测输出按 5 ms 分批发，脚本持续读取即可，不会撞断连。

### 9.2 IO 采样与写入：去掉每 tick 的分配与哈希查找（📝 已定待写码，2026-11-04）

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

### 9.3 预约表：单调化 + 上限 65536（📝 已定待写码，2026-11-04）

- `✎` **上限**：`MAX_RESERVATIONS` 由 255 提到 **65536**（用户 2026-11-04 定）。每条约 170 字节（64 位组：结构体 56 + 组名堆约 32 + `bits` 堆约 80），65536 条约 **11 MB**。30 万条以上才需要驻留组名 + 位打包（可降到约 24 字节/条），本次不做。
- `✚` **单调约束**：预约必须按 tick 单调不减发送，违反返回新错误码 `ERR order`。理由：单调后表天然有序，出队是游标 O(1)，查重只需从尾部扫当前 tick 的那几条——替掉现在每 tick 全表 `retain` 加每条 `clone` 的写法（655360 tick × 65536 条约 2×10^10 次遍历）。
- `✎` **结构**：`reservations: Vec<Reservation>` 改为 `VecDeque<Reservation>` 加 `last_tick: u32`。
  - `reserve_input`：`tick < last_tick` 拒绝；`tick == last_tick` 从尾部往前扫同 tick 条目，同组覆盖（保住“后到覆盖”语义）；否则 `push_back`。
  - `apply_reservations`：`while front.tick == now { pop_front(); group.write(...) }`。
  - `VecDeque` 出队不自动缩容，需在清空或长期低水位时 `shrink_to_fit`，否则峰值决定常驻内存。

插入摊还 O(1)，应用 O(k)（k 为该 tick 的条目数）。配合流式灌入时表里始终保持一个批次（65536 条 / 11 MB）。

`✎` **`offset` 扩成 `u32`（用户 2026-11-04 定）**：`RequestKind::Input { offset: Option<u32> }`。解析保持**十进制**不变（现在就是 `args[0].parse::<u32>()`，随后才压成 `u16`），改动只是去掉 `offset > u16::MAX` 的 `ERR range` 判断与 `as u16` 转换；`mod.rs` 侧 `u32::from(offset.unwrap_or(1))` 直接用。`offset == 0` 仍然非法（“下一 tick”用不带 offset 的形式）。值仍是十六进制，语法 `INPUT <name> <offset> <hex>` 不变。

### 9.4 自动化消息的字符串分配（📋 待做，2026-11-04）

每条消息当前的开销：

| 方向 | 分配点 | 每条成本 |
| --- | --- | --- |
| 输入 | `name.to_string()` + `hex.to_string()` + `decode_hex` 内两次 `Vec<bool>` | 1~1.5 µs |
| 输出 | `encode_hex` 的 `String` + `format!` 整行 + `hex.clone()` | 0.5~1 µs |

65536 条一批合计 **30~100 ms**，与 Python 侧解析同量级。

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

除基岩版外（见 `BEDROCK.md`），与上游的差异只有三处：

- `✚` **插座 / 插头**（2026-11-03）：基岩协议栈拆成接口层 `crates/network/src/bedrock_api.rs`
  （12 个数据类型 + `BedrockStack` / `ChunkEncoder` 两个 trait + 全局注册，常驻编译、只依赖 std）
  与实现层 `crates/network/src/bedrock/**`（`--features bedrock` 才编译，整个文件夹可删）。
  core 侧只通过插座拿编码器/换算接口，不再引用实现模块。完整记录见 `BEDROCK.md` §9.41。
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

- `✚` `docs/protocol.md` / `protocol_zh.md`：自动化协议完整规格。
- `✚` `docs/file-formats.md` / `file-formats_zh.md`：本文件 §5–§7 的面向用户版本（含示例）。
- `✚` README（中英）"快速开始"：依赖、构建运行、进服编译、io.txt、协议、python 工具、故障排查。
- `✚` `python/`：协议测试工具（正确性、时序、流水线），自带台账。
- `✚` `app/`：APK 工程（`build.sh` 一键出包，可内置文档与 python 工具）。

---

## 14. 未结项（待办清单）

- [ ] **Java 侧区块推送**：客户端视距 6 / 服务端按 8 推，空洞随机分布，发过不再补（用户 2026-11-02 决定暂不修）。
- [ ] **模电 `matrix:`**：报 `a matrix: list cannot be analog`。
- [ ] **上游 workflow 在 tag 上误报失败**：`build.yml` / `ci.yml` / `docker.yml` 监听 `v*`。
- [ ] **`python/` 没有冒烟测试**：三个脚本目前靠人工跑。
- [ ] **APK 609 的自动释放资源**需真机确认（`assets` 解包、覆盖策略）。
- [ ] **基岩监听地址（第 54 条）**需真机复测重连行为。
- [ ] **存量警告**（`is_running_behind`、`fetch_update`、`info` 导入等），用户 2026-11-03 决定不处理。

---

## 15. 版本摘要

| 版本 | 状态 | 要点 |
| --- | --- | --- |
| `0.6.1-alpha` | ✅ 当前 | 日志英文化、基岩日志收口、监听地址跟随 `bind_address`、插座/插头拆分 |
| `0.5.4-alpha` | ✅ | `width = 0` 时钟端口；删 `manual.md`，`file-formats` 增时钟章节 |
| `0.5.2-alpha` | ✅ | 模电 `matrix:`、日志去重、文件版本兼容；自动化协议定型 |
| `0.5.1-alpha` | ✅ | 模电双键（`rs_width` + `width`）、动词大写 |
| `0.5.0-alpha` | ✅ | 首个带「自动化 + io.txt + 存储器」的版本 |
| `0.4.x` / `0.3.0` / `0.2.x` | ✅ | 早期迭代，详见归档 |

- 归档：旧台账 `~/.ssset/backup/BACKLOG.pre-rewrite-2026-10-28.md`、`BACKLOG.archive-2026-10-27.md`。

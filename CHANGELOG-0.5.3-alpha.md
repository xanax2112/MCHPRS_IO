# MCHPRS_IO 0.5.3-alpha 更新说明

发布日期：2026-10-28（未发布 Release，仅源码与构建产物）

本次更新围绕三件事：**给 IO 加一个可选的时序模式（`betterio`）**、**新增 `/coord` 用木斧直接写坐标文件**、**把指令系统补全并修掉一批老问题**。所有改动都在 `main`，没有打 tag、没有 Release。

---

## 一、新增功能

### 1. `betterio` 配置开关（`Config.toml`）

新增布尔键，默认 `false`：

```toml
betterio = false
```

`false` 时行为与 0.5.2-alpha 完全一致；`true` 时有两处时序变化：

| 元件 | `false`（默认） | `true` |
| --- | --- | --- |
| 石按钮 | 按下后 **10 tick** 弹起 | 按下后 **2 tick** 弹起 |
| 红石灯 | 亮无延迟、熄延迟 **2 tick** | 与活板门一致，亮灭都在当 tick 生效 |

- 与 `performance` **完全独立**，任意组合都允许。
- 缺键时启动自动补写 `betterio = false`，旧配置不用手改。
- 按钮一次按下只产生一次上升沿，按下期间重复触发仍被忽略；`betterio` 只是把窗口从 10 tick 缩到 2 tick。

### 2. `/coord`：木斧取点写坐标

用 WorldEdit 木斧右键取**第二点**，然后直接把它写进 `io.txt` 或存储器文件，不必再手敲坐标。

```text
/coord in <file> [name]
/coord new <type> [步长] [个数] [方向]
/coord add <type> [步长] [个数] [方向]
/coord undo
/coord redo
```

| 写法 | 作用 |
| --- | --- |
| `/coord in io.txt <组名>` | 进入 `io.txt` 的某个组（不分输入输出） |
| `/coord in xx.rom` / `xx.ram` | 进入 `memory/xx.rom.txt` / `xx.ram.txt` |

`type` 短码：

| 短码 | 含义 | 坐标数 |
| --- | --- | --- |
| `clk` | 时钟 | 1（只能用 `new`） |
| `addr` | 地址 | 多 |
| `rd` | 读数据（ROM 数据口也是 `rd`） | 多 |
| `wd` | 写数据 | 多 |
| `we` / `re` / `bwe` | 写使能 / 读使能 / 字节写使能 | 多 |
| `a_addr` / `a_rd` / `a_wd` | 模电地址 / 读数据 / 写数据 | 多 |

进入 `io.txt` 的组之后，`type` 只有 `clk` 与 `data`（Tab 补全会按上下文切换列表）。

**规则**

- `new` 覆盖该端口坐标，`add` 追加；单坐标类型只能用 `new`。
- 步长 = 文件语法 `+<n>x` 的 `n`；方向沿用 `//stack`（省略 = 玩家朝向，支持 `l/r/u/d` 与 `n/s/e/w`）；不写步长/个数 = 只写当前点。
- 结果超过文件里写死的 `width`（模电对照 `rs_width`）时**不予执行**。
- 文件里 `origin` 非零时写入的是**相对坐标**（自动减 origin）。
- `width` 不会被自动修改；写少了可以，但编译时点数与 `width` 不等会按现有校验禁用该组/文件。
- 不做写前备份；`undo`/`redo` 深度 **8**，超出丢弃最旧。
- **只能在未编译时使用**，回车即保存。
- 成功会弹紫色消息：`10, 64, 5, +2x, 8 -> read data`（坐标串在前、`type` 全码在后）。
- `/coord` 改过文件后，下一次 `/redpiler compile` 会自动重载这些文件一次。

### 3. 指令写法升级

| 新写法 | 等价 / 作用 |
| --- | --- |
| `//schem load <名>` / `//schem save <名>` | 与 `//load` / `//save` 完全等价 |
| `//schem list` | 列出 `./schems` 里的投影 |
| `/tick freeze` | 等价 `/rtps 0`，并记住冻结前的 rtps |
| `/tick unfreeze` | 恢复**冻结前那个** rtps 值 |
| `/tick step <偶数>` | 等价 `/radv (n/2)`；`step 2` = `radv 1`；**奇数直接报错** |
| `/tick rate [数值]` | 等价 `/rtps`；**不带参数等于设为 20**（`/rtps` 空参是打印报告，两者不完全等价） |

### 4. `/io reload`

运行期重读 `io.txt`，不用重启：

- 与 `/memory reload` 同一套流程：`-i` 下拒绝；已编译时先退出编译态再重读；未编译时直接重读。
- `/io`（含 `list` 与 `reload`）现在**控制台与游戏内都能用**，并补进了 Tab 补全树。

### 5. Tab 补全扩展

- 新增 `/tick` 的 `freeze` / `unfreeze` / `step` / `rate` 子命令补全；
- 新增 `/io`、`/coord` 的补全；
- `/redpiler compile` 会补全**完整长参数**：`--optimize --export --io-only --update --export-dot --wire-dot-out --illegal-states-out --wire-cross-out --print-after-all --print-before-backend --passes=`，并且输入空格后继续补；
- `//schem` 的 `load` / `save` 直接复用 `//load` / `//save` 的补全节点。

### 6. 启动自动初始化

首次启动（或目录为空时）自动创建：

| 路径 | 内容 |
| --- | --- |
| `memory/` | 存储器目录 |
| `memory/.rom.txt`、`memory/.ram.txt` | **隐藏模板**，骨架内容：版本行 + `origin = 0` + `clk`（ROM 写 `0`，RAM 写 `0, 0, 0`）+ 各角色 `width = 0` 空列表 |
| `io.txt` | 版本行 `v0.5.3-alpha` + `input none_in` / `output none_out` 两个空组骨架 |

模板以点号开头，加载器会跳过，不会被当成存储器。

### 7. 重复 `POUT`

对已经订阅过的输出组再发一次 `POUT`，服务器会**再推一条当前值**（`OUTPUT`）；订阅关系与后续推送规则不变。

---

## 二、修复与行为调整

| 项 | 变化 |
| --- | --- |
| `io.txt` 输入白名单 | 现在也接受**石按钮**，报错文案改为 `lever, pressure plate or button`；写入 `1` 表示脉冲按下、`0` 无为 |
| `io.txt` 版本行 | 现在也接受**只写版本号**（`v0.5.3-alpha`）的首行；生成的骨架就用这个形式 |
| 空组 | `width = 0` 且无坐标的组不再报错，作为空组加载（骨架需要） |
| 同一按钮双角色 | 仍然禁止：同一按钮不能既当 `clk` 又当数据点位 |

---

## 三、升级方式

1. 替换二进制即可，**配置与存档无需修改**：
   - `Config.toml` 会自动补写 `betterio = false`（保持旧行为）；
   - 已有的 `io.txt`、`memory/*.txt` 继续可用；
   - 缺失的 `memory/.rom.txt` / `.ram.txt` / `io.txt` 会在启动时补上。
2. 想试新时序：把 `betterio` 改成 `true` 后重启。
3. 二进制位置：`target/release/mchprs`（8,567,408 字节，sha256 `e8eba427…`）。

---

## 四、已知限制 / 待验证

- **`/coord` 尚未实机验证**：需要玩家在游戏里用木斧取点才能跑通，本轮只做到编译通过 + 启动验证。
- **`betterio` 的两条时序未上电路实测**。
- **“派发与补全单一来源”只做了一半**：补全由扩展后的节点表驱动，指令派发仍是 `match`，完整重构未进行。
- `docs/protocol*.md` 已补“重复 `POUT`”，但 `POUT` 的完整说明仍以协议文档为准。
- 本轮**未打 tag、未发 Release**，GitHub Actions 未触发。

---

## 五、相关文档

- 完整说明书：[docs/manual_zh.md](docs/manual_zh.md)（[English](docs/manual.md)）
- 协议：[docs/protocol_zh.md](docs/protocol_zh.md)（[English](docs/protocol.md)）
- 文件格式：[docs/file-formats_zh.md](docs/file-formats_zh.md)（[English](docs/file-formats.md)）
- 台账与设计决策：[BACKLOG.md](BACKLOG.md)

# 0.5.3-alpha 未验收清单

> 生成时间：2026-10-29
> 用途：把「已经写进代码、但还没有被你实际验收」的东西一条条列清，避免把“编译通过”当成“功能可用”。
>
> 验收口径：**编译通过 ≠ 已验收**；只有你在游戏里/脚本里实际跑过并确认行为正确，才算通过。

---

## 一、尚未编译进二进制的改动（连编译验证都没有）

这几条是 2026-10-29 当天发现并修复的，**源码已改、尚未编译**，线上二进制里没有它们。

| 项 | 改动 | 现状 | 需要怎么验 |
| --- | --- | --- | --- |
| `/coord` 宽度校验漏洞 | 原来 `if width != 0 && total > width`，`width = 0` 被豁免，能往 `width = 0` 的组里写坐标 | 已改成严格 `total > width`（io.txt 与 memory 两处） | `/coord in io.txt none_in` + `/coord new data`，应报 `Refused: … width = 0` |
| `/coord` 写入后不重载 | `commit()` 未设 `coord_dirty`，导致“编译时自动重载”从未生效 | 已补 `self.coord_dirty = true` | 改文件后直接 `/redpiler compile`，日志应出现重新加载 `io.txt` 的新报错/成功信息 |
| `/coord in io.txt` 组名补全漏组 | 候选取自校验后的 `IoTable::groups`，组级错误（如 `width = 1` 无坐标）的组被隐藏 | 已改为扫描 `io.txt` 原文的 `input/output <名> = [` | `/coord in io.txt ` 后应同时出现 `none_in` 与 `none_out` |

---

## 二、已编译进二进制、但未验收

线上二进制：`~/mchprs/mchprs`，0.5.3-alpha，2026-10-29 09:26（sha256 前缀 `5799cfcc`）。

| 项 | 实现内容 | 已有验证 | 待验收内容 |
| --- | --- | --- | --- |
| `betterio`（已编译路径） | 按钮释放 10→2 tick；红石灯亮灭都在当 tick | 仅代码审查 | 编译电路里按按钮，2 tick 弹起；灯即时熄灭 |
| `betterio`（未编译路径） | 共享标志下移到 `mchprs_redstone`，未编译模拟器也读同一开关 | 仅代码审查 | **编译前**按按钮应 2 tick 弹起、灯应即时熄灭 |
| 编译时自动 `//update -p` | `/redpiler compile` 强制 `options.update = true`，编译后整块地皮 update | 仅确认编译能过 | 有电路的世界里编译一次，确认世界状态被整理、耗时是否可接受 |
| `/coord` 命令本体 | `in/new/add/undo/redo`、8 步撤销、按行改文件、紫色回馈 | **你已实测**：`/coord`、`in` 正常；`io.txt` 组名补全曾缺组（已修未编译） | 文件补全（`memory/*.rom|ram`）、`type` 补全、`new/add` 实际写入、`undo/redo`、`add` 超 `width` 报错 |
| RAM 模板修正 | `we`/`re` 改单坐标、三个使能块移到 `address coordinate` 之前 | 仅确认二进制里是新写法 | 删掉 `memory/*.txt` 重启，检查生成内容与顺序 |
| 重复 `POUT` | 已订阅的组再 `POUT` 会再回一条当前值 | 仅代码审查 | 脚本连上后连续两次 `POUT`，应收到两条 `OUTPUT` |
| `/io reload` | 运行期重读 `io.txt`，`-i` 下拒绝 | **控制台已验**（`Reloaded 2 IO group(s).`） | **游戏内**使用；已编译时是否按规则先退出编译态 |
| `/tick freeze|unfreeze|step|rate` | `freeze` = `rtps 0`、`unfreeze` 恢复冻结前值、`step` 只收偶数、`rate` 空参设 20 | **控制台已验**（freeze/unfreeze/rate/step 奇偶） | **游戏内**使用；`/tick rate` 空参 = 20、与 `/rtps` 空参的区别 |
| `//schem load|save|list` | `load/save` 复用 `//load`/`//save`，`list` 列 `./schems` | 未验证 | 三个子命令各跑一次，`list` 应列出 `mul.schem`、`cca.schem` |
| `io.txt` 输入白名单加按钮 | 输入组可放石按钮；写 `1` 脉冲、写 `0` 无为 | 未验证 | 放一个按钮当输入点位，`io.txt` 能加载；`INPUT` 写 1 产生一次脉冲 |
| 同一点双角色禁止 | 同一按钮不能既当 `clk` 又当数据点位 | 未验证 | 故意让同一按钮兼任，应报错禁用该组 |
| 启动自动初始化 | 建 `memory/`、写 `.rom.txt`/`.ram.txt`、生成 `io.txt`（版本行 + 两个空组） | **临时目录已验**（三个文件生成、`io list` 报 2 组 0 错误） | 在你自己的运行目录首次启动时的表现 |
| `io.txt` 空组合法 | `width = 0` 且无坐标的组不再报错 | **临时目录已验** | 正式使用中确认不会误报 |
| 只写版本号的首行 | `v0.5.3-alpha`（不带 `hex words addressed`）也能加载 | **临时目录已验** | 你的运行目录 `io.txt` 首行就是这种写法，确认加载正常 |

---

## 三、文档类（不需要功能验收，但需要你确认口径）

| 文档 | 内容 | 状态 |
| --- | --- | --- |
| `docs/manual_zh.md` / `manual.md` | 完整手册（含 `/coord`、`betterio`、命令表、排错） | 已写入并复制到共享存储，**未逐条校对** |
| `CHANGELOG-0.5.3-alpha.md` | 更新说明 + 后续修复 6 项 | 已写入，未校对 |
| `docs/protocol*.md` | tick 语义（差值 +1）、重复 `POUT`、轮询需绑永不触发的时钟 | 已写入，未校对 |
| `docs/file-formats*.md` | `we`/`re` 单坐标、`bwe` 宽松、`bwe` 仅 `we=1` 生效 | 已写入，未校对 |

---

## 四、已知但**不打算改**的行为（如果你认为不对，请明确指出）

| 行为 | 说明 |
| --- | --- |
| `/coord` 不自动改 `width` | `width` 由文件写死，`new` 覆盖时也不更新；写少了允许，编译时点数与 `width` 不符会禁用该组/文件 |
| `io.txt` 文件级禁用时不显示 | `/coord in` 的候选里不会出现整文件被禁用的 `io.txt`（组级错误不受此限） |
| 派发未改成单一来源 | 补全已由节点表驱动，但指令派发仍是 `match` 分支 |
| 未打 tag、未发 Release | 0.5.3-alpha 只有源码与本地二进制 |

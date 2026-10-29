# 0.5.4-alpha 待实机验收清单

版本：0.5.4-alpha　日期：2026-10-29
旧的 0.5.3 清单见 `UNVERIFIED-0.5.3-alpha.md`（未完成项继续沿用）。

## 一、版本号

| # | 检查 | 结果 |
| --- | --- | --- |
| V-01 | `/version` 显示 `mchprs_core v0.5.4-alpha` 与新的 git hash | ☐ |
| V-02 | 自动化桥 `HELLO` 回到 `HELLO MCHPRS_IO 0.5.4-alpha <tick>` | ☐ |
| V-03 | 新生成的 `io.txt` 版本行是 `v0.5.4-alpha` | ☐ |
| V-04 | 新生成的 `memory/.rom.txt`、`.ram.txt` 首行是 `v0.5.4-alpha hex words addressed` | ☐ |
| V-05 | 0.5.3 的旧 `io.txt` / 旧 memory 文件仍能加载（最低版本 0.5.0） | ☐ |

## 二、`width = 0` 的端口

| # | 检查 | 结果 |
| --- | --- | --- |
| W-01 | `width = 0` + `clk` 指向电路里的按钮 → 编译后**未被禁用** | ☐ |
| W-02 | 对它发 `INPUT <组名> 0` → 回 `OK INPUT <组名> <tick>`，时钟按钮确实被按一次 | ☐ |
| W-03 | 发 `INPUT <组名> 1` 或 `00` → 回 `ERR hex` | ☐ |
| W-04 | `POUT <组名>` 的绑定确认是 `OUTPUT <组名> <tick> 0` | ☐ |
| W-05 | 时钟每来一次上升沿推一条 `OUTPUT <组名> <tick> 0`（数据不变也推） | ☐ |
| W-06 | `width = 0` 且 `clk = 0, 0, 0` → 编译后被禁用，`INPUT`/`POUT` 回 `ERR disabled` | ☐ |
| W-07 | `/io list` 的 `disabled` 列表里能看到 W-06 那个组 | ☐ |
| W-08 | 普通组（位宽 > 0）的 hex 行为没变：`width = 4` 仍是 1 位 hex、`width = 8` 仍是 2 位 | ☐ |
| W-09 | `matrix:` 组的 hex 位数与打包规则没变 | ☐ |

## 三、文档

| # | 检查 | 结果 |
| --- | --- | --- |
| D-01 | 仓库里已无 `docs/manual.md` / `manual_zh.md`，README 无死链 | ☐ |
| D-02 | `file-formats*` 的 3.3 时钟节内容与实际行为一致 | ☐ |
| D-03 | `protocol*` 的 `width = 0` 说明与实际回执一致 | ☐ |

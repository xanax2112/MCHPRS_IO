# MCHPRS_IO 0.5.4-alpha 更新说明

日期：2026-10-29

## 一、版本号

- 内核与协议版本均为 **0.5.4-alpha**：`/version` 输出 `mchprs_core v0.5.4-alpha`，自动化桥的 `HELLO MCHPRS_IO 0.5.4-alpha` 同步变化。
- 启动时生成的 `io.txt`、`memory/.rom.txt`、`memory/.ram.txt` 模板版本行改为 `v0.5.4-alpha`。
- 文件格式的最低版本仍是 **0.5.0**，0.5.0 及以后的旧文件照常加载，不会因为版本行旧而被拒绝。

## 二、新规则：`width = 0` 的端口

`io.txt` 里 `width = 0` 的组现在有明确定义：

- **`width = 0` + `clk` 写了真实坐标 = 合法端口**，没有数据位，只有时钟有意义：
  - `INPUT <组名> 0`：`<hex>` 固定写 `0`，含义是"按一次按钮"，只脉冲一次 `clk`；
  - `OUTPUT`（含 `POUT` 绑定确认那一帧）：`<hex>` 固定为 `0`，只在 `clk` 上升沿推送。
- **`width = 0` + `clk = 0, 0, 0`（没有时钟）= 无意义**，编译后校验时整组禁用，reason 为
  `` `width = 0` needs a clock (`clk = x, y, z`)... ``；`INPUT` 与 `POUT` 都回 `ERR disabled`。

顺带修掉的问题：此前位宽 0 的组推送出去是 `OUTPUT <名字> <tick> `（hex 为空、行尾一个空格），按空白分词的脚本端根本解析不了。现在固定是 `0`。

## 三、文档

- **删除** `docs/manual.md` 与 `docs/manual_zh.md`（与 `file-formats*` / `protocol*` / `README*` 内容重复），README、更新说明里的指向一并清理。
- `docs/file-formats.md` / `file-formats_zh.md` 新增 **3.3 时钟 `clk`** 专节（原 3.3–3.7 顺延为 3.4–3.8）：三种写法、ROM 有无时钟的差异、RAM 上升沿的处理顺序、时钟必须是**输出类元件**（红石灯 / 铁活板门 / 音符盒），以及"两个存储器不得共用时钟""与 io 组共用时钟时只禁用 io 组"等约束。
- `file-formats*` 与 `protocol*` 补上 `width = 0` 端口的 hex 规则与禁用规则。

## 四、兼容性与行为变化

- 老的 `io.txt` / `memory/*.txt` 不需要改。
- 唯一的行为变化：**如果某个 `width = 0` 的 io 组没有时钟，升级后它会在编译后被禁用**（以前是静默加载、永不推送）。这属于新增的显式规则，`/io list` 会把它列在 `disabled` 里。

## 五、未做

- 未打 tag、未发布 Release。

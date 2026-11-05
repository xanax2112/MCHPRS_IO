# MCHPRS_IO Python 测试工具 · 使用手册

一个入口，用 `mode` 选两种模式之一，配合 MCHPRS_IO 的自动化协议（默认 `127.0.0.1:25585`）验证红石电路。

```
python/
├── main.py          入口：mode 选 correctness / pipeline
└── core/            内部实现，不用打开
```

---

## 一、前置条件（都要手动做）

1. **服务器在跑**（App 里启动，或 Termux 里跑 `~/mchprs/mchprs`）。
2. **电路已编译**：游戏里执行 `/redpiler compile`。未编译时 `HELLO` 不带 tick，脚本会直接提示退出。
3. **推进速度**：建议 `/rtps 5000`（脚本没有改 rtps 的权限，只会在启动时检查并提示）。
4. **`mode = "pipeline"` 额外要求**：先执行 `/tick seq`（独占脚本、`rtps` 自动变 0）。
   每次重跑前都要重新 `/tick seq`。

> `mode = "correctness"` 只能在普通模式（`rtps > 0`）下跑；
> 在 `/tick seq` 冻结状态（`rtps = 0`）下电路不自己走，结果会全错。

---

## 二、配置区（`main.py` 顶部：全局区 + 两个模式区）

```python
PIN = "add_in"        # 输入组名（io.txt 里 input 的名字）
POUT = "add_out"      # 输出组名（io.txt 里 output 的名字）
IN_BITS = 17          # 输入位宽
OUT_BITS = 9          # 输出位宽


def test(x):
    """x 是输入整数，返回期望输出整数。"""
    cin = (x >> 16) & 1
    a = (x >> 8) & 0xFF
    b = x & 0xFF
    return a + b + cin
```

- `test(x)` 收**整数**、返回**整数**，直接用位运算写电路逻辑（`>>`、`&`、`^`、`<<`、`+`）。
- 输入按 `IN_BITS` 裁剪，期望值按 `OUT_BITS` 裁剪，hex 编解码由程序完成。
- 组名、端口、位宽都不写死在代码里，换电路只改这一段。

### 配置项（`main.py` 顶部，全部是普通变量）

| 变量 | 含义 | 默认 |
| --- | --- | --- |
| `mode` | `correctness` / `pipeline` | `correctness` |
| `host` / `port` | 服务器地址 | `127.0.0.1` / `25585` |
| `total` | 总共跑多少组 | `2**20` |
| `batch` | 每批多少组（预约表上限 65536），同时是进度打印间隔 | `32768` |
| `coverage` | `random` / `full`（从 `start_value` 起连续）/ `specified` | `random` |
| `seed` / `start_value` / `specified` | 随机种子 / full 起始值 / 指定列表 | `0` / `0` / `[]` |
| `wait_seconds` | `correctness`：每组等待；`pipeline`：批内空闲超时 + 批末等尾巴的时长 | `0.05` |
| `delay` | 元件延迟（tick） | `27` |
| `interval` | 相邻输入间隔（tick，≥2） | `3` |
| `svg` | `off` / `error` / `True` | `off` |

> `pipeline` 的批内超时是**空闲超时**：`WAIT_SECONDS` 内还有回复/输出就一直等，不会因为 `batch` 变大而误判。

---

## 三、两个测试怎么判

### 1. `mode = "correctness"` — 无时序正确性

```
发 INPUT → 开始计时 wait_seconds（期间只保留 OK INPUT 和最后一条 OUTPUT）
→ 到点发 POUT 取当前值 → 判值
→ 有那条 OUTPUT 时记 out_tick - in_tick + 1 作为本组 delay
```

- 值一律以 POUT 为准；OUTPUT 只用来算延迟。
- 判值为主，不要求“只允许一条推送”。
- 结尾打印 `延迟统计：delay = lo ~ hi（中位 m），延迟固定 / 延迟随输入变化`。
- 速度由 `wait_seconds` 决定。

### 2. `mode = "pipeline"` — 流水时序精确（仅 `/tick seq`）

```
按 interval 间隔连续预约输入 → RADV 推进 → 校验
每个输出落在 in_tick + delay − 1
相邻输出间隔 == interval
每个值正确
```

- 验证流水线不阻塞、间隔不乱。
- 每批 `batch` 个数据，速度受 `RADV` 推进速度限制（约 18 组/s，间隔 3t 时）。

---

## 四、输出怎么看

进度行：

```
  [ 25.00%] 125/500  通过 125 失败 0  62.3 组/s
```

失败时打印当前组和上一组，便于复现：

```
✗ 第 12 组  输入 0x08996
   当前: 期望 0x11f，实得 0x01f  in_tick=137544 out_tick=137547  毛刺：...
   上一组: 输入 0x10530
           期望 0x036，实得 0x036
```

退出码：`0` 全过，`1` 有失败，`2` 环境问题（连不上 / 未编译 / 模式不对）。

---

## 五、日志

每次运行会在 `log/` 下生成一份日志，文件名 `测试名_日期_时间.log`，内容与终端输出完全一致：
时间、`PIN`/`POUT`、位宽、连接信息、进度、失败明细、汇总。排查时直接看日志即可。

---

## 六、协议要点（排查时用）

- 动词全大写；`/` 开头一律 `ERR unknown`。
- 位序：`io.txt` 列表**第一个坐标是最显著位**。
- 真实延迟 = `输出 tick − 输入 tick + 1`。
- 无 `clk` 的输出组**只在值变化时推送**。
- 单行报文上限 65536 字节；积压超 1 MiB 会断线。

错误码：`unknown` / `arg` / `name` / `disabled` / `nocompile` / `range` / `hex` / `mode` / `full` / `not_output` / `no_io` / `line` / `encoding`。

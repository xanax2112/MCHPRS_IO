# 自动化测试速度：从 80 组/秒 到 25 万组/秒

> ⚠ **本文所有数字已作废（2026-11-04 21:44 用户判定，见 `BACKLOG.md` 文首声明）**
> 文中引用的实测值（80 / 4,527 / 250,567 / 95,719 / 130,991 组/s、0.221 ms、0.155 s 等）
> 全部标记为不可信，**不得再作依据**，需要按新口径重测。
> 保留下来的只有**机制解释**（线程模型、批量预约、采样缓存、回复量），这部分与具体数字无关，仍然成立。

> 对象：MCHPRS_IO 的自动化桥（`crates/core/src/automation.rs`、`crates/core/src/plot/mod.rs`）
> 测试电路：`add_in`（17 位）→ `add_out`（9 位）加法器，延迟 4 tick
> 配置：`io_mode = exact`、`/tick seq`、`RADV`，每组间隔 2 tick
> 数据来源：`BACKLOG.md` §9.1 / §9.2 / §9.3 的实测记录（2026-11-03 / 11-04）

---

## 0. 一句话结论

**不是把某个函数写快了 3000 倍，而是换掉了"一次只能有一个请求在路上"的结构。**

旧架构的瓶颈不在 CPU，而在**延迟**：每个测试向量都要走一趟完整的"用户态 → 主循环 → 工作线程 → 主循环 → 用户态"，全程只有一个请求在飞，单趟 12.5 ms，于是吞吐被钉死在 `1 / 0.0125 ≈ 80 组/秒`。

新架构做的是三件事，每件砍掉一层不同的天花板：

1. **协议栈下沉到 plot 线程**：往返路径从"两跳消息队列 + 两次主循环窗口"变成"同线程内一次 poll"，单趟 12.5 ms → 0.221 ms。
2. **预约表批量灌入**：把 65536 个请求同时放上路，延迟被摊薄到 `65536 / 0.262 s`，吞吐从"1 / 延迟"变成"批量 / 延迟"。
3. **削掉每 tick 的常数开销和回复量**：IO 采样 0.65~1.0 µs/tick → 0.15~0.25 µs/tick；65536 条 `OK INPUT` 的 1.7 MB 回复 → 1 条。

---

## 1. 三个数字，先对齐口径

"从 80 到 25 万"是不同口径的数字，混着比会算错账：

| 口径 | 单组成本 | 吞吐 | 含义 |
| --- | --- | --- | --- |
| 旧：逐组串行往返 | 12.5 ms | **≈ 80 组/秒** | 一问一答，全程只有一个请求在飞 |
| 新：逐组串行往返 | 0.221 ms | **4,527 组/秒** | 结构改完，但还没批量发 |
| 新：批量预约 + `RADV`（纯模拟段） | 4.0 µs | **250,567 组/秒** | 65536 组 / 0.262 s，客户端只收不算 |
| 新：全组合扫描（端到端） | 10.4 µs | **95,719 组/秒** | 131072 组 / 1.37 s，含灌入、起停、解析 |

所以：

- **80 → 4,527（56 倍）** 是架构改动（§9.1）的功劳；
- **4,527 → 250,567（55 倍）** 是批量预约的功劳；
- **250,567 → 95,719** 的落差是客户端（Python 逐行解析）和脚本起停的固定成本，不是服务端变慢。

25 万组/秒 是"服务端能把红石喂多快"，9.5 万组/秒 是"这套 Python 工具链能测多快"。

---

## 2. 原理一：延迟决定吞吐（旧架构为什么卡在 80）

先建立一个基本关系，排队论里叫 Little 定律：

```
吞吐 = 在途请求数 / 单次延迟
```

旧架构的请求路径是这样的（`automation.rs` + `server.rs`）：

```
脚本 ──TCP──> 主线程（accept / 读 socket / 解析）
                 │  mpsc 第一跳
                 ▼
              plot 线程（tick 循环，定期收 inbox）
                 │  mpsc 第二跳
                 ▼
              主线程（encode 回复 / 写 socket）──TCP──> 脚本
```

问题在于**两个线程的节奏是各自独立的**：

- 主线程每轮 `loop` 睡 `2 ms`（后来 §9.1 改成 `5 ms`），醒来才看一次 socket；除此之外还要应付世界发包（`world_send_rate` 限速、`batch_interval` 上限 50 ms），发包正在跑的时候根本没空看 socket；
- plot 线程在 tick 循环里**顺便**看一眼 inbox，全速跑批时这一眼要等一个 64 tick 的计数周期才轮到（110 tps 下 0.58 s）。

两个齿轮互不对齐，一次往返就得凑齐"主线程醒来"和"plot 线程正好看一眼"两个时刻。实测平均 **12.5 ms**：

```
1 / 0.0125 s = 80 组/秒
```

**关键点：这 12.5 ms 里，真正算红石的时间只有 2 个 tick（几百纳秒），其余 99% 在等调度。**
这就是为什么"优化模拟算法"对测试速度毫无帮助 —— 瓶颈根本不在那里。

---

## 3. 原理二：把整条路径塞进一个线程

§9.1 做的是**把 automation 的所有权直接交给 plot 线程**：

```rust
// server.rs：io_port 仍在主线程 bind（失败就禁用 automation，不 panic）
let automation = AutomationServer::bind(io_port);
...
// 成功后整体 take() 走，随 Plot::load_and_run 进入 plot 线程
plot.automation = automation;
```

于是 accept、读 socket、解析、执行、编码、写回**全在 plot 线程上完成**，`Message::IoResponse` / `PrivMessage::AutomationRequest` 这些跨线程消息全部删除（`server.rs` 里只剩一行 `use`）。

新路径：

```
脚本 ──TCP──> plot 线程（同一时刻：poll → 解析 → 改 IO → 算 tick → 编码 → write）
```

配套的四条细节（缺一条速度就掉回去）：

**(a) 200 µs 的 poll 片。** 热循环里不能每个 tick 都查 socket，也不能等太久。用 `automation_slice` 计数器，每 64 tick（或 `update()` 每轮）取一次墙钟：

```rust
fn automation_pump(&mut self) {
    let now = Instant::now();
    if !self.automation_busy
        && now.duration_since(self.automation_last_poll).as_micros() >= 200
    {
        self.automation_last_poll = now;
        self.poll_automation();
    }
    ...
}
```

所以**串行往返的理论下限就是 200 µs**，实测 0.221 ms —— 和这个片长严丝合缝，说明剩下的开销只有解析和写回。想把串行再压下去，只能缩短这个片（代价是全速跑批时 `Instant::now` 的调用变密）。

**(b) 回复不回 5 ms 队列。** `poll_automation()` 处理完请求**立刻 flush**：

```rust
// 等 5 ms 片再发，就是给每次往返加 5 ms 地板（BACKLOG.md/§9.1）
if !self.automation_busy {
    server.flush();
}
```

这一条曾经把串行吞吐卡在 ~200 组/秒 —— 因为 5 ms 一冲，`1/0.005 = 200`。删掉后直接跳到 4,527。

**(c) 热路径不取时间。** 全速时 tick 短到几十纳秒，每个 tick 一次 `Instant::now()`（约 20~25 ns）会吃掉 5% CPU。用 `% 64` 计数器把它摊薄：

```rust
self.automation_slice = self.automation_slice.wrapping_add(1);
if self.automation_slice % 64 != 0 { return; }
```

代价是 110 tps 下 poll 间隔变成 0.58 s —— 所以 `update()` 里**无条件**再调一次 `automation_pump()`，保证慢速/冻结状态也有 200 µs 的响应。

**(d) 空闲不空转。** plot 没有玩家、也没有在编译时，循环里加 `sleep(1ms)`。之前没有这一句，空闲 plot 会把一个 8 核 SoC 的一核吃到 100%，连 sshd 都握手不上（表现为 SSH "banner exchange" 超时，一断十分钟）。

---

## 4. 原理三：把延迟从关键路径上挪走（批量预约）

架构修好后单趟 0.221 ms，但**一个请求在飞**这个限制还在 —— 吞吐 = 1 / 0.221 ms = 4,527。想再快，只能让更多请求同时在路上。

`/tick seq` 模式提供了这个条件：

- `rtps = 0`，tick 不再由墙钟驱动，**只由 `RADV n` 推进**；
- tick 号因此是确定的（`HELLO` 拿基准 + 脚本自算偏移）；
- 于是脚本可以**提前**把"第几个 tick 哪个组写什么"全部交给服务器，然后一句 `RADV 131072` 让它闷头跑。

```
旧：INPUT → 等 OK → RADV 2 → 等 OUTPUT → INPUT → ...      （N 趟往返）
新：65536 × INPUT ← 一次写进 socket
    → 等 OK INPUT（1 条，见 §6）
    → RADV 131072 ← 一次
    → 边读 65536 行 OUTPUT 边等 OK RADV
```

往返次数从 **O(N) 降到 O(1)**。延迟还是 0.262 s，但它现在覆盖的是 65536 组而不是 1 组：

```
65536 组 / 0.262 s = 250,567 组/秒
```

对应的小学算术：**吞吐 = 在途数 / 延迟**。旧架构在途数 1，新架构在途数 65536。

### 4.1 但队列本身会变成新瓶颈

批量化的前提是"表能撑住"。旧实现是 `Vec<Reservation>` + 每 tick 全表 `retain` 再逐条 `clone`：

```
655360 tick × 65536 条 ≈ 4.3 × 10^10 次遍历
```

按每次几纳秒算，这是分钟级的开销 —— 批量化之后立刻撞上。§9.3 的改法：

- **单调约束**：预约必须按 tick 单调不减发送，违反回 `ERR order`。有了单调性，表天然有序，出队是游标 `O(1)`：

```rust
while let Some(front) = self.reservations.front() {
    if front.tick > tick { break; }          // 后面的肯定更晚，不用看
    let reservation = self.reservations.pop_front().unwrap();
    ...
}
```

- **`VecDeque` + 尾部扫重**：`tick == last_tick` 时只从尾部往前扫同 tick 的几条（保"后到覆盖"语义），不再全表查重。
- **上限 255 → 65536**：每条约 170 字节（结构体 56 + 组名堆 32 + 位堆 80），满表约 11 MB。

插入摊还 O(1)，应用 O(k)，k 是该 tick 的条目数（正常是 1）。

---

## 5. 原理四：每 tick 的常数开销，在全速时就是全部

`io_mode = exact` 时，**每个 tick 都要采样一次输出组**（这是"输出行描述的正是它名字那个 tick 的末态"这个语义的来源，不能省）。所以：

```
每 tick 采样成本 × 每秒 tick 数 = CPU 占比
```

改之前的实测基线（64 位组、绑了 `POUT`、每 tick）：

| 动作 | 成本 |
| --- | --- |
| `Vec::with_capacity` 堆分配 | 40~80 ns |
| 64 次 `query_io` → `pos_map` 的 `FxHashMap` 查找 | **500~750 ns** |
| `prev_bits != bits`（memcmp 64 字节） | 5~10 ns |
| `bits.clone()` 分配 + 拷贝 | 50~100 ns |
| **合计** | **0.65~1.0 µs** |

一眼就能看出大头是哈希查找：**每个 tick、每个坐标、查一次 `pos_map`**。50 万 tick/s 时，1 µs/tick 就是 50% 的 CPU 白烧在"找节点"上。

§9.2 的三条改动：

1. **节点 id 缓存（省 60~70%，是大头）**：绑定时把每个坐标的 `NodeId` 解析一次存进 `IoGroup::handles`，之后直接下标取值。为此后端新增了按 `NodeId` 读写的能力：

```rust
// crates/redpiler/src/backend/mod.rs
fn resolve_io_node(&mut self, pos, kind) -> Option<u32>;
fn node_state(&self, id) -> Option<(u32, bool)>;
fn node_wire_power(&self, id) -> u8;
fn set_node_powered(&mut self, id, powered: bool);
fn set_node_wire_power(&mut self, id, power: u8);
```

2. **缓冲复用（省约 15%）**：`IoGroup` 加 `scratch: Vec<bool>`，`poll` 用 `prev_bits != scratch` 判定变化，命中后 `std::mem::swap` —— 每 tick **零分配、零拷贝**。原来的"只在变化时 clone"因此被 `swap` 覆盖，不再单独实现。

3. **写入路径同改**：`try_set_powered` / `try_set_wire_power` 原来每点一次哈希查找，改用缓存 id。

收益：**0.65~1.0 µs/tick → 0.15~0.25 µs/tick（约 -75%）**。

> 风险（改码时已处理）：`NodeId` 只在一次编译内有效。重新编译、`/redpiler reset`、`crash_save` 重载后必须重新解析；坐标已不存在时把该组标 `active = false`，绝不能拿旧 id 去读 —— 那是静默错数据，比崩溃更难查。

---

## 6. 原理五：回复量也是吞吐的一部分

批量化之后暴露出来的下一个瓶颈在**回程**，而且跟模拟完全无关：

```
65536 条 OK INPUT <name> <tick> ≈ 1.7 MB
客户端逐行解析：0.155 s（43 万条/秒）
而整轮 RADV 13 万 tick 只要 0.262 s
→ 光读 OK 就占了整轮时间的 37%
```

这正是"服务端 250k、端到端 95k"落差的一大来源。

§9.3 的方案是**按静默判定批次**：

- 收到 `INPUT <name> <offset> <hex>` 只入队并计数，**不回任何东西**；
- 连续 **5 ms** 没再收到新的预约，视为这批结束，回**一条** `OK INPUT <name> <条数>`；
- 出错仍然**立即**回 `ERR <code>`，不受 5 ms 影响；
- 任何**非 INPUT** 请求（`RADV` / `TICK NOW` / `READ` / `POUT`）到达时，先结算未回的批次 —— 保证回复顺序不乱。

**1.7 MB / 0.155 s → 1 条回复，基本归零。** 代价是每批固定多 ~5 ms 延迟（在 0.262 s 的批里占 2%）。

条数回显不是装饰，它是**完整性检查**：脚本发出的行数应等于所有 `OK INPUT` 的条数之和，对不上说明有丢失，或发送中途停顿被切成了两批。用户 2026-11-04 明确要求：**先核对条数，再发 `RADV`**，不能把两者一起发出去。

> 顺带一提：早期设想的"把 `RADV` 提前发去掩盖读 65536 个 `OK` 的等待"因此不再需要 —— 没有可重叠的东西了。

---

## 7. 为什么没有到 135 万组/秒

理论上限是有的：不编译时全速约 270 万 tick/s，`interval = 2` → 135 万组/秒。实测纯模拟段 250,567 组/秒（50 万 tick/s），差 5 倍。剩下的时间去哪了：

| 消耗 | 说明 |
| --- | --- |
| 每 tick 采样 | `exact` 模式必付，已从 1 µs 压到 0.2 µs，但在 50 万 tick/s 下仍占 ~10% |
| `tickn` 本身 | 红石模拟本体，不可省 |
| 客户端解析 `OUTPUT` | 65536 行 hex 逐行 `split` + `int()` —— **250k 这一档就是客户端 bound** |
| `sample_io_groups` 的残余分配 | 每 tick 还在 `Vec<String>` + `String::clone` 组名（`bound` 集合），下一轮可改成长度固定的 `Vec<u16>` |
| 脚本起停 / 灌入 / 日志 | 端到端 95k 那一档的主要成本 |

**瓶颈迁移路径**（这是整件事最值得记的一点）：

```
80/s      → 卡在调度往返（12.5 ms 里 99% 在等）
4,527/s   → 卡在 200 µs 的 poll 片
250,567/s → 卡在每 tick 采样 + 客户端解析
95,719/s（端到端）→ 卡在 Python
```

每修好一层，下一层才会露出来。所以"为什么快"的正确答案不是"哪段代码优化了"，而是**"哪一层天花板被换掉了"**。

---

## 8. 复现

```sh
# 1. 服务端起在测试世界（Unix 侧）
cd ~/mchprs/source/MCHPRS_IO
nice -n 10 ./target/release/mchprs /storage/emulated/0/termux/MCHPRS_IO

# 2. 控制台输入一次（之后独占权自动续接，不必每次重敲）
/redpiler compile
/tick seq

# 3. 跑批（total / batch 在脚本顶部配置区改）
python3 python/pipeline.py        # 每组判值 + 判间隔，报告 组/s、tick/s
```

`Config.toml` 关键项：`io_port = 25585`、`io_mode` 默认 `exact`、`betterio = true`。

---

## 9. 附：这次改动的清单

| 层 | 改动 | 文件 |
| --- | --- | --- |
| 架构 | automation 所有权移交 plot 线程；删 6 个跨线程消息/枚举 | `automation.rs`、`server.rs`、`plot/mod.rs` |
| 调度 | 200 µs poll 片、64 tick 计数器、5 ms flush、回复立即 flush、空闲 `sleep(1ms)` | `plot/mod.rs` |
| 队列 | `MAX_RESERVATIONS` 255 → 65536、单调约束、`VecDeque` 游标出队、尾部扫重 | `plot/mod.rs` |
| 采样 | `NodeId` 缓存、`scratch` 复用 + `swap`、写入路径同改 | `io.rs`、`memory.rs`、`redpiler/*` |
| 协议 | `MAX_PENDING` 1→16 MB（全局 32 MB）、`INPUT` 带 `u32` 偏移、`OK INPUT <name> <n>` 批次回复 | `automation.rs` |
| 工具 | 流水线改 `total`/`batch` 分批循环、一次性发完再收 OK、per-chunk 进度 | `python/core/runner.py` |

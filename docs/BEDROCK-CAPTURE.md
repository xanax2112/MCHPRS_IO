# 基岩版取证与抓包（两条通道）

> 配套：`BACKLOG.md` 的 BUG-032 / §9.x（结论）、`docs/BEDROCK-AGENT-NOTES.md`（边界与地图）。
> 本文只讲**怎么取证**：遇到"客户端表现和我们的预期不一致"时，证据从哪来、怎么落地。
> 所有工具都在仓库外（`D:\computer\mchprs\bedrock\refs\`），**不进仓库、不进构建**。

## 0. 顺序（别跳步）

1. 先读 **Geyser 源码**（`refs\Geyser-<hash>\core\src\main\java`）看它怎么做 —— 唯一权威；
2. 再跑 **Cloudburst 探针**拿"权威字节"（离线、秒级）；
3. 需要"真实客户端/真实服务端"时，才上 **Geyser packet-spy 抓包**；
4. 结论写进 `BACKLOG.md`（BUG-032 第 N 条 / §9.x），**并把关键字节固化成 `#[test]`**，
   免得下次又要重新取证。

## 1. 通道 A：Cloudburst 探针（离线、权威字节）

**为什么可信**：`refs\geyser-deps` 里就是 Geyser 自己用的 Cloudburst v766 codec，
`tryEncode` 出来的字节和真实 Bedrock 服务端一致（本项目多次逐字节对拍验证过）。

**一键编译运行**（唯一入口，别再手拼 217 个 jar）：

```powershell
$probe = "D:\computer\mchprs\bedrock\refs\tools\probe.ps1"
& $probe -List                                   # 有哪些探针
& $probe ProbeSign                               # 编译 + 运行
& $probe ProbeItemStackResponse                  # 同上
& $probe ProbeGeyserChunk -WithGeyserClasses     # 需要 Geyser 自己的类时加这个开关
& $probe ProbeSign -NoRun                        # 只编译不运行
```

新增探针：在 `refs\probes\` 写一个 `public class ProbeXxx { public static void main(...) }`，
用 `Bedrock_v766.CODEC.tryEncode(...)`（或 `tryDecode`）打印 hex，然后 `& $probe ProbeXxx`。

**已知探针产出**（结论都在台账，代码里引用了它们）：

| 探针 | 用来钉住什么 |
| --- | --- |
| `ProbeEntityFlagOrder` | 实体 `FLAGS` 的位号（配合真机验证低位区 +1） |
| `ProbeInventory` / `ProbeCraftCreative` / `ProbeItemRequest` | 背包容器包、创造取物、`ItemStackRequest` 字段宽度 |
| `ProbeScoreboard` | 侧边栏 `SetDisplayObjective` / `SetScore` 的排序与分数编码 |
| `ProbeSoftEnum` | 软枚举参数符号 `0x04100000 \| 下标` |
| `ProbeGeyserChunk` | 单值子区块 / 生物群系段的字节 |
| `ProbeSign` | `BlockActorData`（牌子）字节 |
| `ProbeChunkNbt` | 区块尾部是**裸网络 NBT 串**（无长度前缀） |
| `ProbeItemStackResponse` | `ItemStackResponsePacket`：`result \| requestId \| containers` 顺序、`Success` 才带容器 |

## 2. 通道 B：Geyser packet-spy 抓包（真机、对照）

用于"我方实现和 Geyser 行为不一致"的时候：让同一个客户端走 Geyser，抓 Geyser 实际发什么。

- Geyser 源码 checkout：`refs\Geyser-<hash>`（**只在它自己的目录里改**，用 `refs\patches` 里的补丁思路）；
  补丁细节与历史记录见 `BACKLOG.md` §9.1 附近。
- 运行目录：`run-capture\`（Geyser 的 config 副本）、`run756\`。
- **抓完立刻把原始日志/hex 结论摘进台账**，日志文件本身放 `logs\`（会被归档，不进仓库）。

## 3. 证据落地规矩

- 任何"客户端接受/拒绝某字节"的结论，必须能指向：探针输出 + `#[test]`（最好）或真机日志行。
- 逐字节回归测试放 `crates/network/src/bedrock/protocol.rs`（或其他对应模块的 `tests`），
  命名 `*_matches_cloudburst_bytes` / `*_decodes_cloudburst_bytes`。
- **不写"猜"的常量**；探针还拿不到就上真机，真机和 codec 冲突时**以真机为准并记账**
  （例：`FLAGS` 低位区偏移，见 `protocol.rs::CLIENT_LOW_FLAG_SHIFT`）。
